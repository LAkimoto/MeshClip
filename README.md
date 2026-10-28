# MeshClip

Private clipboard synchronization and lightweight peer-to-peer file relay over
an **existing EasyTier virtual network**. Rust + Tokio + rustls; Tauri 2 + Vue 3 +
TypeScript desktop UI. MIT licensed. No cloud file server, VPN implementation or
bundled EasyTier binaries.

> **Development preview, not an accepted Windows MVP.** Core network tests and
> frontend build run locally. The baseline Windows desktop was built and smoke-
> started in CI (run 40620113290); the mobile-web extension needs its own Windows
> CI validation. Two real EasyTier-connected Windows devices remain untested.
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
- Six UI pages connected to backend state, tray/background and autostart wiring.
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
an unsigned Windows desktop installer. Baseline CI run 40620113290 passed the
build and native EXE startup smoke; extension-specific results are recorded in
[progress](docs/progress.md).

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

## Mobile browser: QR, LAN or external EasyTier

Open **手机网页** in Windows MeshClip. Enter the Windows LAN IP (for example
`192.168.1.10:47832`) when the phone is on the same Wi-Fi/LAN, or its EasyTier
virtual IP when the phone independently runs EasyTier in the same virtual network.
MeshClip does not install, join or configure the phone's EasyTier client.

1. Enable HTTPS sharing, then use the phone camera/scanner to open the QR URL.
   A separate explicit-IP listener is opened only while this feature is enabled;
   permit its port through Windows Firewall if needed. No wildcard binding.
2. Verify the temporary self-signed certificate's SHA-256 fingerprint against the
   desktop display over a trusted channel. Browser/OS trust warnings are expected;
   do not continue if the certificate cannot be verified. Browser support after a
   certificate exception varies. A trusted HTTPS reverse proxy/certificate setup
   is not included in this preview.
3. Each QR grants access: keep it private. It expires in 120 seconds and can be
   exchanged once for a random, memory-only, one-hour browser session (maximum
   eight). Refresh QR does not revoke already paired phones. **关闭共享并撤销全部
   手机会话** stops the listener/connections and clears sessions and shared items.
4. Click **主动共享当前 Windows 剪切板** to expose one filtered snapshot. The page
   never reads desktop clipboard history or automatically polls the OS clipboard.
   On the phone, paste/type and Send to Windows, or refresh and Copy to phone.
   Receiving text enters the native clipboard write queue; desktop diagnostics
   report OS write failures. It is an explicit action, independent of auto-sync.
5. Choose files on Windows to make them downloadable on paired phones. Mobile
   uploads create **pending** receive tasks in the desktop Files page. Accept
   there, then tap **开始 / 继续** on the phone. Acknowledged 256 KiB offsets drive
   progress, speed and ETA. After a disconnect/refresh, select the same file and
   resume in the same unexpired session. Cancel/reject prevents further writes.

Mobile-web files are capped at **64 MiB each**: browser WebCrypto hashes a bounded
file buffer, and downloaded files are verified before saving. Desktop peer
transfers retain their existing configurable limit. Finalized uploads use the
same filename validation, no-overwrite, disk checks, staging and SHA-256 checks as
native transfers; staging/receive directories must share a filesystem. Download
links expose only individually selected files, never directory browsing. All
active browser sessions share the explicitly published board and download list;
only the owning browser session can inspect/write/cancel its upload tasks.

Browser clipboard/camera APIs require platform permissions, trusted secure
contexts and typically a user gesture. Manual long-press paste/copy remains the
fallback. **No mobile background clipboard synchronization is claimed.** Locking
the phone or switching apps can interrupt transfers. Browser sessions do not
survive restarting/shutting down the bridge; old partial tasks must be cancelled
on desktop and a new upload offered after reconnecting. No Android/iOS/HarmonyOS
native client is provided or claimed as tested. See [mobile protocol and
limitations](docs/mobile-web.md).

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
