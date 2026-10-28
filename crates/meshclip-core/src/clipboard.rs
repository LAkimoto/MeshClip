use crate::model::{now, Clip, Config};
use sha2::{Digest, Sha256};
use std::collections::{HashSet, VecDeque};
pub fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
#[derive(Default)]
pub struct Filter {
    seen: HashSet<String>,
    order: VecDeque<String>,
    last_hash: String,
    latest: (u64, String),
}
impl Filter {
    pub fn next_timestamp(&self) -> u64 {
        now().max(self.latest.0.saturating_add(1))
    }
    pub fn admit(&mut self, c: &Clip, cfg: &Config) -> bool {
        if !cfg.enabled
            || c.text.len() > cfg.max_text
            || c.hash != hash(c.text.as_bytes())
            || c.id.len() > 64
            || c.source.len() != 64
            || c.timestamp > now() + 300_000
            || cfg
                .blocked_terms
                .iter()
                .any(|s| !s.is_empty() && c.text.contains(s))
            || self.seen.contains(&c.id)
        {
            return false;
        }
        self.seen.insert(c.id.clone());
        self.order.push_back(c.id.clone());
        if self.order.len() > 4096 {
            if let Some(id) = self.order.pop_front() {
                self.seen.remove(&id);
            }
        }
        let clock = (c.timestamp, c.id.clone());
        if clock <= self.latest {
            return false;
        }
        self.latest = clock;
        if self.last_hash == c.hash {
            return false;
        }
        self.last_hash = c.hash.clone();
        true
    }
}
