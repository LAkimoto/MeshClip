# Development and acceptance record

Date: 2026-10-28 (Asia/Shanghai). Status: **development preview; Windows MVP not accepted**.

## Phase progress

| Phase | Actual result |
|---|---|
| Workspace/environment | Empty initial workspace; Node 24.22.1/npm 11.9.0. Installed Rust stable 1.99.1 with rustfmt/clippy. No existing project overwritten. |
| EasyTier verification | Official repository, README, license, current release list and RPC option source inspected. Latest observed v2.6.4 (2026-05-12), LGPL-3.0. External-client TCP integration selected. |
| Architecture/protocol | Documented module boundaries, trust bootstrap, signed TLS request framing, limits, data flow, recovery, threats and unresolved decisions in architecture.md. |
| Native/UI skeleton | Cargo workspace, Tauri 2 shell/commands, Vue 3/TypeScript/Vite UI, config/trust persistence, tray/autostart wiring. Native desktop runtime unverified. |
| Network and identity | rustls server certificate pinning, Ed25519 challenge authentication, one-time invitation pairing, revocation, unknown-device refusal, protocol/version/message/rate limits. Tested on loopback TCP, not EasyTier. |
| Text synchronization | Text polling interface, filters, pause/per-device authorization, hash/event deduplication, deterministic conflict ordering, latest-event reconnect retry, memory history. Protocol/event behavior tested; OS clipboard integration unverified. |
| File transfer | Real chunked binary transport, receiver approval, progress/speed/ETA, cancellation, manual retry, restart resume, SHA-256, disk checks, explicit folder TAR packaging. Same-volume finalization only. |
| Checks | Format, core Clippy, core unit/integration tests and Vue production build executed. Native Linux and Windows cross-checks blocked by toolchain/system dependencies. |
| GitHub | Public repository created at https://github.com/LAkimoto/MeshClip. Source uploaded to main; remote core checks pass on Ubuntu and Windows after the credential-size fix. Native desktop build/start validation is in progress. |

## Executed validation

- `cargo fmt --all --check`: passed (workspace Rust sources).
- `cargo test -p meshclip-core --locked`: **18 tests passed locally on Linux** (11 core, 7 real-TCP/TLS integration; one symlink test is Unix-only).
- `cargo clippy -p meshclip-core --all-targets --locked -- -D warnings`: passed.
- `npm run build`: TypeScript check and Vite production build passed; frontend JS about 100 kB before gzip.
- Process startup smoke: native diagnostic agent started, accepted TLS and returned a 32-byte challenge; Vite returned HTTP 200 with MeshClip title. Both temporary processes were terminated after the check. See startup-smoke.json.
- `cargo check -p meshclip-desktop` on Linux: failed before application type checking because pkg-config/GLib/GTK/WebKit development libraries are absent. Attempted package installation did not complete because of container permission/source restrictions; stalled package command stopped.
- `cargo check -p meshclip-desktop --target x86_64-pc-windows-msvc`: target standard library installed, but failed in ring's C build because MSVC `lib.exe` is unavailable. This is **not a successful Windows compile**.
- Remote CI run 40619582267 exposed an oversized UTF-16 credential payload and a desktop Clippy lint. Both are fixed. Run 40619842748 confirms Ubuntu and Windows core checks/tests pass. Run 40620113290 passed all three jobs, including the Windows desktop build,
  NSIS installer upload and native EXE startup smoke. This is baseline evidence,
  not validation of the subsequently added mobile-web extension.

Tested cases include: text duplicate event/hash/echo suppression, stale/future events,
sensitive substring and size limits, pause, simultaneous copies, bidirectional
protocol exchange, disconnect/reconnect retry; trusted pairing, reused token,
unknown identity, wrong certificate, protocol version, oversized frame; empty and
multi-chunk files (~3 MiB), duplicate/invalid offsets, hash error, unsafe names,
no-overwrite collision, cancellation, receiver approval across restart, resume
from durable offsets, sender retry/progress, and folder archive/symlink handling.
Multi-gigabyte transfer, prolonged soak, saturation performance and power-loss
fault injection are **not tested**. Unit tests use memory events/files; no OS
clipboard success is inferred from them.

## Acceptance checklist supplied in project brief

