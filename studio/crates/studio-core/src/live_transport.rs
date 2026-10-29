use crate::{
    comfy::Comfy,
    live::{decode_preview, LiveJob},
    Database, Result,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    net::{Shutdown, SocketAddr, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tungstenite::{protocol::WebSocketConfig, Message};

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveState {
    pub revision: u64,
    pub connected: bool,
    /// Single short banner shown while the engine is unreachable. Raw request
    /// details stay in `error`/`http_error`; the UI shows this sentence alone.
    pub offline_note: Option<String>,
    pub client_id: String,
    pub reconnects: u32,
    pub retry_ms: u64,
    pub error: Option<String>,
    pub http_error: Option<String>,
    pub last_http_sync: Option<u64>,
    pub jobs: BTreeMap<String, LiveJob>,
}
pub const OFFLINE_NOTE: &str =
    "ComfyUI hors ligne — les essais restent en file, reprise dès qu'il revient.";
pub struct Monitor {
    owner: Option<std::fs::File>,
    state: Arc<Mutex<LiveState>>,
    stop: Arc<AtomicBool>,
    socket: Arc<Mutex<Option<TcpStream>>>,
    ws: Option<JoinHandle<()>>,
    http: Option<JoinHandle<()>>,
}
fn edit(state: &Mutex<LiveState>, f: impl FnOnce(&mut LiveState)) {
    if let Ok(mut s) = state.lock() {
        f(&mut s);
        s.revision = s.revision.wrapping_add(1);
    }
}
pub fn backoff(attempt: u32) -> Duration {
    Duration::from_secs((1u64 << attempt.min(5)).min(30))
}
/// Polling cadence of `/queue` while the engine is unreachable: consecutive
/// failures stretch the delay (4 s → 60 s) so a closed ComfyUI is no longer
/// hammered every 5 s. `Monitor::refresh` and a WebSocket reconnection unpark
/// the worker for an immediate retry, so recovery stays fast.
pub fn offline_poll_backoff(failures: u32) -> Duration {
    Duration::from_secs(4u64.saturating_mul(1u64 << failures.saturating_sub(1).min(4)))
        .min(Duration::from_secs(60))
}
impl Monitor {
    pub fn start(directory: PathBuf) -> Result<Self> {
        Self::loopback(directory, 8188)
    }
    pub fn loopback(directory: PathBuf, port: u16) -> Result<Self> {
        let owner = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("comfy-monitor.lock"))?;
        owner
            .try_lock()
            .map_err(|_| "Cette bibliothèque est déjà suivie par une autre fenêtre Saya Studio")?;
        let client_id = Database::open(&directory.join("studio.sqlite3"))?.comfy_client_id()?;
        let state = Arc::new(Mutex::new(LiveState {
            client_id: client_id.clone(),
            ..Default::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let socket = Arc::new(Mutex::new(None));
        let (s, flag, dir) = (state.clone(), stop.clone(), directory.clone());
        let http = thread::spawn(move || {
            let run = || -> Result<()> {
                let mut db = Database::open(&dir.join("studio.sqlite3"))?;
                let comfy = Comfy::loopback(port)?;
                let mut failures: u32 = 0;
                while !flag.load(Ordering::Acquire) {
                    // Queue is fetched even with no Studio jobs: HTTP health is not inferred from WS.
                    let result = comfy.get("/queue").and_then(|queue| {
                        if !queue["queue_running"].is_array() || !queue["queue_pending"].is_array()
                        {
                            return Err("File ComfyUI invalide".into());
                        }
                        db.sync_jobs_until(&comfy, &dir.join("results"), Some(&flag))
                    });
                    let healthy = result.is_ok();
                    edit(&s, |state| match result {
                        Ok(()) => {
                            state.http_error = None;
                            state.last_http_sync = Some(
                                SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            );
                        }
                        Err(e) => state.http_error = Some(e.to_string()),
                    });
                    failures = if healthy {
                        0
                    } else {
                        failures.saturating_add(1)
                    };
                    if flag.load(Ordering::Acquire) {
                        break;
                    }
                    let delay = if healthy {
                        Duration::from_secs(5)
                    } else {
                        offline_poll_backoff(failures)
                    };
                    thread::park_timeout(delay);
                }
                Ok(())
            };
            if let Err(e) = run() {
                edit(&s, |s| s.http_error = Some(e.to_string()));
            }
        });
        let wake = http.thread().clone();
        let (s, flag, stream_slot) = (state.clone(), stop.clone(), socket.clone());
        let ws = thread::spawn(move || {
            let mut attempt = 0;
            let mut established = false;
            let run = || -> Result<Database> { Database::open(&directory.join("studio.sqlite3")) };
            let db = match run() {
                Ok(db) => db,
                Err(e) => {
                    edit(&s, |s| s.error = Some(e.to_string()));
                    return;
                }
            };
            while !flag.load(Ordering::Acquire) {
                let connected_at = Instant::now();
                let session = (|| -> Result<()> {
                    let address: SocketAddr = ([127, 0, 0, 1], port).into();
                    let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
                    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
                    *stream_slot.lock().map_err(|_| "Socket lock")? = Some(stream.try_clone()?);
                    let config = WebSocketConfig::default()
                        .max_message_size(Some(2_000_000))
                        .max_frame_size(Some(2_000_000))
                        .read_buffer_size(8192);
                    let (mut ws, _) = tungstenite::client::client_with_config(
                        format!("ws://127.0.0.1:{port}/ws?clientId={client_id}"),
                        stream,
                        Some(config),
                    )?;
                    ws.get_mut()
                        .set_read_timeout(Some(Duration::from_millis(250)))?;
                    ws.send(Message::Text(
                        json!({"type":"feature_flags","data":{"supports_preview_metadata":true}})
                            .to_string()
                            .into(),
                    ))?;
                    edit(&s, |s| {
                        s.connected = true;
                        s.error = None;
                        s.retry_ms = 0;
                        s.offline_note = None;
                        if established {
                            s.reconnects += 1;
                        }
                    });
                    established = true;
                    wake.unpark();
                    let mut ping = Instant::now();
                    let mut received = Instant::now();
                    while !flag.load(Ordering::Acquire) {
                        if received.elapsed() > Duration::from_secs(35) {
                            return Err("WebSocket silencieux depuis 35 s".into());
                        }
                        if ping.elapsed() > Duration::from_secs(10) {
                            ws.send(Message::Ping(Vec::new().into()))?;
                            ping = Instant::now();
                        }
                        let message = match ws.read() {
                            Ok(message) => {
                                received = Instant::now();
                                message
                            }
                            Err(tungstenite::Error::Io(e))
                                if matches!(
                                    e.kind(),
                                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                                ) =>
                            {
                                continue
                            }
                            Err(e) => return Err(e.into()),
                        };
                        match message {
                            Message::Text(text) => {
                                let event: Value = serde_json::from_str(&text)?;
                                let Some(prompt) = event["data"]["prompt_id"].as_str() else {
                                    continue;
                                };
                                if !db.tracked_prompt(prompt)? {
                                    continue;
                                }
                                edit(&s, |s| {
                                    if !s.jobs.contains_key(prompt) && s.jobs.len() >= 32 {
                                        if let Some(key) = s.jobs.keys().next().cloned() {
                                            s.jobs.remove(&key);
                                        }
                                    }
                                    s.jobs
                                        .entry(prompt.into())
                                        .or_default()
                                        .apply(prompt, &event);
                                });
                                db.record_execution_event(&event)?;
                                if matches!(
                                    event["type"].as_str(),
                                    Some(
                                        "execution_success"
                                            | "execution_error"
                                            | "execution_interrupted"
                                            | "executed"
                                    )
                                ) {
                                    wake.unpark();
                                }
                            }
                            Message::Binary(bytes) => {
                                if let Some(preview) = decode_preview(&bytes)? {
                                    if db.tracked_prompt(&preview.prompt_id)? {
                                        edit(&s, |s| {
                                            // Previews do not allocate another job entry without an identified event.
                                            for job in s.jobs.values_mut() {
                                                job.preview = None;
                                            }
                                            if let Some(job) = s.jobs.get_mut(&preview.prompt_id) {
                                                if !job.finished {
                                                    job.preview = Some(preview.image);
                                                }
                                            }
                                        });
                                    }
                                }
                            }
                            Message::Close(_) => {
                                let _ = ws.flush();
                                return Err("WebSocket fermé par ComfyUI".into());
                            }
                            Message::Ping(_) | Message::Pong(_) => {
                                ws.flush()?;
                            }
                            _ => {}
                        }
                    }
                    let _ = ws.close(None);
                    Ok(())
                })();
                if let Ok(mut slot) = stream_slot.lock() {
                    *slot = None;
                }
                if flag.load(Ordering::Acquire) {
                    break;
                }
                if connected_at.elapsed() >= Duration::from_secs(10) {
                    attempt = 0;
                }
                let delay = backoff(attempt);
                attempt = attempt.saturating_add(1);
                edit(&s, |s| {
                    s.connected = false;
                    s.jobs.clear();
                    s.retry_ms = delay.as_millis() as u64;
                    s.error = session.err().map(|e| e.to_string());
                    s.offline_note = Some(OFFLINE_NOTE.to_string());
                });
                // A failed WebSocket attempt must NOT unpark the HTTP worker:
                // each reconnect try would otherwise trigger an extra /queue
                // request and defeat the offline backoff. Only a successful
                // reconnection (above) and Monitor::refresh wake it.
                thread::park_timeout(delay);
            }
            edit(&s, |s| {
                s.connected = false;
                s.retry_ms = 0;
                s.offline_note = None;
            });
        });
        Ok(Self {
            owner: Some(owner),
            state,
            stop,
            socket,
            ws: Some(ws),
            http: Some(http),
        })
    }
    pub fn snapshot(&self) -> Result<LiveState> {
        Ok(self.state.lock().map_err(|_| "Live state lock")?.clone())
    }
    pub fn refresh(&self) {
        if let Some(worker) = &self.http {
            worker.thread().unpark();
        }
    }
    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Ok(socket) = self.socket.lock() {
            if let Some(socket) = socket.as_ref() {
                let _ = socket.shutdown(Shutdown::Both);
            }
        }
        if let Some(worker) = &self.ws {
            worker.thread().unpark();
        }
        if let Some(worker) = &self.http {
            worker.thread().unpark();
        }
        if let Some(worker) = self.ws.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.http.take() {
            let _ = worker.join();
        }
        self.owner.take();
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::atomic::AtomicUsize,
    };
    #[test]
    fn backoff_is_bounded() {
        assert_eq!(backoff(0), Duration::from_secs(1));
        assert_eq!(backoff(3), Duration::from_secs(8));
        assert_eq!(backoff(u32::MAX), Duration::from_secs(30));
    }
    #[test]
    fn offline_poll_backoff_stretches_and_caps() {
        assert_eq!(offline_poll_backoff(1), Duration::from_secs(4));
        assert_eq!(offline_poll_backoff(2), Duration::from_secs(8));
        assert_eq!(offline_poll_backoff(3), Duration::from_secs(16));
        assert_eq!(offline_poll_backoff(4), Duration::from_secs(32));
        assert_eq!(offline_poll_backoff(5), Duration::from_secs(60));
        assert_eq!(offline_poll_backoff(50), Duration::from_secs(60));
    }
    #[test]
    fn offline_queue_polling_backs_off_and_notes_offline() {
        let dir = std::env::temp_dir().join(format!("studio-offline-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(AtomicBool::new(false));
        let queue_attempts = Arc::new(AtomicUsize::new(0));
        let (flag, counter) = (stop.clone(), queue_attempts.clone());
        let server = thread::spawn(move || {
            let mut handlers = Vec::new();
            while !flag.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let counter = counter.clone();
                        handlers.push(thread::spawn(move || {
                            stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                            let mut buffer = [0u8; 8192];
                            let header = loop {
                                let n = stream.peek(&mut buffer).unwrap();
                                let header = String::from_utf8_lossy(&buffer[..n]);
                                if header.contains("\r\n\r\n") {
                                    break header.into_owned();
                                }
                                thread::sleep(Duration::from_millis(5));
                            };
                            if header.starts_with("GET /queue ") {
                                counter.fetch_add(1, Ordering::SeqCst);
                            }
                            // Broken engine: one clear failure per request, so the
                            // count is exact and free of client-side retry ambiguity.
                            let _ = std::io::Write::write_all(
                                &mut stream,
                                b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                            );
                        }));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
            for handler in handlers {
                handler.join().unwrap();
            }
        });
        let mut monitor = Monitor::loopback(dir.clone(), port).unwrap();
        // Window 30 s: old fixed 5 s cadence sends ~7 requests; the offline
        // backoff (4, 8, 16, 32 s) sends exactly 4 (t=0, 4, 12, 28).
        thread::sleep(Duration::from_secs(30));
        let attempts = queue_attempts.load(Ordering::SeqCst);
        assert!(
            attempts >= 2,
            "offline polling stopped retrying: {attempts}"
        );
        assert!(
            attempts <= 5,
            "queue hammered while offline: {attempts} requests in 30 s (fixed 5 s cadence would send ~7)"
        );
        let live = monitor.snapshot().unwrap();
        assert!(!live.connected);
        assert_eq!(live.offline_note.as_deref(), Some(OFFLINE_NOTE));
        assert!(live.error.is_some());
        assert!(live.http_error.is_some());
        monitor.shutdown();
        assert_eq!(monitor.snapshot().unwrap().offline_note, None);
        stop.store(true, Ordering::Release);
        server.join().unwrap();
        drop(monitor);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn mock_reconnects_with_same_identity_recovers_http_and_closes_workers() {
        let dir = std::env::temp_dir().join(format!("studio-ws-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
        let client = db.comfy_client_id().unwrap();
        let workflow = db
            .import_workflow(
                "fixture",
                r#"{"n":{"class_type":"Text","inputs":{"text":"synthetic"}}}"#,
            )
            .unwrap();
        let prompt = uuid::Uuid::new_v4().to_string();
        db.connection.execute("INSERT INTO generation_jobs(id,workflow_revision_id,seed,request_snapshot_json,state,comfy_prompt_id) VALUES('job',?1,'18446744073709551615','{}','queued',?2)",rusqlite::params![workflow,prompt]).unwrap();
        drop(db);
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(AtomicBool::new(false));
        let count = Arc::new(AtomicUsize::new(0));
        let http = Arc::new(AtomicUsize::new(0));
        let (flag, connections, gets, pid, cid) = (
            stop.clone(),
            count.clone(),
            http.clone(),
            prompt.clone(),
            client.clone(),
        );
        let server = thread::spawn(move || {
            let mut handlers = Vec::new();
            while !flag.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let (connections, gets, pid, cid) =
                            (connections.clone(), gets.clone(), pid.clone(), cid.clone());
                        handlers.push(thread::spawn(move || {
                        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();let mut buffer=[0u8;8192];
                        let header=loop {let n=stream.peek(&mut buffer).unwrap();let h=String::from_utf8_lossy(&buffer[..n]);if h.contains("\r\n\r\n"){break h.into_owned();}thread::sleep(Duration::from_millis(5));};
                        assert!(header.starts_with("GET "),"Test never allows a POST");
                        if header.starts_with("GET /ws?") {
                            assert!(header.contains(&format!("clientId={cid}")));
                            let mut ws=tungstenite::accept(stream).unwrap();
                            let flags=ws.read().unwrap().into_text().unwrap();assert_eq!(serde_json::from_str::<Value>(&flags).unwrap()["data"]["supports_preview_metadata"],true);
                            let number=connections.fetch_add(1,Ordering::SeqCst);
                            ws.send(Message::Text(json!({"type":"progress","data":{"prompt_id":pid,"node":"9","value":2,"max":8}}).to_string().into())).unwrap();
                            if number==0 {let _=ws.close(None);}else {while ws.read().is_ok(){}}
                        } else {
                            let n=stream.read(&mut buffer).unwrap();let request=String::from_utf8_lossy(&buffer[..n]);gets.fetch_add(1,Ordering::SeqCst);
                            let body=if request.starts_with("GET /queue "){json!({"queue_running":[[0,pid,{}, {},[]]],"queue_pending":[]})}else{json!({})}.to_string();
                            let _=write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body);
                        }
                    }));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
            for h in handlers {
                h.join().unwrap();
            }
        });
        let mut monitor = Monitor::loopback(dir.clone(), port).unwrap();
        let start = Instant::now();
        loop {
            let live = monitor.snapshot().unwrap();
            if live.connected
                && live.reconnects >= 1
                && live.last_http_sync.is_some()
                && live.jobs.get(&prompt).is_some_and(|j| j.value == Some(2.0))
            {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "reconnection snapshot not recovered"
            );
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(monitor.snapshot().unwrap().client_id, client);
        assert!(http.load(Ordering::SeqCst) >= 3);
        let db = Database::open(&dir.join("studio.sqlite3")).unwrap();
        assert_eq!(db.jobs().unwrap()[0]["state"], "running");
        drop(db);
        let start = Instant::now();
        monitor.shutdown();
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(!monitor.snapshot().unwrap().connected);
        stop.store(true, Ordering::Release);
        server.join().unwrap();
        drop(monitor);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn second_monitor_cannot_steal_same_client_socket() {
        let dir = std::env::temp_dir().join(format!("studio-owner-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let mut first = Monitor::loopback(dir.clone(), 1).unwrap();
        assert!(Monitor::loopback(dir.clone(), 1).is_err());
        first.shutdown();
        drop(first);
        let second = Monitor::loopback(dir.clone(), 1).unwrap();
        drop(second);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
