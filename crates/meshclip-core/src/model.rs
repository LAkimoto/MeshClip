use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const VERSION: u16 = 1;
pub const CHUNK: usize = 256 * 1024;
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub address: String,
    pub cert: Vec<u8>,
    pub public_key: Vec<u8>,
    pub clipboard: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub name: String,
    pub listen: String,
    pub receive_dir: String,
    pub enabled: bool,
    pub max_text: usize,
    pub history_limit: usize,
    pub history_hours: u64,
    pub blocked_terms: Vec<String>,
    pub ignored_apps: Vec<String>,
    pub max_file: u64,
    pub chunk_size: usize,
    pub timeout_seconds: u64,
    pub transfer_concurrency: usize,
    pub peers: BTreeMap<String, Peer>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            name: "My device".into(),
            listen: "127.0.0.1:47831".into(),
            receive_dir: "received".into(),
            enabled: false,
            max_text: 1024 * 1024,
            history_limit: 0,
            history_hours: 24,
            blocked_terms: vec![
                "-----BEGIN PRIVATE KEY".into(),
                "password=".into(),
                "Bearer ".into(),
            ],
            ignored_apps: vec![],
            max_file: 16 * 1024 * 1024 * 1024,
            chunk_size: CHUNK,
            timeout_seconds: 15,
            transfer_concurrency: 2,
            peers: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub source: String,
    pub timestamp: u64,
    pub hash: String,
    pub text: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Offer {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub hash: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Task {
    pub offer: Offer,
    pub peer: String,
    pub direction: String,
    pub status: String,
    pub offset: u64,
    pub error: Option<String>,
    pub bytes_per_second: u64,
    pub eta_seconds: Option<u64>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Status {
    pub listening: bool,
    pub error: Option<String>,
    pub clipboard_error: Option<String>,
    pub peers: BTreeMap<String, String>,
    pub history: Vec<Clip>,
    pub tasks: BTreeMap<String, Task>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Ticket {
    pub peer: Peer,
    pub token: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Request {
    Pair { peer: Peer, token: String },
    Ping,
    Clipboard(Clip),
    Offer(Offer),
    Chunk { id: String, offset: u64 },
    Finish { id: String },
    Cancel { id: String },
}
#[derive(Serialize, Deserialize)]
pub struct Auth {
    pub version: u16,
    pub id: String,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
    pub request: Request,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    pub message: String,
    pub offset: u64,
}
impl Reply {
    pub fn ok(offset: u64) -> Self {
        Self {
            ok: true,
            message: "ok".into(),
            offset,
        }
    }
}
