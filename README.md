# MeshClip

Private clipboard synchronization and lightweight peer-to-peer file relay over
an **existing EasyTier virtual network**. Rust + Tokio + rustls; Tauri 2 + Vue 3 +
TypeScript desktop UI. MIT licensed. No cloud file server, VPN implementation or
bundled EasyTier binaries.

> **Development preview, not an accepted Windows MVP.** Core network tests and
> frontend build run locally. A Windows desktop binary has not been built or
> launched, and two real EasyTier-connected Windows devices have not been tested.
> See [progress](docs/progress.md) for exact evidence and remaining blockers.

## Implemented

- Device identity, independent app-layer trust, revocation, TLS certificate pinning
  and Ed25519 challenge signatures. Single-use 120-second pairing invitations.
- Text polling, pause/resume, per-device permission, bounded deduplication, deterministic
  concurrent-event ordering and latest-value reconnection retry. 1 MiB text limit.
- Configurable substring filtering and application exclusion interface; Windows
  foreground-process adapter exists but still needs actual Windows verification.
- Memory-only clipboard history with disabled-by-default retention, search, copy
  and clear. Clipboard bodies are never intentionally logged or persisted.
- Real single/multiple file transfer with receiver approval, durable 256 KiB chunks,
  cancellation, retry/resume, SHA-256 verification, progress, speed and ETA.
- Explicit folder-to-TAR sending. No automatic unpacking or file execution.
- Persistent identity/config/trust/task records; Windows Credential Manager backend;
  separate receive staging; path and filename validation, disk-space checks.
- Five UI pages connected to backend state, tray/background and autostart wiring.
  A browser preview clearly reports that the native backend is disconnected.

## Start on Windows 10/11 x64

Install [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/):
Visual Studio Build Tools with Desktop development with C++, Windows SDK,
WebView2, Rust stable (MSVC) and Node.js 22 LTS or later. Then:

```powershell
.\scripts\dev-windows.ps1
```

Or manually:

```powershell
cargo test -p meshclip-core --locked
cd apps/desktop
npm ci
npm run tauri dev
# Release / unsigned NSIS installer:
npm run tauri build -- --bundles nsis
```

Build output: `target/release/bundle/nsis/`. The workflow in
`.github/workflows/ci.yml` builds/tests the core on Linux and Windows and attempts
an unsigned Windows desktop installer. It has not run remotely yet.

## Connect two devices

1. Start EasyTier separately on both devices. Confirm their virtual IPs can reach
   each other. MeshClip never starts or reconfigures EasyTier in this release.
2. In Settings, choose this device's own virtual IP and port (e.g.
   `10.144.144.1:47831`). Save and **exit through the tray**, then restart. The
   initial bind is loopback; wildcard listeners are forbidden. Allow the chosen
   app/port through the host firewall where required; no firewall rules are
   modified automatically. Configure the receive directory on the same volume as
   application state (cross-volume finalization is not implemented).
3. On device B, generate an invitation. Send the complete JSON privately to A
   through a channel you trust. Paste it into A's Devices page within 120 seconds.
   This JSON contains a one-time credential; do not post it publicly.
4. After pairing, separately enable clipboard permission for the peer **on both
   devices** and enable automatic text synchronization. Unknown devices have no
   access. Filter rules may intentionally suppress delivery.
5. Select a trusted device in Files, choose files/a folder, or drop paths into the
   window. B must accept each offer. A then clicks Retry to begin/resume. Folder
   sends are TAR files and are not extracted automatically.
6. Closing the window hides it to the tray. Use the tray Exit action to stop.

## Checks and diagnostics

```sh
./scripts/check.sh
# Core-only diagnostic runner, no GUI dependencies:
cargo run -p meshclip-core --bin meshclip-agent -- /tmp/meshclip-state check
cargo run -p meshclip-core --bin meshclip-agent -- /tmp/meshclip-state serve
```

Agent state is isolated from the desktop unless the same state directory is
explicitly selected. `configure <json-path>` imports a configuration; `pair
<invitation-json-path>` pairs a stopped/local agent. Do not run two processes on
the same state directory. Agent `serve` retries application availability every
five seconds. Clipboard failure in a headless session does not stop networking.

## EasyTier verification

Checked official upstream on **2026-10-28**:

- [Repository and license](https://github.com/EasyTier/EasyTier): LGPL-3.0.
- [Latest published release observed](https://github.com/EasyTier/EasyTier/releases):
  v2.6.4, commit `8428a89`, released 2026-05-12.
- Official README documents `easytier-cli peer`, `route`, and `node`; upstream
  `easytier/src/core.rs` defines `RpcPortalOptions` and `--rpc-portal`. These are
  references for a future management adapter, **not invoked by MeshClip**.
- No SDK, RPC schema, broadcast discovery or undocumented configuration fields
  are assumed. App-level reachability is separate from EasyTier node status.
- No EasyTier binaries or source are redistributed here. Reassess LGPL duties and
  release-specific attribution before any future distribution.

## Known limits

This revision does not yet satisfy all requested MVP acceptance conditions:
Windows native build/runtime, real EasyTier two-device clipboard integration,
OS permissions, tray and autostart remain unverified. Same-name files are refused;
rename/overwrite UI is pending. Receiving requires state and destination on one
hard-link-capable volume. Short pairing codes and QR are not implemented. History
is intentionally memory-only. Sensitive filtering cannot identify all credentials.
Packaging progress and automatic archive cleanup are pending. Single-instance
locking and full power-loss recovery need work. Transport uses one TLS connection
per chunk and has no performance benchmark. No mobile, macOS or Linux desktop
support is claimed.

Architecture, wire format, threat model, decisions and limitations:
[docs/architecture.md](docs/architecture.md).
