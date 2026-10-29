#!/usr/bin/env python3
"""Local Civitai -> Saya Studio test bridge. Token never leaves this process."""
import argparse, hashlib, json, os, re, secrets, threading, time, uuid
from collections import deque
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener

PORT=47821
TOKEN_FILE=None
TOKEN=Path.home()/".config/civitai/token"

def default_download_root():
    """XDG download directory: $XDG_DOWNLOAD_DIR, then user-dirs.dirs, then ~/Downloads."""
    home=Path.home()
    env=os.environ.get("XDG_DOWNLOAD_DIR")
    if env: return Path(env).expanduser()
    config=Path(os.environ.get("XDG_CONFIG_HOME") or home/".config")/"user-dirs.dirs"
    try:
        for line in config.read_text().splitlines():
            m=re.match(r'\s*XDG_DOWNLOAD_DIR\s*=\s*"(.*)"\s*$',line)
            if m: return Path(m.group(1).replace("$HOME",str(home))).expanduser()
    except OSError: pass
    return home/"Downloads"

# Override with SAYA_CIVITAI_DOWNLOAD_ROOT or --download-root.
DEFAULT=Path(os.environ.get("SAYA_CIVITAI_DOWNLOAD_ROOT") or default_download_root()).expanduser()
API_BASE=os.environ.get("SAYA_CIVITAI_API","https://civitai.red/api/v1")
JOBS={}
JOBS_LOCK=threading.RLock()
CONTROLS={}
THREADS={}
FILE_LOCKS={}
JOBS_FILE=None
ALLOWED_ROOTS=[]
SAVE_INTERVAL=0.5

class Cancelled(Exception):
    pass

class JobControl:
    def __init__(self):
        self.cancel=threading.Event()
        self.paused=threading.Event()

class SameHostRedirect(HTTPRedirectHandler):
    """Never forward credentials to another host after a redirect."""
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        new=super().redirect_request(req, fp, code, msg, headers, newurl)
        if new is not None:
            a=urlsplit(req.full_url); b=urlsplit(newurl)
            if (a.scheme,a.hostname,a.port)!=(b.scheme,b.hostname,b.port):
                for name in list(new.headers)+list(new.unredirected_hdrs):
                    if name.lower() in ("authorization","x-saya-token","cookie"):
                        new.headers.pop(name,None); new.unredirected_hdrs.pop(name,None)
        return new

OPENER=build_opener(SameHostRedirect)
def urlopen(req, timeout=None): return OPENER.open(req, timeout=timeout)

def token():
    try: return TOKEN.read_text().strip()
    except OSError: return None

def bridge_token():
    global TOKEN_FILE
    path=Path(TOKEN_FILE or (Path.home()/".config/saya-studio/civitai-token"))
    path.parent.mkdir(parents=True,exist_ok=True)
    if not path.exists():
        try:
            fd=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
            with os.fdopen(fd,"w") as f: f.write(secrets.token_hex(32))
        except FileExistsError: pass
    return path.read_text().strip()

def allowed_root(dest):
    raw=Path(dest).expanduser() if dest else (Path(ALLOWED_ROOTS[0]) if ALLOWED_ROOTS else DEFAULT)
    root=raw.resolve()
    roots=[Path(p).expanduser().resolve() for p in ALLOWED_ROOTS] or [DEFAULT.resolve()]
    if not any(root == p or root.is_relative_to(p) for p in roots):
        raise ValueError("Destination hors des racines Saya autorisées")
    root.mkdir(parents=True,exist_ok=True)
    return root

def api(path):
    req=Request(API_BASE.rstrip("/")+path,headers={"User-Agent":"SayaStudio/1.0"})
    t=token()
    if t: req.add_header("Authorization","Bearer "+t)
    with urlopen(req,timeout=30) as r: return json.load(r)

def save_jobs():
    if not JOBS_FILE: return
    with JOBS_LOCK:
        try: data=json.dumps(JOBS,ensure_ascii=False)
        except Exception: return
    try:
        tmp=Path(str(JOBS_FILE)+".tmp")
        with open(tmp,"w",encoding="utf-8") as out:
            out.write(data); out.flush(); os.fsync(out.fileno())
        os.replace(tmp,JOBS_FILE)
    except Exception: pass

