//! Headless diagnostic runner. Clipboard contents are never printed.
use meshclip_core::{model::Ticket, Service};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| ".meshclip-agent".into());
    let cmd = args.next().unwrap_or_else(|| "serve".into());
    let service = Service::open(std::path::Path::new(&dir))?;
    match cmd.as_str(){
 "serve"=>{service.start_monitor();service.start_clipboard();println!("MeshClip agent listening at {} (external EasyTier mode)",service.config.lock().unwrap().listen);service.listen().await?;},
 "configure"=>{let path=args.next().ok_or_else(||anyhow::anyhow!("configure requires a config JSON path"))?;service.configure(serde_json::from_slice(&std::fs::read(path)?)?)?;},
 "pair"=>{let path=args.next().ok_or_else(||anyhow::anyhow!("pair requires an invitation JSON path"))?;let ticket:Ticket=serde_json::from_slice(&std::fs::read(path)?)?;service.pair(ticket).await?;println!("Paired; restart any running agent to load trust changes");},
 "check"=>{let cfg=service.config.lock().unwrap().clone();println!("Device {}: {} trusted peers; auto sync {}",cfg.name,cfg.peers.len(),cfg.enabled);},
 _=>anyhow::bail!("usage: meshclip-agent [state-dir] [serve|configure config.json|pair invitation.json|check]")
 };
    Ok(())
}
