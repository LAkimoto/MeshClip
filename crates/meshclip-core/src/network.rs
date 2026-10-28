//! Managed network runtime. EasyTier stays a replaceable, separately licensed process.
use crate::Service;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::Path, process::Stdio};
use tokio::{
    process::{Child, Command},
    task::JoinHandle,
};
#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Lan,
    Easytier,
}
#[derive(Clone, Deserialize)]
pub struct StartOptions {
    pub mode: Mode,
    pub address: String,
    pub config_file: Option<String>,
}
#[derive(Clone, Default, Serialize)]
pub struct NetworkStatus {
    pub mode: Option<String>,
    pub running: bool,
    pub kernel_running: bool,
    pub error: Option<String>,
}
#[derive(Default)]
pub struct NetworkRuntime {
    process: Option<Child>,
    listener: Option<JoinHandle<()>>,
    status: NetworkStatus,
}
impl NetworkRuntime {
    pub fn status(&mut self) -> NetworkStatus {
        if let Some(p) = self.process.as_mut() {
            if !matches!(p.try_wait(), Ok(None)) {
                self.status.error =
                    Some("EasyTier 内核已退出；请停止服务并检查配置、驱动与管理员权限".into());
                self.status.kernel_running = false;
                if let Some(h) = self.listener.take() {
                    h.abort();
                }
                self.status.running = false;
            }
        }
        if self.listener.as_ref().is_some_and(|h| h.is_finished()) {
            self.status.running = false;
        }
        self.status.clone()
    }
    pub async fn stop(&mut self, service: &Service) -> Result<()> {
        if let Some(h) = self.listener.take() {
            h.abort();
            let _ = h.await;
        }
        if let Some(mut p) = self.process.take() {
            if matches!(p.try_wait(), Ok(None)) {
                p.kill().await?;
            }
        }
        service.status.lock().unwrap().listening = false;
        self.status = NetworkStatus::default();
        Ok(())
    }
    pub async fn start(
        &mut self,
        service: std::sync::Arc<Service>,
        options: StartOptions,
        kernel: &Path,
    ) -> Result<()> {
        ensure!(
            self.listener.is_none() && self.process.is_none(),
            "请先停止当前服务"
        );
        let address: SocketAddr = options.address.parse()?;
        ensure!(
            !address.ip().is_unspecified() && !address.ip().is_multicast() && address.port() != 0,
            "请选择本机具体 IP 和非零端口"
        );
        if matches!(options.mode, Mode::Easytier) {
            ensure!(
                kernel.is_file(),
                "安装包未包含 EasyTier 内核，请使用集成构建包"
            );
            let config = std::fs::canonicalize(
                options
                    .config_file
                    .ok_or_else(|| anyhow::anyhow!("请选择 EasyTier TOML 配置"))?,
            )?;
            ensure!(config.is_file(), "配置文件不存在");
            let mut command = Command::new(kernel);
            command
                .arg("--config-file")
                .arg(config)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            #[cfg(windows)]
            command.creation_flags(0x08000000);
            self.process = Some(
                command
                    .spawn()
                    .map_err(|_| anyhow::anyhow!("无法启动 EasyTier 内核，请检查文件与权限"))?,
            );
            self.status.kernel_running = true;
        }
        let mut cfg = service.config.lock().unwrap().clone();
        cfg.listen = options.address.clone();
        if let Err(e) = service.configure(cfg) {
            let _ = self.stop(&service).await;
            return Err(e);
        }
        // Wait for the selected local adapter address, not just process creation.
        let mut ready = false;
        for _ in 0..100 {
            if self
                .process
                .as_mut()
                .is_some_and(|p| !matches!(p.try_wait(), Ok(None)))
            {
                break;
            }
            if let Ok(socket) = tokio::net::TcpListener::bind(address).await {
                drop(socket);
                ready = true;
                break;
            }
            if matches!(options.mode, Mode::Lan) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        if !ready {
            self.stop(&service).await?;
            anyhow::bail!(
                "无法绑定所选 IP:端口，请检查地址占用、EasyTier 配置、TUN 驱动及管理员权限"
            );
        }
        service.status.lock().unwrap().error = None;
        self.status = NetworkStatus {
            mode: Some(
                match options.mode {
                    Mode::Lan => "lan",
                    Mode::Easytier => "easytier",
                }
                .into(),
            ),
            running: false,
            kernel_running: self.process.is_some(),
            error: None,
        };
        let s = service.clone();
        self.listener = Some(tokio::spawn(async move {
            if let Err(e) = s.clone().listen().await {
                s.status.lock().unwrap().error = Some(e.to_string());
            }
        }));
        for _ in 0..50 {
            if service.snapshot().listening {
                self.status.running = true;
                return Ok(());
            }
            if self.listener.as_ref().is_some_and(|h| h.is_finished()) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        self.stop(&service).await?;
        anyhow::bail!("MeshClip 应用服务启动失败，请检查绑定地址与端口")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn lan_start_stop_and_missing_kernel() {
        let dir = tempfile::tempdir().unwrap();
        let service = Service::open(dir.path()).unwrap();
        let mut r = NetworkRuntime::default();
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let options = StartOptions {
            mode: Mode::Lan,
            address: format!("127.0.0.1:{port}"),
            config_file: None,
        };
        r.start(service.clone(), options.clone(), Path::new("missing.exe"))
            .await
            .unwrap();
        assert!(r.status().running);
        assert!(!r.status().kernel_running);
        assert!(r
            .start(service.clone(), options, Path::new("missing.exe"))
            .await
            .is_err());
        r.stop(&service).await.unwrap();
        assert!(!service.snapshot().listening);
        assert!(r
            .start(
                service.clone(),
                StartOptions {
                    mode: Mode::Easytier,
                    address: format!("127.0.0.1:{port}"),
                    config_file: None
                },
                Path::new("missing.exe")
            )
            .await
            .is_err());
        assert!(r
            .start(
                service,
                StartOptions {
                    mode: Mode::Lan,
                    address: "0.0.0.0:47831".into(),
                    config_file: None
                },
                Path::new("missing.exe")
            )
            .await
            .is_err());
    }
}