def update(job, save=True, **fields):
    with JOBS_LOCK:
        j=JOBS.get(job)
        if j is None: return None
        j.update(fields)
    if save: save_jobs()
    return j

def snapshot(job):
    with JOBS_LOCK:
        j=JOBS.get(job)
        return dict(j) if j else None

def all_jobs():
    with JOBS_LOCK:
        return {k:dict(v) for k,v in JOBS.items()}

def gate(job, ctl):
    if ctl.cancel.is_set(): raise Cancelled()
    paused=False
    while ctl.paused.is_set():
        if not paused:
            update(job,state="paused",speed=0.0,eta=None); paused=True
        if ctl.cancel.is_set(): raise Cancelled()
        time.sleep(0.2)
    if paused: update(job,state="downloading")
    return paused

def open_stream(url, existing):
    req=Request(url,headers={"User-Agent":"SayaStudio/1.0"})
    t=token()
    if t: req.add_header("Authorization","Bearer "+t)
    if existing>0: req.add_header("Range","bytes=%d-"%existing)
    try:
        return urlopen(req,timeout=60)
    except HTTPError as e:
        if e.code==416: return None
        raise

def content_total(response):
    value=response.headers.get("Content-Range") or ""
    m=re.search(r"/(\d+)\s*$",value)
    if m: return int(m.group(1))
    length=response.headers.get("Content-Length")
    return int(length) if length and length.isdigit() and response.getcode()!=206 else None

def hash_existing(path, digest):
    if not path.exists(): return
    with open(path,"rb") as old:
        while True:
            chunk=old.read(1024*1024)
            if not chunk: break
            digest.update(chunk)

def download(job, version_id, file_id, dest):
    with JOBS_LOCK: ctl=CONTROLS.get(job)
    if ctl is None:
        ctl=JobControl()
        with JOBS_LOCK: CONTROLS[job]=ctl
    part=None
    final=None
    response=None
    file_key=None
    try:
        v=api(f"/model-versions/{version_id}")
        files=v.get("files") or []
        f=next((x for x in files if file_id and x.get("id")==file_id),None)
        if not f: f=next((x for x in files if x.get("primary")),None) or (files[0] if files else None)
        if not f or not f.get("downloadUrl"): raise ValueError("Aucun fichier téléchargeable")
        url=f["downloadUrl"]
        name=re.sub(r"[^A-Za-z0-9._-]","_",f.get("name") or f"civitai-{version_id}.safetensors")
        root=allowed_root(dest)
        final=root/name
        if final.exists(): raise ValueError("Fichier déjà présent : aucun écrasement")
        part=root/("."+name+".part")
        file_key=str(part)
        with JOBS_LOCK:
            if file_key in FILE_LOCKS: raise ValueError("Un téléchargement de ce fichier est déjà en cours")
            FILE_LOCKS[file_key]=job
        expected=(f.get("hashes") or {}).get("SHA256")
        total=int(f.get("sizeKB") or 0)*1024 or None
        existing=part.stat().st_size if part.exists() else 0
        update(job,state="downloading",name=name,total=total,bytes=existing,error=None,speed=0.0,eta=None,sha256=None)
        gate(job,ctl)
        response=open_stream(url,existing)
        if response is None:
            existing=0; response=open_stream(url,0)
            if response is None: raise ValueError("Reprise HTTP 416 impossible")
        status=response.getcode()
        digest=hashlib.sha256()
        exact_total=False
        if status==206 and existing>0:
            hash_existing(part,digest); mode="ab"; offset=existing
            head_total=content_total(response)
            if head_total: total=head_total; exact_total=True
            update(job,total=total)
        else:
            if existing>0: response.close(); response=open_stream(url,0)
            offset=0; mode="wb"
            head_total=content_total(response)
            if head_total: total=head_total; exact_total=True
        n=offset
        samples=deque([(time.time(),n)],maxlen=8)
        last_save=0.0
        with open(part,mode) as out:
            while True:
                if gate(job,ctl):
                    samples=deque([(time.time(),n)],maxlen=8)
                chunk=response.read(65536)
                if not chunk: break
                out.write(chunk); digest.update(chunk); n+=len(chunk)
                now=time.time(); samples.append((now,n))
                t0,n0=samples[0]
                rate=(n-n0)/(now-t0) if now>t0 else 0.0
                eta=int(max(0,(total-n)/rate)) if total and rate>0 else None
                update(job,save=False,bytes=n,speed=round(rate,1),eta=eta)
                if now-last_save>=SAVE_INTERVAL: save_jobs(); last_save=now
        response.close()
        gate(job,ctl)
        if exact_total and total and n<total: raise ValueError("Téléchargement incomplet : %d/%d octets"%(n,total))
        value=digest.hexdigest()
        if expected and value.lower()!=str(expected).lower():
            try: part.unlink()
            except OSError: pass
            part=None
            raise ValueError("SHA-256 invalide")
        os.replace(part,final); part=None
        if ctl.cancel.is_set():
            try: final.unlink()
            except OSError: pass
            raise Cancelled()
        update(job,state="done",sha256=value,path=str(final),bytes=n,speed=0.0,eta=0,finishedAt=time.time())
    except Cancelled:
        if part is not None:
            try: part.unlink()
            except OSError: pass
        update(job,state="cancelled",error=None,speed=0.0,eta=None,finishedAt=time.time())
    except Exception as e:
        update(job,state="error",error=str(e),speed=0.0,eta=None,finishedAt=time.time())
    finally:
        if file_key is not None:
            with JOBS_LOCK:
                if FILE_LOCKS.get(file_key)==job: FILE_LOCKS.pop(file_key,None)
        if response is not None:
            try: response.close()
            except Exception: pass
        with JOBS_LOCK:
            THREADS.pop(job,None)
            if CONTROLS.get(job) is ctl: CONTROLS.pop(job,None)
        save_jobs()

