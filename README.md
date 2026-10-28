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

## Windows 集成组网（开发中）

启动后先选择“本地局域网”或“集成 EasyTier 内核组网”，再点击“启动服务”。
局域网模式填写本机局域网 IP，无需 EasyTier。内核模式选择官方 EasyTier TOML
配置，并填写配置中固定的虚拟 IP 和 MeshClip 应用端口；内核仅通过官方
`--config-file` 接口启动。网络密钥保留在用户选择的配置文件中，不复制到
MeshClip 普通配置、不显示在命令行或日志；请妥善限制该文件访问权限。

构建 Windows 集成包前执行 `./scripts/prepare-easytier.ps1`，下载并校验官方
v2.6.4 Windows x86_64 包，打包内核、DLL、LGPL/GPL 文本、对应源码归档及署名。
开发模式也需要先执行该脚本。内核以可替换的独立进程运行，不链接到 MIT 核心。

MeshClip 默认不启动应用监听服务。内核创建虚拟网卡可能需要管理员权限和驱动；
不会自动提权或修改防火墙。启动失败会停止子进程并返回错误。停止服务也会关闭
手机网页共享；退出进程时内核采用 kill-on-drop 清理。UI 切换模式前须先停止服务。
进程运行仅说明内核存活，不表示 EasyTier 对端已互通。DHCP 地址自动发现和内核
异常后自动重启尚未实现。上述内容取代此前仅使用外部 EasyTier 客户端的限制。

验证边界：局域网服务生命周期有自动测试；Windows 集成内核打包、TUN 及实网
启动尚待执行验证，不能视为已经完成。
