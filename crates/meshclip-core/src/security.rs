use crate::model::Peer;
#[cfg(not(windows))]
use crate::storage;
use anyhow::{bail, Result};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Serialize, Deserialize)]
pub struct Identity {
    pub cert: Vec<u8>,
    pub tls_key: Vec<u8>,
    pub signing: [u8; 32],
}
impl Identity {
    pub fn load(dir: &Path) -> Result<Self> {
        #[cfg(windows)]
        {
            let entry = keyring::Entry::new("MeshClip", &dir.to_string_lossy())?;
            match entry.get_secret() {
                Ok(s) => return Self::from_credential_blob(&s),
                Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(e.into()),
            };
            let id = Self::generate()?;
            entry.set_secret(&id.credential_blob()?)?;
            Ok(id)
        }
        #[cfg(not(windows))]
        {
            let p = dir.join("identity.json");
            if p.exists() {
                return storage::load(&p);
            }
            let id = Self::generate()?;
            storage::save(&p, &id)?;
            Ok(id)
        }
    }
    /// Compact UTF-8 bytes avoid the Windows password API's UTF-16 expansion.
    pub fn credential_blob(&self) -> Result<Vec<u8>> {
        let blob = serde_json::to_vec(&(
            hex::encode(&self.cert),
            hex::encode(&self.tls_key),
            hex::encode(self.signing),
        ))?;
        anyhow::ensure!(
            blob.len() <= 2560,
            "identity exceeds Windows credential blob limit"
        );
        Ok(blob)
    }
    pub fn from_credential_blob(blob: &[u8]) -> Result<Self> {
        let (cert, key, signing): (String, String, String) = serde_json::from_slice(blob)?;
        Ok(Self {
            cert: hex::decode(cert)?,
            tls_key: hex::decode(key)?,
            signing: hex::decode(signing)?
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid signing key length"))?,
        })
    }
    pub fn generate() -> Result<Self> {
        let c = rcgen::generate_simple_self_signed(vec!["meshclip.local".into()])?;
        Ok(Self {
            cert: c.cert.der().to_vec(),
            tls_key: c.key_pair.serialize_der(),
            signing: SigningKey::generate(&mut OsRng).to_bytes(),
        })
    }
    pub fn public(&self) -> Vec<u8> {
        SigningKey::from_bytes(&self.signing)
            .verifying_key()
            .to_bytes()
            .to_vec()
    }
    pub fn id(&self) -> String {
        hex::encode(self.public())
    }
    pub fn peer(&self, name: String, address: String) -> Peer {
        Peer {
            id: self.id(),
            name,
            address,
            cert: self.cert.clone(),
            public_key: self.public(),
            clipboard: false,
        }
    }
    pub fn sign(&self, data: &[u8]) -> Vec<u8> {
        SigningKey::from_bytes(&self.signing)
            .sign(data)
            .to_bytes()
            .to_vec()
    }
}
pub fn verify(key: &[u8], data: &[u8], signature: &[u8]) -> Result<()> {
    let a: [u8; 32] = key.try_into()?;
    VerifyingKey::from_bytes(&a)?.verify(data, &Signature::from_slice(signature)?)?;
    Ok(())
}
pub fn validate_peer(p: &Peer) -> Result<()> {
    if p.id != hex::encode(&p.public_key)
        || p.public_key.len() != 32
        || p.cert.len() > 8192
        || p.name.len() > 128
    {
        bail!("invalid identity")
    };
    let _: std::net::SocketAddr = p.address.parse()?;
    Ok(())
}
pub fn token() -> String {
    let mut b = [0; 32];
    OsRng.fill_bytes(&mut b);
    hex::encode(b)
}