def spawn(job, version_id, file_id, dest):
    ctl=JobControl()
    with JOBS_LOCK: CONTROLS[job]=ctl
    thread=threading.Thread(target=download,args=(job,version_id,file_id,dest),daemon=True,name="civitai-"+job)
    with JOBS_LOCK: THREADS[job]=thread
    thread.start()

class H(BaseHTTPRequestHandler):
    def cors(self):
        origin=self.headers.get("Origin","")
        if origin.startswith("chrome-extension://"): self.send_header("Access-Control-Allow-Origin",origin)
        self.send_header("Access-Control-Allow-Headers","Content-Type, X-Saya-Token, X-Saya-Extension"); self.send_header("Access-Control-Allow-Methods","POST,GET,OPTIONS")
    def out(self,code,obj):
        body=json.dumps(obj).encode(); self.send_response(code); self.cors(); self.send_header("Content-Type","application/json"); self.send_header("Content-Length",str(len(body))); self.end_headers(); self.wfile.write(body)
    def do_OPTIONS(self): self.send_response(204); self.cors(); self.end_headers()
    def authorized(self):
        return self.headers.get("X-Saya-Token","")==bridge_token()
    def control(self, verb, job):
        with JOBS_LOCK:
            j=JOBS.get(job); state=j.get("state") if j else None
            ctl=CONTROLS.get(job); thread=THREADS.get(job)
        if j is None: return self.out(404,{"error":"tâche inconnue"})
        if verb=="cancel":
            if state=="done": return self.out(409,{"error":"tâche déjà terminée","state":state})
            if state=="cancelled": return self.out(200,{"job":job,"state":state})
            if ctl is None:
                ctl=JobControl()
                with JOBS_LOCK: CONTROLS[job]=ctl
            ctl.cancel.set()
            update(job,state="cancelled",error=None,speed=0.0,eta=None,finishedAt=time.time())
            return self.out(200,{"job":job,"state":"cancelled"})
        if verb=="pause":
            if state=="paused": return self.out(200,{"job":job,"state":state})
            if state not in {"queued","downloading"}: return self.out(409,{"error":"tâche non pausable","state":state})
            if ctl is None:
                ctl=JobControl()
                with JOBS_LOCK: CONTROLS[job]=ctl
            ctl.paused.set()
            update(job,state="paused",speed=0.0,eta=None)
            return self.out(200,{"job":job,"state":"paused"})
        if state=="done": return self.out(409,{"error":"tâche déjà terminée","state":state})
        if state in {"queued","downloading"}: return self.out(200,{"job":job,"state":state})
        if state=="paused" and thread is not None and thread.is_alive():
            if ctl is not None: ctl.paused.clear()
            update(job,state="downloading")
            return self.out(200,{"job":job,"state":"downloading"})
        if thread is not None and thread.is_alive(): thread.join(timeout=5.0)
        with JOBS_LOCK: thread=THREADS.get(job)
        if thread is not None and thread.is_alive():
            return self.out(409,{"error":"arrêt en cours, réessayer","state":state})
        update(job,state="queued",error=None,speed=0.0,eta=None,sha256=None)
        spawn(job,j.get("versionId"),j.get("fileId"),j.get("destination"))
        return self.out(202,{"job":job,"state":"queued"})

    def do_GET(self):
        if not self.authorized(): return self.out(403,{"error":"jeton Saya requis"})
        if self.path=="/health": return self.out(200,{"ok":True})
        if self.path=="/jobs": return self.out(200,{"jobs":all_jobs()})
        m=re.fullmatch(r"/(cancel|pause|resume)/([0-9a-f-]+)",self.path)
        if m: return self.control(m.group(1),m.group(2))
        m=re.fullmatch(r"/status/([0-9a-f-]+)",self.path)
        if m: return self.out(200,snapshot(m.group(1)) or {"state":"unknown"})
        self.out(404,{"error":"not found"})
    def do_POST(self):
        if not self.authorized(): return self.out(403,{"error":"jeton Saya requis"})
        m=re.fullmatch(r"/(cancel|pause|resume)/([0-9a-f-]+)",self.path)
        if m: return self.control(m.group(1),m.group(2))
        if self.path!="/download": return self.out(404,{"error":"not found"})
        try:
            n=int(self.headers.get("Content-Length","0")); p=json.loads(self.rfile.read(n) or b"{}")
            vid=p.get("versionId") or p.get("modelVersionId")
            if not vid: return self.out(400,{"error":"versionId manquant"})
            root=allowed_root(p.get("destination"))
            job=uuid.uuid4().hex
            with JOBS_LOCK:
                JOBS[job]={"state":"queued","bytes":0,"total":None,"speed":0.0,"eta":None,"createdAt":time.time(),"versionId":vid,"fileId":p.get("fileId"),"destination":str(root)}
            save_jobs()
            spawn(job,vid,p.get("fileId"),str(root))
            self.out(202,{"job":job})
        except Exception as e: self.out(400,{"error":str(e)})

