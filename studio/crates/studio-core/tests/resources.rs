use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use studio_core::{
    comfy::Comfy,
    resources::{measure, ComfyMemoryManager},
};

struct Server {
    port: u16,
    calls: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(queue: Value, before: Value, after: Value) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (log, done) = (calls.clone(), stop.clone());
        let handle = thread::spawn(move || {
            let mut freed = false;
            while !done.load(Ordering::SeqCst) {
                let Ok((mut socket, _)) = listener.accept() else {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(1)))
                    .unwrap();
                let mut data = Vec::new();
                let mut chunk = [0; 4096];
                loop {
                    let size = socket.read(&mut chunk).unwrap();
                    if size == 0 {
                        break;
                    }
                    data.extend_from_slice(&chunk[..size]);
                    if let Some(end) = data.windows(4).position(|v| v == b"\r\n\r\n") {
                        let headers = String::from_utf8_lossy(&data[..end]).to_lowercase();
                        let length: usize = headers
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .map(|v| v.trim().parse().unwrap())
                            .unwrap_or(0);
                        if data.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let request = String::from_utf8_lossy(&data);
                let path = request.split_whitespace().nth(1).unwrap();
                log.lock().unwrap().push(path.to_owned());
                let body = match path {
                    "/queue" => queue.clone(),
                    "/system_stats" => {
                        if freed {
                            after.clone()
                        } else {
                            before.clone()
                        }
                    }
                    "/free" => {
                        assert!(request.contains("\"unload_models\":true"));
                        assert!(request.contains("\"free_memory\":true"));
                        freed = true;
                        json!({})
                    }
                    _ => panic!("unexpected endpoint {path}"),
                }
                .to_string();
                write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
            }
        });
        Self {
            port,
            calls,
            stop,
            thread: Some(handle),
        }
    }
    fn manager(&self) -> ComfyMemoryManager {
        ComfyMemoryManager {
            client: Comfy::loopback(self.port).unwrap(),
            port: self.port,
            drm: None,
            settle: Duration::ZERO,
            threshold: 100,
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn stats(free: u64, reserved: u64) -> Value {
    json!({"system":{"ram_free":8000},"devices":[{"name":"AMD via ROCm", "type":"cuda", "index":0, "vram_total":10000, "vram_free":free, "torch_vram_total":reserved,"torch_vram_free":50}]})
}

#[test]
fn release_refuses_a_busy_or_malformed_queue_before_posting_free() {
    for queue in [
        json!({"queue_running":[[0,"busy"]],"queue_pending":[]}),
        json!({"queue_running":[],"queue_pending":[[1,"queued"]]}),
        json!({}),
    ] {
        let server = Server::new(queue, stats(1000, 500), stats(2000, 200));
        assert!(server.manager().release_measured().is_err());
        assert!(!server.calls.lock().unwrap().iter().any(|p| p == "/free"));
    }
}

#[test]
fn release_requires_a_measured_torch_reservation_drop_not_device_wide_activity() {
    let idle = json!({"queue_running":[],"queue_pending":[]});
    for (after, expected) in [
        (stats(2000, 200), true),
        (stats(2000, 500), false),
        (stats(1000, 500), false),
        (json!({"devices":[]}), false),
    ] {
        let server = Server::new(idle.clone(), stats(1000, 500), after);
        let (released, report) = server.manager().release_measured().unwrap();
        assert_eq!(released, expected, "{report}");
        assert_eq!(
            server
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|p| *p == "/free")
                .count(),
            1
        );
    }
}

#[test]
fn cpu_ram_is_never_reported_as_vram_and_reading_does_not_release() {
    let server = Server::new(
        json!({}),
        json!({"devices":[{"name":"cpu", "type":"cpu", "vram_total":2000,"vram_free":1000,"torch_vram_total":2000}]}),
        json!({}),
    );
    let client = Comfy::loopback(server.port).unwrap();
    let snapshot = measure(Some(&client), server.port, None);
    assert_eq!(snapshot.engine_vram_used(), None);
    assert_eq!(snapshot.torch_reserved(), None);
    assert_eq!(*server.calls.lock().unwrap(), vec!["/system_stats"]);
}

#[test]
fn process_tree_rss_includes_the_current_process() {
    let (total, processes) = studio_core::resources::process_tree_rss(std::process::id()).unwrap();
    assert!(total > 0);
    assert!(processes.iter().any(|(pid, _)| *pid == std::process::id()));
}
