use crate::{
    clipboard::{hash, Filter},
    model::*,
    platform::{ClipboardPort, NativeClipboard},
    security::{self, Identity},
    storage,
    transfer::{self, Receiver},
    transport,
};
use anyhow::{bail, ensure, Result};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Semaphore,
};
pub struct Service {
    pub identity: Identity,
    dir: PathBuf,
    pub config: Mutex<Config>,
    pub status: Mutex<Status>,
    filter: Mutex<Filter>,
    pairing: Mutex<Option<(String, u64)>>,
    receiver: Mutex<Receiver>,
    inbox: Mutex<Option<String>>,
    send_paths: Mutex<BTreeMap<String, PathBuf>>,
    outbox: Mutex<BTreeMap<String, Clip>>,
    send_permits: Arc<Semaphore>,
    running: Mutex<HashSet<String>>,
}
impl Service {
    pub fn open(dir: &Path) -> Result<Arc<Self>> {
        std::fs::create_dir_all(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
        let identity = Identity::load(dir)?;
        let p = dir.join("config.json");
        let config: Config = if p.exists() {
            storage::load(&p)?
        } else {
            let c = Config {
                receive_dir: dir.join("received").to_string_lossy().into(),
                ..Config::default()
            };
            storage::save(&p, &c)?;
            c
        };
        let receiver = Receiver::new(Path::new(&config.receive_dir), dir, config.max_file)?;
        let mut status = Status::default();
        if dir.join("tasks.json").exists() {
            status.tasks = storage::load(&dir.join("tasks.json"))?;
        }
        for task in status.tasks.values_mut() {
            if ["sending", "receiving", "connecting", "queued"].contains(&task.status.as_str()) {
                task.status = "interrupted".into();
            }
        }
        let paths = if dir.join("send-paths.json").exists() {
            storage::load(&dir.join("send-paths.json"))?
        } else {
            BTreeMap::new()
        };
        let send_permits = Arc::new(Semaphore::new(config.transfer_concurrency.clamp(1, 8)));
        Ok(Arc::new(Self {
            identity,
            dir: dir.into(),
            config: Mutex::new(config),
            status: Mutex::new(status),
            filter: Mutex::new(Filter::default()),
            pairing: Mutex::new(None),
            receiver: Mutex::new(receiver),
            inbox: Mutex::new(None),
            send_paths: Mutex::new(paths),
            outbox: Mutex::new(BTreeMap::new()),
            send_permits,
            running: Mutex::new(HashSet::new()),
        }))
    }
    pub fn snapshot(&self) -> Status {
        let cfg = self.config.lock().unwrap().clone();
        let mut status = self.status.lock().unwrap();
        status.history.retain(|c| {
            now().saturating_sub(c.timestamp) < cfg.history_hours.saturating_mul(3_600_000)
        });
        status.history.truncate(cfg.history_limit);
        status.clone()
    }
    pub fn configure(&self, c: Config) -> Result<()> {
        let addr: std::net::SocketAddr = c.listen.parse()?;
        ensure!(
            !addr.ip().is_unspecified(),
            "bind an explicit virtual IP, never a wildcard"
        );
        ensure!(
            c.max_text <= 1024 * 1024 && c.history_limit <= 1000,
            "configuration exceeds limit"
        );
        ensure!(
            (1..=CHUNK).contains(&c.chunk_size)
                && (1..=120).contains(&c.timeout_seconds)
                && (1..=8).contains(&c.transfer_concurrency),
            "invalid transport limits"
        );
        let receiver = Receiver::new(Path::new(&c.receive_dir), &self.dir, c.max_file)?;
        storage::save(&self.dir.join("config.json"), &c)?;
        *self.receiver.lock().unwrap() = receiver;
        *self.config.lock().unwrap() = c;
        Ok(())
    }
    fn persist_tasks(&self) -> Result<()> {
        storage::save(
            &self.dir.join("tasks.json"),
            &self.status.lock().unwrap().tasks,
        )
    }
    pub fn ticket(&self) -> Ticket {
        let c = self.config.lock().unwrap().clone();
        let token = security::token();
        *self.pairing.lock().unwrap() = Some((token.clone(), now() + 120_000));
        Ticket {
            peer: self.identity.peer(c.name.clone(), c.listen.clone()),
            token,
        }
    }
    pub async fn pair(&self, ticket: Ticket) -> Result<()> {
        security::validate_peer(&ticket.peer)?;
        let c = self.config.lock().unwrap().clone();
        let me = self.identity.peer(c.name, c.listen);
        self.rpc(
            &ticket.peer,
            Request::Pair {
                peer: me,
                token: ticket.token,
            },
            &[],
        )
        .await?;
        let mut cfg = self.config.lock().unwrap().clone();
        cfg.peers.insert(ticket.peer.id.clone(), ticket.peer);
        self.configure(cfg)
    }
    pub fn revoke(&self, id: &str) -> Result<()> {
        let mut c = self.config.lock().unwrap().clone();
        c.peers.remove(id);
        self.configure(c)?;
        self.status.lock().unwrap().peers.remove(id);
        Ok(())
    }
    pub async fn rpc(&self, peer: &Peer, request: Request, body: &[u8]) -> Result<Reply> {
        let timeout = self.config.lock().unwrap().timeout_seconds;
        tokio::time::timeout(Duration::from_secs(timeout), async {
            let mut stream = transport::connect(&peer.address, &peer.cert).await?;
            let mut nonce = [0; 32];
            stream.read_exact(&mut nonce).await?;
            let mut signed = nonce.to_vec();
            signed.extend(serde_json::to_vec(&request)?);
            signed.extend(hash(body).as_bytes());
            let auth = Auth {
                version: VERSION,
                id: self.identity.id(),
                signature: self.identity.sign(&signed),
                public_key: self.identity.public(),
                request,
            };
            transport::write(&mut stream, &auth, body).await?;
            let (reply, _): (Reply, Vec<u8>) = transport::read(&mut stream).await?;
            ensure!(reply.ok, "{}", reply.message);
            Ok(reply)
        })
        .await?
    }
    pub async fn listen(self: Arc<Self>) -> Result<()> {
        let address = self.config.lock().unwrap().listen.clone();
        let bind: std::net::SocketAddr = address.parse()?;
        ensure!(
            !bind.ip().is_unspecified(),
            "wildcard listener is forbidden"
        );
        let listener = tokio::net::TcpListener::bind(&address).await?;
        let acceptor = transport::server(&self.identity)?;
        self.status.lock().unwrap().listening = true;
        let permits = Arc::new(Semaphore::new(16));
        let mut rates: BTreeMap<std::net::IpAddr, (std::time::Instant, u32)> = BTreeMap::new();
        loop {
            let (socket, remote) = listener.accept().await?;
            if rates.len() > 1024 {
                rates.retain(|_, (t, _)| t.elapsed() < Duration::from_secs(1));
                if rates.len() > 1024 {
                    continue;
                }
            }
            let rate = rates
                .entry(remote.ip())
                .or_insert((std::time::Instant::now(), 0));
            if rate.0.elapsed() >= Duration::from_secs(1) {
                *rate = (std::time::Instant::now(), 0);
            }
            rate.1 += 1;
            if rate.1 > 100 {
                continue;
            }
            let permit = match permits.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let s = self.clone();
            let a = acceptor.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let result = tokio::time::timeout(Duration::from_secs(15), async {
                    let mut stream = a.accept(socket).await?;
                    let nonce = hex::decode(security::token())?;
                    stream.write_all(&nonce).await?;
                    let (auth, body): (Auth, Vec<u8>) = transport::read(&mut stream).await?;
                    let reply = match s.handle(auth, body, nonce) {
                        Ok(r) => r,
                        Err(e) => Reply {
                            ok: false,
                            message: e.to_string(),
                            offset: 0,
                        },
                    };
                    transport::write(&mut stream, &reply, &[]).await?;
                    anyhow::Ok(())
                })
                .await;
                if !matches!(result, Ok(Ok(()))) {
                    tracing::debug!("connection closed, timed out or TLS failed");
                }
            });
        }
    }
    fn handle(&self, a: Auth, body: Vec<u8>, nonce: Vec<u8>) -> Result<Reply> {
        ensure!(a.version == VERSION, "unsupported protocol version");
        let mut signed = nonce;
        signed.extend(serde_json::to_vec(&a.request)?);
        signed.extend(hash(&body).as_bytes());
        security::verify(&a.public_key, &signed, &a.signature)?;
        ensure!(a.id == hex::encode(&a.public_key), "identity mismatch");
        if let Request::Pair { peer, token } = &a.request {
            security::validate_peer(peer)?;
            ensure!(peer.id == a.id, "pair identity mismatch");
            let mut pairing = self.pairing.lock().unwrap();
            ensure!(
                pairing
                    .as_ref()
                    .is_some_and(|(t, exp)| t == token && *exp >= now()),
                "invalid or expired pairing invitation"
            );
            let mut c = self.config.lock().unwrap().clone();
            c.peers.insert(peer.id.clone(), peer.clone());
            self.configure(c)?;
            *pairing = None;
            return Ok(Reply::ok(0));
        }
        let cfg = self.config.lock().unwrap().clone();
        let peer = cfg
            .peers
            .get(&a.id)
            .ok_or_else(|| anyhow::anyhow!("untrusted device"))?;
        ensure!(peer.public_key == a.public_key, "key changed");
        self.status
            .lock()
            .unwrap()
            .peers
            .insert(a.id.clone(), "online".into());
        match a.request {
            Request::Ping => Ok(Reply::ok(0)),
            Request::Clipboard(c) => {
                ensure!(
                    peer.clipboard && c.source == a.id,
                    "clipboard permission denied"
                );
                let mut filter = self.filter.lock().unwrap();
                if filter.admit(&c, &cfg) {
                    *self.inbox.lock().unwrap() = Some(c.text.clone());
                    self.history(c);
                }
                Ok(Reply::ok(0))
            }
            Request::Offer(o) => {
                Receiver::validate(&o, cfg.max_file)?;
                let mut st = self.status.lock().unwrap();
                if let Some(t) = st.tasks.get(&o.id) {
                    ensure!(
                        t.peer == a.id
                            && t.offer.hash == o.hash
                            && t.offer.size == o.size
                            && t.offer.name == o.name,
                        "task identity mismatch"
                    );
                    ensure!(
                        !["pending", "rejected", "cancelled"].contains(&t.status.as_str()),
                        "receiver approval required or task rejected"
                    );
                    let offset = if t.status == "complete" {
                        t.offset
                    } else {
                        self.receiver.lock().unwrap().accept(&o, &a.id)?
                    };
                    return Ok(Reply::ok(offset));
                }
                ensure!(st.tasks.len() < 1000, "task limit reached");
                st.tasks.insert(
                    o.id.clone(),
                    Task {
                        offer: o,
                        peer: a.id,
                        direction: "receive".into(),
                        status: "pending".into(),
                        offset: 0,
                        error: None,
                        bytes_per_second: 0,
                        eta_seconds: None,
                    },
                );
                drop(st);
                self.persist_tasks()?;
                bail!("receiver approval required; retry after acceptance")
            }
            Request::Chunk { id, offset } => {
                self.require_active(&id, &a.id)?;
                let n = self
                    .receiver
                    .lock()
                    .unwrap()
                    .chunk(&id, &a.id, offset, &body)?;
                self.update_task(&id, n, "receiving", None)?;
                Ok(Reply::ok(n))
            }
            Request::Finish { id } => {
                {
                    let st = self.status.lock().unwrap();
                    if let Some(t) = st.tasks.get(&id) {
                        if t.peer == a.id && t.status == "complete" {
                            return Ok(Reply::ok(t.offset));
                        }
                    }
                }
                self.require_active(&id, &a.id)?;
                let n = self.receiver.lock().unwrap().finish(&id, &a.id)?;
                self.update_task(&id, n, "complete", None)?;
                Ok(Reply::ok(n))
            }
            Request::Cancel { id } => {
                self.require_active(&id, &a.id)?;
                self.receiver.lock().unwrap().cancel(&id, &a.id)?;
                self.update_task(&id, 0, "cancelled", None)?;
                Ok(Reply::ok(0))
            }
            Request::Pair { .. } => unreachable!(),
        }
    }
    fn require_active(&self, id: &str, peer: &str) -> Result<()> {
        let st = self.status.lock().unwrap();
        let t = st
            .tasks
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("unknown task"))?;
        ensure!(
            t.peer == peer && ["accepted", "receiving", "interrupted"].contains(&t.status.as_str()),
            "task not approved/active"
        );
        Ok(())
    }
    pub fn decide(&self, id: &str, accept: bool) -> Result<()> {
        let mut st = self.status.lock().unwrap();
        let t = st
            .tasks
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("unknown task"))?;
        ensure!(
            t.direction == "receive" && t.status == "pending",
            "not a pending receive"
        );
        if accept {
            t.offset = self.receiver.lock().unwrap().accept(&t.offer, &t.peer)?;
            t.status = "accepted".into();
        } else {
            t.status = "rejected".into();
        }
        drop(st);
        self.persist_tasks()
    }
    fn update_task(
        &self,
        id: &str,
        offset: u64,
        status: &str,
        error: Option<String>,
    ) -> Result<()> {
        if let Some(t) = self.status.lock().unwrap().tasks.get_mut(id) {
            if t.status == "cancelled" && status != "connecting" {
                return Ok(());
            }
            t.offset = offset;
            t.status = status.into();
            t.error = error;
        }
        self.persist_tasks()
    }
    fn history(&self, c: Clip) {
        let cfg = self.config.lock().unwrap();
        let mut st = self.status.lock().unwrap();
        if cfg.history_limit > 0 {
            st.history.insert(0, c);
            st.history.truncate(cfg.history_limit);
        }
    }
    pub async fn publish(&self, text: String) -> Result<()> {
        let mut c = Clip {
            id: uuid::Uuid::new_v4().to_string(),
            source: self.identity.id(),
            timestamp: now(),
            hash: hash(text.as_bytes()),
            text,
        };
        let cfg = self.config.lock().unwrap().clone();
        let admitted = {
            let mut filter = self.filter.lock().unwrap();
            c.timestamp = filter.next_timestamp();
            filter.admit(&c, &cfg)
        };
        if !admitted {
            return Ok(());
        }
        self.history(c.clone());
        for peer in cfg.peers.values().filter(|p| p.clipboard) {
            self.outbox
                .lock()
                .unwrap()
                .insert(peer.id.clone(), c.clone());
            let result = self.rpc(peer, Request::Clipboard(c.clone()), &[]).await;
            if result.is_ok() {
                let mut out = self.outbox.lock().unwrap();
                if out.get(&peer.id).is_some_and(|v| v.id == c.id) {
                    out.remove(&peer.id);
                }
            }
            let state = match result {
                Ok(_) => "online".into(),
                Err(e) => format!("sync failed: {e}"),
            };
            self.status
                .lock()
                .unwrap()
                .peers
                .insert(peer.id.clone(), state);
        }
        Ok(())
    }
    pub fn start_clipboard(self: &Arc<Self>) {
        let s = self.clone();
        let runtime = tokio::runtime::Handle::current();
        std::thread::spawn(move || {
            let mut port = match NativeClipboard::open() {
                Ok(p) => p,
                Err(e) => {
                    s.status.lock().unwrap().clipboard_error = Some(e.to_string());
                    return;
                }
            };
            let mut last = String::new();
            loop {
                std::thread::sleep(Duration::from_millis(500));
                if let Some(text) = s.inbox.lock().unwrap().take() {
                    match port.write_text(text.clone()) {
                        Ok(()) => last = text,
                        Err(e) => s.status.lock().unwrap().clipboard_error = Some(e.to_string()),
                    }
                }
                let cfg = s.config.lock().unwrap().clone();
                if !cfg.enabled {
                    continue;
                }
                if !cfg.ignored_apps.is_empty() {
                    let app = port.foreground_app();
                    if app.is_none()
                        || app.is_some_and(|a| {
                            cfg.ignored_apps
                                .iter()
                                .any(|ignored| ignored.eq_ignore_ascii_case(&a))
                        })
                    {
                        s.status.lock().unwrap().clipboard_error=Some("Application exclusion unavailable: automatic capture paused (fail closed)".into());
                        continue;
                    }
                }
                if let Ok(text) = port.read_text() {
                    if text != last {
                        last = text.clone();
                        let sender = s.clone();
                        runtime.spawn(async move {
                            if let Err(e) = sender.publish(text).await {
                                tracing::warn!(error=%e,"clipboard delivery failed")
                            }
                        });
                    }
                }
            }
        });
    }
    pub fn start_monitor(self: &Arc<Self>) {
        let s = self.clone();
        tokio::spawn(async move {
            loop {
                let cfg = s.config.lock().unwrap().clone();
                let peers = cfg.peers;
                for p in peers.values() {
                    let pending = s.outbox.lock().unwrap().get(&p.id).cloned();
                    if cfg.enabled && p.clipboard {
                        if let Some(c) = pending {
                            if s.rpc(p, Request::Clipboard(c.clone()), &[]).await.is_ok() {
                                let mut out = s.outbox.lock().unwrap();
                                if out.get(&p.id).is_some_and(|v| v.id == c.id) {
                                    out.remove(&p.id);
                                }
                            }
                        }
                    } else {
                        s.outbox.lock().unwrap().remove(&p.id);
                    }
                    let state = match s.rpc(p, Request::Ping, &[]).await {
                        Ok(_) => "online".into(),
                        Err(e) => format!("offline: {e}"),
                    };
                    s.status.lock().unwrap().peers.insert(p.id.clone(), state);
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }
    pub fn queue_folder(self: &Arc<Self>, peer: &str, path: &Path) -> Result<String> {
        ensure!(
            self.config.lock().unwrap().peers.contains_key(peer),
            "untrusted target"
        );
        let out = self
            .dir
            .join("outgoing")
            .join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir_all(&out)?;
        let archive = transfer::pack_folder(path, &out, self.config.lock().unwrap().max_file)?;
        self.queue_file(peer, &archive)
    }
    pub fn queue_file(self: &Arc<Self>, peer: &str, path: &Path) -> Result<String> {
        let cfg = self.config.lock().unwrap().clone();
        ensure!(cfg.peers.contains_key(peer), "untrusted target");
        ensure!(path.is_file(), "select a regular file");
        ensure!(self.snapshot().tasks.len() < 1000, "task limit reached");
        let o = transfer::offer(path)?;
        Receiver::validate(&o, cfg.max_file)?;
        let id = o.id.clone();
        {
            let mut paths = self.send_paths.lock().unwrap();
            paths.insert(id.clone(), path.into());
            storage::save(&self.dir.join("send-paths.json"), &*paths)?;
        }
        self.status.lock().unwrap().tasks.insert(
            id.clone(),
            Task {
                offer: o,
                peer: peer.into(),
                direction: "send".into(),
                status: "queued".into(),
                offset: 0,
                error: None,
                bytes_per_second: 0,
                eta_seconds: None,
            },
        );
        self.persist_tasks()?;
        self.retry(&id)?;
        Ok(id)
    }
    pub fn retry(self: &Arc<Self>, id: &str) -> Result<()> {
        let task = self
            .status
            .lock()
            .unwrap()
            .tasks
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown task"))?;
        ensure!(
            task.direction == "send"
                && !["sending", "connecting", "complete"].contains(&task.status.as_str()),
            "task already active/complete"
        );
        let path = self
            .send_paths
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing source path"))?;
        {
            let mut running = self.running.lock().unwrap();
            ensure!(
                running.insert(id.into()),
                "task worker still stopping; retry shortly"
            );
        }
        if let Err(e) = self.update_task(id, task.offset, "connecting", None) {
            self.running.lock().unwrap().remove(id);
            return Err(e);
        }
        let s = self.clone();
        tokio::spawn(async move {
            let _permit = s.send_permits.clone().acquire_owned().await;
            if s.snapshot()
                .tasks
                .get(&task.offer.id)
                .is_some_and(|t| t.status == "cancelled")
            {
                s.running.lock().unwrap().remove(&task.offer.id);
                return;
            }
            if let Err(e) = s.send_file(&task, &path).await {
                if s.snapshot()
                    .tasks
                    .get(&task.offer.id)
                    .is_some_and(|t| t.status == "cancelled")
                {
                    s.running.lock().unwrap().remove(&task.offer.id);
                    return;
                }
                let _ = s.update_task(
                    &task.offer.id,
                    s.snapshot()
                        .tasks
                        .get(&task.offer.id)
                        .map_or(0, |t| t.offset),
                    "interrupted",
                    Some(e.to_string()),
                );
            }
            s.running.lock().unwrap().remove(&task.offer.id);
        });
        Ok(())
    }
    async fn send_file(&self, t: &Task, path: &Path) -> Result<()> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        ensure!(
            transfer::digest(path)? == t.offer.hash,
            "source file changed"
        );
        let p = self
            .config
            .lock()
            .unwrap()
            .peers
            .get(&t.peer)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("trust revoked"))?;
        let mut offset = self
            .rpc(&p, Request::Offer(t.offer.clone()), &[])
            .await?
            .offset;
        ensure!(offset <= t.offer.size, "invalid resume offset");
        let start = std::time::Instant::now();
        let initial = offset;
        let mut file = tokio::fs::File::open(path).await?;
        file.seek(std::io::SeekFrom::Start(offset)).await?;
        let mut b = vec![0; self.config.lock().unwrap().chunk_size.clamp(1, CHUNK)];
        while offset < t.offer.size {
            ensure!(
                self.status
                    .lock()
                    .unwrap()
                    .tasks
                    .get(&t.offer.id)
                    .is_some_and(|t| t.status != "cancelled"),
                "cancelled"
            );
            ensure!(
                self.config.lock().unwrap().peers.contains_key(&t.peer),
                "trust revoked"
            );
            let n = file.read(&mut b).await?;
            ensure!(n > 0, "source truncated");
            let next = self
                .rpc(
                    &p,
                    Request::Chunk {
                        id: t.offer.id.clone(),
                        offset,
                    },
                    &b[..n],
                )
                .await?
                .offset;
            ensure!(next == offset + n as u64, "invalid chunk acknowledgement");
            offset = next;
            self.update_task(&t.offer.id, offset, "sending", None)?;
            if let Some(task) = self.status.lock().unwrap().tasks.get_mut(&t.offer.id) {
                task.bytes_per_second =
                    ((offset - initial) as f64 / start.elapsed().as_secs_f64().max(0.001)) as u64;
                task.eta_seconds = Some((t.offer.size - offset) / task.bytes_per_second.max(1));
            }
        }
        ensure!(
            self.snapshot()
                .tasks
                .get(&t.offer.id)
                .is_some_and(|t| t.status != "cancelled"),
            "cancelled"
        );
        self.rpc(
            &p,
            Request::Finish {
                id: t.offer.id.clone(),
            },
            &[],
        )
        .await?;
        self.update_task(&t.offer.id, offset, "complete", None)
    }
    pub fn cancel(&self, id: &str) -> Result<()> {
        let task = self
            .snapshot()
            .tasks
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown task"))?;
        if task.direction == "receive" && task.status != "pending" {
            self.receiver.lock().unwrap().cancel(id, &task.peer)?;
        }
        self.update_task(id, task.offset, "cancelled", None)
    }
    pub fn copy_local(&self, text: String) -> Result<()> {
        ensure!(
            text.len() <= self.config.lock().unwrap().max_text,
            "text exceeds limit"
        );
        *self.inbox.lock().unwrap() = Some(text);
        Ok(())
    }
    // Web callers are authenticated by the separate, explicitly enabled HTTPS bridge.
    pub(crate) fn web_offer(&self, peer: &str, offer: Offer) -> Result<Task> {
        let max = self
            .config
            .lock()
            .unwrap()
            .max_file
            .min(crate::web::MAX_WEB_FILE);
        Receiver::validate(&offer, max)?;
        let mut st = self.status.lock().unwrap();
        ensure!(st.tasks.len() < 1000, "task limit reached");
        let task = Task {
            offer,
            peer: peer.into(),
            direction: "receive".into(),
            status: "pending".into(),
            offset: 0,
            error: None,
            bytes_per_second: 0,
            eta_seconds: None,
        };
        ensure!(!st.tasks.contains_key(&task.offer.id), "duplicate task");
        st.tasks.insert(task.offer.id.clone(), task.clone());
        drop(st);
        self.persist_tasks()?;
        Ok(task)
    }
    pub(crate) fn web_task(&self, peer: &str, id: &str) -> Result<Task> {
        let st = self.status.lock().unwrap();
        let t = st
            .tasks
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("unknown task"))?;
        ensure!(t.peer == peer, "unauthorized task");
        Ok(t.clone())
    }
    pub(crate) fn web_chunk(&self, peer: &str, id: &str, offset: u64, bytes: &[u8]) -> Result<u64> {
        self.require_active(id, peer)?;
        let n = self
            .receiver
            .lock()
            .unwrap()
            .chunk(id, peer, offset, bytes)?;
        self.update_task(id, n, "receiving", None)?;
        Ok(n)
    }
    pub(crate) fn web_finish(&self, peer: &str, id: &str) -> Result<Task> {
        let t = self.web_task(peer, id)?;
        if t.status == "complete" {
            return Ok(t);
        }
        self.require_active(id, peer)?;
        let n = self.receiver.lock().unwrap().finish(id, peer)?;
        self.update_task(id, n, "complete", None)?;
        self.web_task(peer, id)
    }
    pub(crate) fn web_cancel(&self, peer: &str, id: &str) -> Result<()> {
        self.web_task(peer, id)?;
        self.cancel(id)
    }
    pub fn clear_history(&self) {
        self.status.lock().unwrap().history.clear();
    }
}