def watch_parent(parent):
    while True:
        time.sleep(2)
        try:
            os.kill(parent, 0)
        except OSError:
            os._exit(0)

if __name__=="__main__":
    ap=argparse.ArgumentParser();ap.add_argument("--port",type=int,default=PORT);ap.add_argument("--token-file",default=None);ap.add_argument("--allowed-root",action="append",default=[]);ap.add_argument("--parent-pid",type=int,default=0);ap.add_argument("--download-root",default=None)
    a=ap.parse_args()
    if a.download_root: DEFAULT=Path(a.download_root).expanduser()
    PORT=a.port;TOKEN_FILE=a.token_file;ALLOWED_ROOTS[:]=[str(DEFAULT), *a.allowed_root]
    bridge_token()
    if a.parent_pid > 0: threading.Thread(target=watch_parent,args=(a.parent_pid,),daemon=True).start()
    JOBS_FILE=Path(TOKEN_FILE).parent/"civitai-jobs.json" if TOKEN_FILE else (Path.home()/".config/saya-studio/civitai-jobs.json")
    if JOBS_FILE.exists():
        try:
            with JOBS_LOCK: JOBS.update(json.loads(JOBS_FILE.read_text()))
            for jid,j in list(JOBS.items()):
                j.setdefault("speed",0.0); j.setdefault("eta",None); j.setdefault("total",None)
                j.setdefault("destination",str(DEFAULT))
                if j.get("state") in {"queued","downloading"} and j.get("versionId"):
                    j["state"]="queued"
                    spawn(jid,j["versionId"],j.get("fileId"),j.get("destination"))
                elif j.get("state")=="paused":
                    ctl=JobControl(); ctl.paused.set()
                    with JOBS_LOCK: CONTROLS[jid]=ctl
        except Exception: pass
    print("Saya Civitai bridge http://127.0.0.1:%d"%PORT,flush=True)
    ThreadingHTTPServer(("127.0.0.1",PORT),H).serve_forever()
