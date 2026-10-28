//! Opt-in HTTPS bridge. No network-wide discovery, cloud relay or implicit clipboard capture.
use crate::{
    model::{now, Offer, CHUNK},
    platform::{ClipboardPort, NativeClipboard},
    security, transfer, Service,
};
use anyhow::{ensure, Result};
use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
    service::TowerToHyperService,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{io::AsyncReadExt, sync::Semaphore, task::JoinHandle};
pub const MAX_WEB_FILE: u64 = 64 * 1024 * 1024;
const SESSION_MS: u64 = 60 * 60 * 1000;
#[derive(Clone, Serialize)]
pub struct WebInvitation {
    pub url: String,
    pub qr_svg: String,
    pub certificate_sha256: String,
    pub expires: u64,
}
#[derive(Clone, Serialize)]
pub struct SharedFile {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub hash: String,
}
struct Data {
    invite: Option<(String, u64)>,
    sessions: BTreeMap<String, (String, u64)>,
    text: String,
    active: bool,
    files: BTreeMap<String, (SharedFile, PathBuf)>,
}
pub struct WebBridge {
    service: Arc<Service>,
    origin: String,
    fingerprint: String,
    data: Mutex<Data>,
    listener: Mutex<Option<JoinHandle<()>>>,
    connections: Mutex<Vec<JoinHandle<()>>>,
    operations: Arc<Semaphore>,
    mutation: Mutex<()>,
}
type ApiResult = std::result::Result<Json<serde_json::Value>, (StatusCode, String)>;
fn bad(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, e.to_string())
}
impl WebBridge {
    pub async fn start(service: Arc<Service>, address: &str) -> Result<Arc<Self>> {
        let address: SocketAddr = address.parse()?;
        ensure!(
            !address.ip().is_unspecified() && !address.ip().is_multicast(),
            "choose an explicit LAN or EasyTier IP"
        );
        let tcp = tokio::net::TcpListener::bind(address).await?;
        let address = tcp.local_addr()?;
        let host = address.ip().to_string();
        let cert = rcgen::generate_simple_self_signed(vec![host])?;
        let id = security::Identity {
            cert: cert.cert.der().to_vec(),
            tls_key: cert.key_pair.serialize_der(),
            signing: [0; 32],
        };
        let tls = crate::transport::server(&id)?;
        let bridge = Arc::new(Self {
            service,
            origin: format!("https://{address}"),
            fingerprint: crate::clipboard::hash(&id.cert),
            data: Mutex::new(Data {
                invite: None,
                sessions: BTreeMap::new(),
                text: String::new(),
                active: true,
                files: BTreeMap::new(),
            }),
            listener: Mutex::new(None),
            connections: Mutex::new(Vec::new()),
            operations: Arc::new(Semaphore::new(4)),
            mutation: Mutex::new(()),
        });
        let router = Self::router(bridge.clone());
        let weak = Arc::downgrade(&bridge);
        let handle = tokio::spawn(async move {
            let permits = Arc::new(Semaphore::new(16));
            while let Ok((socket, _)) = tcp.accept().await {
                let Ok(permit) = permits.clone().try_acquire_owned() else {
                    continue;
                };
                let tls = tls.clone();
                let app = router.clone();
                let task = tokio::spawn(async move {
                    let _permit = permit;
                    let _ = tokio::time::timeout(Duration::from_secs(120), async move {
                        if let Ok(stream) = tls.accept(socket).await {
                            let _ = Builder::new(TokioExecutor::new())
                                .serve_connection(
                                    TokioIo::new(stream),
                                    TowerToHyperService::new(app),
                                )
                                .await;
                        }
                    })
                    .await;
                });
                if let Some(b) = weak.upgrade() {
                    let mut tasks = b.connections.lock().unwrap();
                    tasks.retain(|h| !h.is_finished());
                    tasks.push(task);
                } else {
                    task.abort();
                    break;
                }
            }
        });
        *bridge.listener.lock().unwrap() = Some(handle);
        Ok(bridge)
    }
    pub fn router(bridge: Arc<Self>) -> Router {
        Router::new().route("/", get(|| async { Html(include_str!("../web/index.html")) }))
            .route("/api/pair", post(pair))
            .route("/api", post(api))
            .route("/api/chunk/{id}/{offset}", post(chunk))
            .route("/api/download/{id}", get(download))
            .layer(DefaultBodyLimit::max(1024 * 1024 + 4096))
            .layer(axum::middleware::from_fn(|req: axum::extract::Request, next: axum::middleware::Next| async move {
                let mut r = next.run(req).await;
                for (k,v) in [("cache-control", "no-store"), ("referrer-policy", "no-referrer"), ("x-content-type-options", "nosniff"), ("x-frame-options", "DENY"), ("content-security-policy", "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; img-src 'self' blob:; frame-ancestors 'none'; form-action 'none'; base-uri 'none'")] { r.headers_mut().insert(axum::http::HeaderName::from_static(k), v.parse().unwrap()); }
                r
            })).with_state(bridge)
    }
    pub fn invitation(&self) -> Result<WebInvitation> {
        let _guard = self.mutation.lock().unwrap();
        ensure!(self.data.lock().unwrap().active, "sharing stopped");
        let token = security::token();
        let expires = now() + 120_000;
        self.data.lock().unwrap().invite = Some((token.clone(), expires));
        let url = format!("{}/#{}", self.origin, token);
        let code = qrcode::QrCode::new(url.as_bytes())?;
        Ok(WebInvitation {
            url,
            qr_svg: code
                .render::<qrcode::render::svg::Color>()
                .min_dimensions(256, 256)
                .build(),
            certificate_sha256: self.fingerprint.clone(),
            expires,
        })
    }
    pub fn stop(&self) {
        let _guard = self.mutation.lock().unwrap();
        if let Some(h) = self.listener.lock().unwrap().take() {
            h.abort();
        }
        for h in self.connections.lock().unwrap().drain(..) {
            h.abort();
        }
        let mut d = self.data.lock().unwrap();
        d.active = false;
        d.invite = None;
        d.sessions.clear();
        d.text.clear();
        d.files.clear();
    }
    pub fn share_text(&self, text: String) -> Result<()> {
        self.validate_text(&text)?;
        let mut d = self.data.lock().unwrap();
        ensure!(d.active, "sharing stopped");
        d.text = text;
        Ok(())
    }
    pub fn share_clipboard(&self) -> Result<()> {
        self.share_text(NativeClipboard::open()?.read_text()?)
    }
    fn validate_text(&self, text: &str) -> Result<()> {
        let cfg = self.service.config.lock().unwrap();
        ensure!(
            text.len() <= cfg.max_text
                && !cfg
                    .blocked_terms
                    .iter()
                    .any(|t| !t.is_empty() && text.contains(t)),
            "text exceeds limit or matches a sensitive-content filter"
        );
        Ok(())
    }
    pub fn share_file(&self, path: PathBuf) -> Result<SharedFile> {
        ensure!(
            path.is_file() && path.metadata()?.len() <= MAX_WEB_FILE,
            "select a regular file up to 64 MiB"
        );
        let offer = transfer::offer(&path)?;
        transfer::Receiver::validate(&offer, MAX_WEB_FILE)?;
        let f = SharedFile {
            id: offer.id,
            name: offer.name,
            size: offer.size,
            hash: offer.hash,
        };
        let mut d = self.data.lock().unwrap();
        ensure!(d.active, "sharing stopped");
        ensure!(d.files.len() < 100, "shared file limit reached");
        d.files
            .insert(f.id.clone(), (f.clone(), path.canonicalize()?));
        Ok(f)
    }
    fn origin_check(&self, h: &HeaderMap) -> Result<()> {
        if let Some(origin) = h.get(header::ORIGIN) {
            ensure!(
                origin.to_str()? == self.origin,
                "cross-origin request denied"
            );
        }
        Ok(())
    }
    fn auth(&self, h: &HeaderMap) -> Result<String> {
        self.origin_check(h)?;
        let token = h
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or_else(|| anyhow::anyhow!("scan a fresh QR code to connect"))?;
        let mut d = self.data.lock().unwrap();
        d.sessions.retain(|_, (_, e)| *e > now());
        let (id, _) = d
            .sessions
            .get(token)
            .ok_or_else(|| anyhow::anyhow!("session expired or revoked"))?;
        Ok(id.clone())
    }
}
#[derive(Deserialize)]
struct Pair {
    token: String,
}
async fn pair(State(b): State<Arc<WebBridge>>, h: HeaderMap, Json(p): Json<Pair>) -> ApiResult {
    b.origin_check(&h).map_err(bad)?;
    let mut d = b.data.lock().unwrap();
    if !d
        .invite
        .as_ref()
        .is_some_and(|(t, e)| t == &p.token && *e > now())
    {
        return Err((
            StatusCode::UNAUTHORIZED,
            "expired or used invitation".into(),
        ));
    }
    d.sessions.retain(|_, (_, e)| *e > now());
    if d.sessions.len() >= 8 {
        return Err(bad("session limit; stop sharing to revoke devices"));
    }
    d.invite = None;
    let token = security::token();
    let expires = now() + SESSION_MS;
    d.sessions.insert(
        token.clone(),
        (format!("web-{}", uuid::Uuid::new_v4()), expires),
    );
    Ok(Json(
        serde_json::json!({"token": token, "expires": expires}),
    ))
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Action {
    Board,
    Text { text: String },
    Offer { offer: Offer },
    Task { id: String },
    Finish { id: String },
    Cancel { id: String },
}
async fn api(State(b): State<Arc<WebBridge>>, h: HeaderMap, Json(a): Json<Action>) -> ApiResult {
    let peer = b
        .auth(&h)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    let permit = b
        .operations
        .clone()
        .try_acquire_owned()
        .map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "busy; retry".into()))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let _guard = b.mutation.lock().unwrap();
        b.auth(&h)?;
        let value = match a {
            Action::Board => { let d = b.data.lock().unwrap(); serde_json::json!({"text": d.text, "files": d.files.values().map(|(f,_)|f).collect::<Vec<_>>()}) },
            Action::Text { text } => { b.validate_text(&text)?; b.service.copy_local(text.clone())?; b.share_text(text)?; serde_json::json!({"ok":true}) },
            Action::Offer { offer } => serde_json::to_value(b.service.web_offer(&peer,offer)?)?,
            Action::Task { id } => serde_json::to_value(b.service.web_task(&peer,&id)?)?,
            Action::Finish { id } => serde_json::to_value(b.service.web_finish(&peer,&id)?)?,
            Action::Cancel { id } => { b.service.web_cancel(&peer,&id)?; serde_json::json!({"ok":true}) },
        }; anyhow::Ok(Json(value))
    }).await.map_err(bad)?.map_err(bad)
}
async fn chunk(
    State(b): State<Arc<WebBridge>>,
    h: HeaderMap,
    Path((id, offset)): Path<(String, u64)>,
    bytes: Bytes,
) -> ApiResult {
    let peer = b
        .auth(&h)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;
    if bytes.len() > CHUNK {
        return Err(bad("chunk too large"));
    }
    let permit = b
        .operations
        .clone()
        .try_acquire_owned()
        .map_err(|_| (StatusCode::TOO_MANY_REQUESTS, "busy; retry".into()))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let _guard = b.mutation.lock().unwrap();
        b.auth(&h)?;
        b.service
            .web_chunk(&peer, &id, offset, &bytes)
            .map(|offset| Json(serde_json::json!({"offset":offset})))
    })
    .await
    .map_err(bad)?
    .map_err(bad)
}
async fn download(
    State(b): State<Arc<WebBridge>>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Err(e) = b.auth(&h) {
        return (StatusCode::UNAUTHORIZED, e.to_string()).into_response();
    }
    let Some((meta, path)) = b.data.lock().unwrap().files.get(&id).cloned() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match tokio::fs::File::open(path).await {
        Ok(file) => {
            if !file
                .metadata()
                .await
                .is_ok_and(|m| m.is_file() && m.len() == meta.size && m.len() <= MAX_WEB_FILE)
            {
                return (StatusCode::CONFLICT, "file changed; share it again").into_response();
            }
            let disposition = format!(
                "attachment; filename*=UTF-8''{}",
                meta.name
                    .bytes()
                    .map(|c| format!("%{c:02X}"))
                    .collect::<String>()
            );
            (
                [
                    (header::CONTENT_TYPE, "application/octet-stream".to_string()),
                    (header::CONTENT_DISPOSITION, disposition),
                ],
                Body::from_stream(tokio_util::io::ReaderStream::new(file.take(meta.size))),
            )
                .into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    struct Harness {
        _dir: tempfile::TempDir,
        service: Arc<Service>,
        bridge: Arc<WebBridge>,
        client: reqwest::Client,
    }
    impl Drop for Harness {
        fn drop(&mut self) {
            self.bridge.stop();
        }
    }
    impl Harness {
        async fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let service = Service::open(dir.path()).unwrap();
            let bridge = WebBridge::start(service.clone(), "127.0.0.1:0")
                .await
                .unwrap();
            // Only loopback tests bypass the ephemeral browser certificate's trust warning.
            let client = reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            Self {
                _dir: dir,
                service,
                bridge,
                client,
            }
        }
        async fn pair(&self) -> String {
            let invite = self.bridge.invitation().unwrap();
            assert!(invite.qr_svg.contains("<svg"));
            assert_eq!(invite.certificate_sha256.len(), 64);
            let value: Value = self
                .client
                .post(format!("{}/api/pair", self.bridge.origin))
                .json(&json!({"token":invite.url.split('#').nth(1).unwrap()}))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .json()
                .await
                .unwrap();
            value["token"].as_str().unwrap().into()
        }
        async fn api(&self, token: &str, action: Value) -> reqwest::Response {
            self.client
                .post(format!("{}/api", self.bridge.origin))
                .bearer_auth(token)
                .json(&action)
                .send()
                .await
                .unwrap()
        }
    }
    #[tokio::test]
    async fn https_qr_pairing_expiry_origin_and_revocation() {
        let h = Harness::new().await;
        assert!(reqwest::Client::new()
            .get(&h.bridge.origin)
            .send()
            .await
            .is_err());
        let page = h.client.get(&h.bridge.origin).send().await.unwrap();
        assert_eq!(page.headers()["cache-control"], "no-store");
        assert!(page.text().await.unwrap().contains("手机中转"));
        assert_eq!(
            h.api("unknown", json!({"action":"board"})).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let invite = h.bridge.invitation().unwrap();
        let code = invite.url.split('#').nth(1).unwrap();
        let url = format!("{}/api/pair", h.bridge.origin);
        let evil = h
            .client
            .post(&url)
            .header("origin", "https://evil.invalid")
            .json(&json!({"token":code}))
            .send()
            .await
            .unwrap();
        assert_eq!(evil.status(), StatusCode::BAD_REQUEST);
        let r = h
            .client
            .post(&url)
            .json(&json!({"token":code}))
            .send()
            .await
            .unwrap();
        assert!(r.status().is_success());
        assert_eq!(
            h.client
                .post(&url)
                .json(&json!({"token":code}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let invite = h.bridge.invitation().unwrap();
        h.bridge.data.lock().unwrap().invite.as_mut().unwrap().1 = now() - 1;
        assert_eq!(
            h.client
                .post(&url)
                .json(&json!({"token":invite.url.split('#').nth(1).unwrap()}))
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let token = h.pair().await;
        assert!(h
            .api(&token, json!({"action":"board"}))
            .await
            .status()
            .is_success());
        h.bridge
            .data
            .lock()
            .unwrap()
            .sessions
            .get_mut(&token)
            .unwrap()
            .1 = now() - 1;
        assert_eq!(
            h.api(&token, json!({"action":"board"})).await.status(),
            StatusCode::UNAUTHORIZED
        );
        let token = h.pair().await;
        h.bridge.stop();
        assert!(h
            .bridge
            .auth(&HeaderMap::from_iter([(
                header::AUTHORIZATION,
                format!("Bearer {token}").parse().unwrap()
            )]))
            .is_err());
        assert!(h.client.get(&h.bridge.origin).send().await.is_err());
    }
    #[tokio::test]
    async fn shared_clipboard_is_explicit_bounded_and_filtered() {
        let h = Harness::new().await;
        let token = h.pair().await;
        let board: Value = h
            .api(&token, json!({"action":"board"}))
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(board["text"], "");
        h.bridge.share_text("explicit PC text".into()).unwrap();
        let board: Value = h
            .api(&token, json!({"action":"board"}))
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(board["text"], "explicit PC text");
        assert!(h
            .api(&token, json!({"action":"text","text":"phone text"}))
            .await
            .status()
            .is_success());
        assert_eq!(h.service.snapshot().history.len(), 0);
        assert_eq!(
            h.api(&token, json!({"action":"text","text":"password=secret"}))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert!(h.bridge.share_text("x".repeat(1024 * 1024 + 1)).is_err());
        assert!(h.service.snapshot().tasks.is_empty());
    }
    #[tokio::test]
    async fn browser_upload_approval_isolation_resume_hash_cancel_and_download() {
        let h = Harness::new().await;
        let a = h.pair().await;
        let b = h.pair().await;
        let content = vec![42; CHUNK + 17];
        let offer = Offer {
            id: uuid::Uuid::new_v4().to_string(),
            name: "phone.bin".into(),
            size: content.len() as u64,
            hash: crate::clipboard::hash(&content),
        };
        assert!(h
            .api(&a, json!({"action":"offer","offer":offer}))
            .await
            .status()
            .is_success());
        assert_eq!(
            h.api(&a, json!({"action":"offer","offer":offer}))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            h.api(&b, json!({"action":"task","id":offer.id}))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        let url = format!("{}/api/chunk/{}/0", h.bridge.origin, offer.id);
        assert_eq!(
            h.client
                .post(&url)
                .bearer_auth(&a)
                .body(content[..CHUNK].to_vec())
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        h.service.decide(&offer.id, true).unwrap();
        assert_eq!(
            h.client
                .post(&url)
                .bearer_auth(&b)
                .body(content[..CHUNK].to_vec())
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert!(h
            .client
            .post(&url)
            .bearer_auth(&a)
            .body(content[..CHUNK].to_vec())
            .send()
            .await
            .unwrap()
            .status()
            .is_success());
        // A new HTTPS connection after interruption reads the durable acknowledged offset.
        let task: Value = h
            .api(&a, json!({"action":"task","id":offer.id}))
            .await
            .json()
            .await
            .unwrap();
        assert_eq!(task["offset"], CHUNK);
        assert_eq!(
            h.client
                .post(&url)
                .bearer_auth(&a)
                .body(content[..CHUNK].to_vec())
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            h.api(&a, json!({"action":"finish","id":offer.id}))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert!(h
            .client
            .post(format!(
                "{}/api/chunk/{}/{}",
                h.bridge.origin, offer.id, CHUNK
            ))
            .bearer_auth(&a)
            .body(content[CHUNK..].to_vec())
            .send()
            .await
            .unwrap()
            .status()
            .is_success());
        assert!(h
            .api(&a, json!({"action":"finish","id":offer.id}))
            .await
            .status()
            .is_success());
        let path = PathBuf::from(&h.service.config.lock().unwrap().receive_dir).join("phone.bin");
        assert_eq!(std::fs::read(&path).unwrap(), content);
        let shared = h.bridge.share_file(path).unwrap();
        let url = format!("{}/api/download/{}", h.bridge.origin, shared.id);
        assert_eq!(
            h.client.get(&url).send().await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
        let r = h.client.get(&url).bearer_auth(&a).send().await.unwrap();
        assert!(r.headers()["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment;"));
        assert_eq!(r.bytes().await.unwrap().as_ref(), content);
        let mut wrong = offer.clone();
        wrong.id = uuid::Uuid::new_v4().to_string();
        wrong.size = 0;
        wrong.name = "empty.bin".into();
        wrong.hash = crate::clipboard::hash(b"wrong");
        assert!(h
            .api(&a, json!({"action":"offer","offer":wrong}))
            .await
            .status()
            .is_success());
        h.service.decide(&wrong.id, true).unwrap();
        assert_eq!(
            h.api(&a, json!({"action":"finish","id":wrong.id}))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert!(h
            .api(&a, json!({"action":"cancel","id":wrong.id}))
            .await
            .status()
            .is_success());
        wrong.id = uuid::Uuid::new_v4().to_string();
        wrong.name = "../escape".into();
        assert_eq!(
            h.api(&a, json!({"action":"offer","offer":wrong}))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        // A valid zero-byte upload completes, while an existing name is never overwritten.
        let mut empty = Offer {
            id: uuid::Uuid::new_v4().to_string(),
            name: "empty-ok.bin".into(),
            size: 0,
            hash: crate::clipboard::hash(b""),
        };
        assert!(h
            .api(&a, json!({"action":"offer","offer":empty}))
            .await
            .status()
            .is_success());
        h.service.decide(&empty.id, true).unwrap();
        assert!(h
            .api(&a, json!({"action":"finish","id":empty.id}))
            .await
            .status()
            .is_success());
        assert!(h
            .api(&a, json!({"action":"finish","id":empty.id}))
            .await
            .status()
            .is_success());
        empty.id = uuid::Uuid::new_v4().to_string();
        assert!(h
            .api(&a, json!({"action":"offer","offer":empty}))
            .await
            .status()
            .is_success());
        h.service.decide(&empty.id, true).unwrap();
        assert_eq!(
            h.api(&a, json!({"action":"finish","id":empty.id}))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert!(h
            .api(&a, json!({"action":"cancel","id":empty.id}))
            .await
            .status()
            .is_success());
        wrong.name = "large.bin".into();
        wrong.size = MAX_WEB_FILE + 1;
        assert_eq!(
            h.api(&a, json!({"action":"offer","offer":wrong}))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    #[tokio::test]
    async fn wildcard_bind_is_rejected() {
        let d = tempfile::tempdir().unwrap();
        let s = Service::open(d.path()).unwrap();
        assert!(WebBridge::start(s, "0.0.0.0:0").await.is_err());
    }
}
