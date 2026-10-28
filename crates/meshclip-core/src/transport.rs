use crate::{model::CHUNK, security::Identity};
use anyhow::{bail, Result};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
pub async fn write<S: AsyncWrite + Unpin, T: Serialize>(
    s: &mut S,
    value: &T,
    body: &[u8],
) -> Result<()> {
    let b = serde_json::to_vec(value)?;
    if b.len() > 2 * 1024 * 1024 || body.len() > CHUNK {
        bail!("frame too large")
    }
    s.write_u32(b.len() as u32).await?;
    s.write_all(&b).await?;
    s.write_u32(body.len() as u32).await?;
    s.write_all(body).await?;
    s.flush().await?;
    Ok(())
}
pub async fn read<S: AsyncRead + Unpin, T: DeserializeOwned>(s: &mut S) -> Result<(T, Vec<u8>)> {
    let len = s.read_u32().await? as usize;
    if len > 2 * 1024 * 1024 {
        bail!("control frame too large")
    }
    let mut b = vec![0; len];
    s.read_exact(&mut b).await?;
    let v = serde_json::from_slice(&b)?;
    let n = s.read_u32().await? as usize;
    if n > CHUNK {
        bail!("chunk too large")
    }
    let mut body = vec![0; n];
    s.read_exact(&mut body).await?;
    Ok((v, body))
}
pub fn server(id: &Identity) -> Result<tokio_rustls::TlsAcceptor> {
    let c = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(id.cert.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(id.tls_key.clone())),
        )?;
    Ok(tokio_rustls::TlsAcceptor::from(Arc::new(c)))
}
pub async fn connect(
    address: &str,
    cert: &[u8],
) -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>> {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(CertificateDer::from(cert.to_vec()))?;
    let cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let tcp = tokio::net::TcpStream::connect(address).await?;
    tcp.set_nodelay(true)?;
    Ok(tokio_rustls::TlsConnector::from(Arc::new(cfg))
        .connect("meshclip.local".try_into()?, tcp)
        .await?)
}
