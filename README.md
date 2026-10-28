# MeshClip

## 中文说明

MeshClip 是一个注重隐私的跨设备剪切板同步与轻量级文件中转工具，使用 Rust、Tauri 2、Vue 3 和 TypeScript 开发，采用 MIT 开源许可证。

**EasyTier 负责组网，MeshClip 负责设备认证、剪切板同步、文件传输和界面。** 桌面设备需先自行运行 EasyTier；MeshClip 不重复实现 VPN、NAT 穿透，也不分发 EasyTier 二进制文件。

### 当前功能与状态

- **可信设备配对**：独立设备身份、一次性邀请、TLS 证书固定、签名认证及信任撤销。
- **纯文本剪切板同步**：自动同步开关、按设备授权、内容哈希和事件去重、断线重试；单条文本上限 1 MiB。
- **隐私控制**：敏感关键词过滤、应用排除接口、默认关闭的内存历史；不主动记录正文到日志。过滤不能识别所有密码与令牌。
- **文件中转**：单文件、多文件及文件夹 TAR 打包发送；接收方确认、256 KiB 分块、真实进度、取消、重试、断点续传及 SHA-256 校验。接收后不会自动执行或解压。
- **桌面集成**：配置和可信设备持久化、系统托盘、后台运行及开机启动选项。
- **手机网页扩展**：扫码授权、手动交换文本和文件上传下载已在 [PR #1](https://github.com/LAkimoto/MeshClip/pull/1) 实现，尚待合并。手机可与 Windows 处于同一局域网，或自行加入同一 EasyTier 网络；网页文件上限 64 MiB/个，上传需桌面确认。不承诺手机后台自动同步。

> 当前为开发预览，尚未完成全部 Windows MVP 验收。基线 Windows 安装包构建和 EXE 启动冒烟检查已在 CI 通过；实体手机、真实双机 EasyTier、系统剪切板、托盘及开机启动等仍需实机验收。平台支持和测试边界以 [开发进度](docs/progress.md) 为准，不宣称已支持 Android、iOS、HarmonyOS 原生客户端或完整 macOS/Linux 桌面端。

### Windows 开发启动

适用于 Windows 10/11 x64。先安装 Rust stable（MSVC）、Node.js 22+、Visual Studio C++ 桌面开发工具、Windows SDK 和 WebView2，详见 [Tauri 官方前置要求](https://v2.tauri.app/start/prerequisites/)。

```powershell
.\scripts\dev-windows.ps1
```

也可以手动执行：

```powershell
cargo test -p meshclip-core --locked
cd apps/desktop
npm ci
npm run tauri dev
# 构建未签名的 Windows NSIS 安装包
npm run tauri build -- --bundles nsis
```

安装包输出到 `target/release/bundle/nsis/`。

### 桌面设备连接

1. 在两台设备上分别启动 EasyTier，确认虚拟 IP 互通。
2. 在 MeshClip 设置中填写本机 EasyTier 虚拟 IP 和端口，如 `10.144.144.1:47831`。保存后从托盘退出再重启；默认仅监听回环地址，禁止通配地址。按需配置防火墙。
3. 在一台设备生成一次性邀请，通过可信渠道将完整 JSON 发送给另一台设备，并在 120 秒内完成配对。请勿公开邀请。
4. 双方分别开启对端剪切板权限和自动同步开关。
5. 发送文件时先选择可信设备；接收方接受后，发送方点击重试开始传输或续传。关闭窗口会保留托盘后台运行，彻底退出请使用托盘菜单。

### 手机网页使用（PR #1）

1. 在 Windows 的“手机网页”页填写本机局域网 IP 或 EasyTier 虚拟 IP，以及独立网页端口（建议 `47832`），开启 HTTPS 网页共享。
2. 手机使用系统相机或扫码工具扫描二维码。邀请仅可使用一次、120 秒有效；会话最长一小时。首次访问需通过可信渠道核对桌面显示的自签名证书 SHA-256 指纹，浏览器兼容性仍需验证。
3. Windows 主动共享当前剪切板或选定文件；手机可手动粘贴发送文本、刷新复制共享文本、上传或下载文件。上传请求需在桌面“文件传输”页接受后再开始。
4. 同一未过期会话内可重新选择原文件继续上传。关闭网页共享会停止监听并撤销所有手机会话；重启后需重新扫码和提交上传请求。

浏览器剪切板操作受到 HTTPS、权限及用户点击限制，可使用长按粘贴/复制。锁屏或切换应用可能中断传输。

### 检查与已知限制

```sh
./scripts/check.sh
```

该脚本执行 Rust 格式检查、核心 Clippy、测试及前端生产构建。CI 另执行 Windows 桌面构建和启动检查。

同名文件默认拒绝覆盖；接收暂要求临时目录与目标目录处于支持硬链接的同一文件系统。历史仅保存在内存中；跨卷落盘、完善的重命名/覆盖交互、单实例锁及掉电恢复仍待完善。桌面对端协议目前每个分块建立一次 TLS 连接，尚无吞吐性能承诺。

详细资料：[架构与协议](docs/architecture.md) · [开发与验收记录](docs/progress.md) · [PR #1 手机网页文档](https://github.com/LAkimoto/MeshClip/blob/feature/mobile-web/docs/mobile-web.md)。

---

## English documentation

Private clipboard synchronization and lightweight peer-to-peer file relay over
an **existing EasyTier virtual network**. Rust + Tokio + rustls; Tauri 2 + Vue 3 +
TypeScript desktop UI. MIT licensed. No cloud file server, VPN implementation or
bundled EasyTier binaries.

> **Development preview, not an accepted Windows MVP.** Core network tests and
> frontend build run locally. The baseline Windows desktop built and smoke-started
> in CI run 40620113290; two real EasyTier-connected Windows devices remain untested.
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
an unsigned Windows desktop installer. Baseline CI run 40620113290 passed the
build and native EXE startup smoke.

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
Real EasyTier two-device clipboard integration,
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
