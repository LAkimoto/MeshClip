use crate::{
    clipboard::hash,
    model::{Offer, CHUNK},
    storage,
};
use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
#[derive(Serialize, Deserialize)]
struct Meta {
    offer: Offer,
    peer: String,
    offset: u64,
}
pub struct Receiver {
    root: PathBuf,
    staging: PathBuf,
    max: u64,
}
impl Receiver {
    pub fn new(root: &Path, state: &Path, max: u64) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let staging = state.join("incoming");
        std::fs::create_dir_all(&staging)?;
        Ok(Self {
            root: root.canonicalize()?,
            staging,
            max,
        })
    }
    fn paths(&self, id: &str) -> Result<(PathBuf, PathBuf)> {
        uuid::Uuid::parse_str(id)?;
        Ok((
            self.staging.join(format!("{id}.part")),
            self.staging.join(format!("{id}.json")),
        ))
    }
    pub fn validate(o: &Offer, max: u64) -> Result<()> {
        uuid::Uuid::parse_str(&o.id)?;
        ensure!(o.size <= max, "file exceeds limit");
        ensure!(
            o.hash.len() == 64 && hex::decode(&o.hash)?.len() == 32,
            "invalid digest"
        );
        let n = &o.name;
        ensure!(
            !n.is_empty()
                && n.len() < 200
                && !n.contains(['/', '\\', ':', '\0'])
                && !n.chars().any(|c| c.is_control())
                && !n.ends_with(['.', ' '])
                && n != "."
                && n != "..",
            "unsafe filename"
        );
        let stem = n.split('.').next().unwrap_or("").to_uppercase();
        ensure!(
            ![
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
                "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
                "LPT9"
            ]
            .contains(&stem.as_str()),
            "reserved filename"
        );
        Ok(())
    }
    pub fn accept(&self, o: &Offer, peer: &str) -> Result<u64> {
        Self::validate(o, self.max)?;
        let (part, meta) = self.paths(&o.id)?;
        if meta.exists() {
            let m: Meta = storage::load(&meta)?;
            ensure!(
                m.peer == peer
                    && m.offer.hash == o.hash
                    && m.offer.size == o.size
                    && m.offer.name == o.name,
                "task identity mismatch"
            );
            ensure!(
                fs2::available_space(&self.staging)? >= o.size.saturating_sub(m.offset),
                "insufficient disk space"
            );
            let mut f = std::fs::OpenOptions::new().write(true).open(part)?;
            ensure!(f.metadata()?.len() >= m.offset, "truncated partial file");
            f.set_len(m.offset)?;
            f.flush()?;
            return Ok(m.offset);
        }
        ensure!(
            fs2::available_space(&self.staging)? >= o.size,
            "insufficient disk space"
        );
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(part)?;
        storage::save(
            &meta,
            &Meta {
                offer: o.clone(),
                peer: peer.into(),
                offset: 0,
            },
        )?;
        Ok(0)
    }
    pub fn chunk(&self, id: &str, peer: &str, offset: u64, bytes: &[u8]) -> Result<u64> {
        let (part, meta) = self.paths(id)?;
        let mut m: Meta = storage::load(&meta)?;
        ensure!(m.peer == peer, "unauthorized task");
        ensure!(
            !bytes.is_empty()
                && bytes.len() <= CHUNK
                && offset == m.offset
                && offset + bytes.len() as u64 <= m.offer.size,
            "invalid chunk offset/size"
        );
        let mut f = std::fs::OpenOptions::new().write(true).open(part)?;
        f.seek(SeekFrom::Start(offset))?;
        f.write_all(bytes)?;
        f.sync_data()?;
        m.offset += bytes.len() as u64;
        storage::save(&meta, &m)?;
        Ok(m.offset)
    }
    pub fn finish(&self, id: &str, peer: &str) -> Result<u64> {
        let (part, meta) = self.paths(id)?;
        let m: Meta = storage::load(&meta)?;
        ensure!(
            m.peer == peer && m.offset == m.offer.size,
            "incomplete/unauthorized task"
        );
        ensure!(digest(&part)? == m.offer.hash, "SHA-256 mismatch");
        let dest = self.root.join(&m.offer.name);
        if dest.exists() {
            bail!("destination exists; rename the offer or cancel")
        }; // hard_link is atomic create-without-overwrite; staging must share filesystem.
        std::fs::hard_link(&part, &dest)?;
        std::fs::remove_file(part)?;
        std::fs::remove_file(meta)?;
        Ok(m.offset)
    }
    pub fn cancel(&self, id: &str, peer: &str) -> Result<()> {
        let (part, meta) = self.paths(id)?;
        let m: Meta = storage::load(&meta)?;
        ensure!(m.peer == peer, "unauthorized task");
        std::fs::remove_file(part)?;
        std::fs::remove_file(meta)?;
        Ok(())
    }
}
pub fn digest(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut b = vec![0; CHUNK];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(hex::encode(h.finalize()))
}
pub fn offer(path: &Path) -> Result<Offer> {
    Ok(Offer {
        id: uuid::Uuid::new_v4().to_string(),
        name: path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("missing filename"))?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("non-UTF8 filename"))?
            .into(),
        size: std::fs::metadata(path)?.len(),
        hash: digest(path)?,
    })
}
pub fn chunk_hash(bytes: &[u8]) -> String {
    hash(bytes)
}

/// Explicit folder send: creates a TAR archive; receivers never extract it automatically.
pub fn pack_folder(path: &Path, out: &Path, max: u64) -> Result<PathBuf> {
    ensure!(path.is_dir(), "select a folder");
    let source = path.canonicalize()?;
    let target = out.canonicalize()?;
    ensure!(
        !target.starts_with(&source),
        "archive output must be outside source folder"
    );
    fn check(p: &Path, total: &mut u64, max: u64) -> Result<()> {
        for item in std::fs::read_dir(p)? {
            let item = item?;
            let meta = std::fs::symlink_metadata(item.path())?;
            ensure!(
                !meta.file_type().is_symlink(),
                "folder contains a symbolic link"
            );
            if meta.is_dir() {
                check(&item.path(), total, max)?
            } else {
                ensure!(meta.is_file(), "folder contains a special file");
                *total = total
                    .checked_add(meta.len() + 1024)
                    .ok_or_else(|| anyhow::anyhow!("folder too large"))?;
                ensure!(*total <= max, "folder exceeds transfer limit");
            }
        }
        Ok(())
    }
    let mut total = 2048;
    check(&source, &mut total, max)?;
    ensure!(
        fs2::available_space(out)? >= total,
        "insufficient disk space for folder archive"
    );
    let name = source
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("invalid folder name"))?;
    let destination = out.join(format!("{name}.tar"));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)?;
    let mut archive = tar::Builder::new(file);
    archive.follow_symlinks(false);
    archive.append_dir_all(name, &source)?;
    archive.finish()?;
    ensure!(
        std::fs::metadata(&destination)?.len() <= max,
        "archive exceeds transfer limit"
    );
    Ok(destination)
}