| Criterion | Status / evidence |
|---|---|
| 1. Windows desktop builds and starts | Baseline passed CI run 40620113290: Windows desktop Clippy, NSIS build and native EXE startup smoke. Mobile extension requires fresh validation. |
| 2. Two EasyTier virtual IP endpoints connect | **Unverified**: no two EasyTier nodes available. Loopback TCP/TLS passes. |
| 3. Paired Windows devices auto-sync both ways | **Partially implemented/unverified**: authenticated event exchange passes; real OS clipboards not tested. |
| 4. Same content never loops | Event/hash suppression and remote-write owner logic implemented; protocol regression tests pass. OS loop test pending. |
| 5. Unknown peers cannot read clipboard/write receive dir | No clipboard-read API; unknown identity/unauthorized chunk tests pass. No external security audit. |
| 6. Files transfer with true progress | Loopback integration passes; UI wired to real acknowledged byte counts. Desktop visual/runtime verification pending. |
| 7. Interrupted transfer is recognized and resumable | Passed restart/resume and retry integration tests. Ordinary restart only. |
| 8. SHA-256 verifies finished file | Passed complete/corrupt transfer tests. |
| 9. Config/trust persists across restart | Passed Linux and Windows runner tests, including Windows Credential Manager identity recovery. Desktop session behavior remains unverified. |
| 10. Test/build results truthfully reported | Core/frontend checks pass; desktop checks blocked as recorded above. |

## Fixes made while validating

- Frontend drag/drop union narrowing corrected after TypeScript build failure.
- Pending file approval remains pending after restart (never implicitly authorized).
- Transfer status persistence excludes clipboard history/body entirely.
- Repeated Finish on an already completed task is idempotent.
- Clipboard changes advance the observed clock to avoid same-millisecond UUID
  ordering dropping a causally later local copy; concurrent remote events still
  follow documented total ordering.
- Connection memory/rate bounds, disk checks, sender concurrency, and cancellation
  worker guards added. Cancelled status cannot be overwritten by a late chunk ack.
- Pair-invitation generation avoids retaining config lock while acquiring pairing lock.

- Windows credential payload changed to compact hex JSON bytes using keyring secret APIs;
  round-trip/size regression test added after the first Windows CI failure.

## Next concrete work

1. Upload prepared source to public `LAkimoto/MeshClip`, run Windows CI and resolve
   any native compilation/packaging errors.
2. On a Windows host with MSVC/WebView2, launch UI and verify keyring, foreground
   exclusion, clipboard polling, tray, autostart, restart and shutdown behavior.
3. Connect two Windows devices through EasyTier, configure virtual binds/firewalls,
   exercise bilateral permissions, disconnect/reconnect, folder/multi-file send and
   several large files while checking final hashes and resource usage.
4. Implement collision rename/overwrite decisions, cross-volume finalization,
   single-instance lock, short-code/QR pairing, packaging progress/archive cleanup,
   configurable transfer retention and power-loss consistency.
5. Benchmark persistent sessions before throughput claims. Only then mark MVP
   accepted and consider EasyTier management integration or other platforms.

## Mobile-web extension (2026-10-28)

Implemented a sixth desktop page to explicitly start/stop an independent HTTPS
listener on a chosen LAN/EasyTier IP, generate actual one-use QR codes, show the
temporary certificate fingerprint, explicitly share desktop text/selected files,
and revoke all browser sessions. The mobile page supports text paste/send/copy,
file offers, desktop acceptance, 256 KiB chunk progress/speed/ETA, same-session
resume/cancel, verified downloads and a 64 MiB browser-file limit. Unknown
sessions and other sessions' task operations are refused. No native mobile or
background clipboard support is claimed. See mobile-web.md for protocol details.

Local checks: 22 Rust tests (4 new real-HTTPS bridge tests, 18 existing), core
Clippy, workspace format and Vue TypeScript/production build passed. Windows CI
for this extension and physical phone/LAN/EasyTier acceptance are pending.

The mobile extension is prepared on feature/mobile-web for pull-request review
and extension-specific Windows CI. Browser visual smoke was blocked: Playwright's Chromium
download returned invalid/truncated archives. The embedded mobile JavaScript
syntax was checked; no rendered-browser or physical-phone success is inferred.
