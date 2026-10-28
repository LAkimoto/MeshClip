<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { enable, disable, isEnabled } from "@tauri-apps/plugin-autostart";
interface Peer {
  id: string;
  name: string;
  address: string;
  clipboard: boolean;
}
interface Config {
  name: string;
  listen: string;
  receive_dir: string;
  enabled: boolean;
  max_text: number;
  max_file: number;
  history_limit: number;
  history_hours: number;
  blocked_terms: string[];
  ignored_apps: string[];
  peers: Record<string, Peer>;
}
interface Clip {
  id: string;
  text: string;
  timestamp: number;
  source: string;
}
interface Task {
  offer: { id: string; name: string; size: number };
  peer: string;
  direction: string;
  status: string;
  offset: number;
  error: string | null;
  bytes_per_second: number;
  eta_seconds: number | null;
}
interface Status {
  listening: boolean;
  error: string | null;
  clipboard_error: string | null;
  peers: Record<string, string>;
  history: Clip[];
  tasks: Record<string, Task>;
}
const tab = ref("概览"),
  tabs = ["概览", "设备", "文件传输", "剪切板历史", "手机网页", "设置"];
const cfg = ref<Config>(),
  state = ref<Status>(),
  error = ref(""),
  notice = ref(""),
  ticket = ref(""),
  incoming = ref(""),
  target = ref(""),
  query = ref(""),
  auto = ref(false),
  blocked = ref(""),
  apps = ref("");
interface MobileInvitation { url: string; qr_svg: string; certificate_sha256: string; expires: number }
const mobile = ref<MobileInvitation>();
const mobileAddress = ref("127.0.0.1:47832");
async function shareMobileFiles() {
  const paths = await open({ multiple: true });
  if (paths) for (const path of typeof paths === "string" ? [paths] : paths)
    await invoke("mobile_share", { path });
  notice.value = "已共享所选文件；手机刷新文件列表即可下载。";
}
const desktop = isTauri();
let timer: ReturnType<typeof setInterval> | undefined;
let unlisten: (() => void) | undefined;
const online = computed(
  () =>
    Object.values(state.value?.peers ?? {}).filter((s) => s === "online")
      .length,
);
const history = computed(
  () =>
    state.value?.history.filter((c) =>
      c.text.toLowerCase().includes(query.value.toLowerCase()),
    ) ?? [],
);
async function act(fn: () => Promise<unknown>) {
  error.value = "";
  notice.value = "";
  try {
    await fn();
    await refresh();
  } catch (e) {
    error.value = String(e);
  }
}
async function refresh() {
  if (desktop) state.value = await invoke<Status>("snapshot");
}
async function load() {
  cfg.value = await invoke<Config>("config");
  blocked.value = cfg.value.blocked_terms.join("\n");
  apps.value = cfg.value.ignored_apps.join("\n");
}
async function save() {
  if (cfg.value) {
    cfg.value.blocked_terms = blocked.value.split("\n").filter(Boolean);
    cfg.value.ignored_apps = apps.value.split("\n").filter(Boolean);
    await invoke("save_config", { config: cfg.value });
    notice.value = "已保存。监听地址修改后请从托盘退出并重新启动。";
  }
}
async function files(paths?: string[], folder = false) {
  if (!target.value) throw Error("请先选择可信目标设备");
  const chosen =
    paths ?? (await open({ multiple: !folder, directory: folder }));
  if (chosen) {
    for (const path of typeof chosen === "string" ? [chosen] : chosen)
      await invoke("send_file", { peer: target.value, path });
    tab.value = "文件传输";
  }
}
const size = (n: number) =>
  n > 1048576
    ? `${(n / 1048576).toFixed(1)} MB`
    : n > 1024
      ? `${(n / 1024).toFixed(1)} KB`
      : `${n} B`;
onMounted(async () => {
  if (!desktop) return;
  await act(async () => {
    await load();
    auto.value = await isEnabled();
    await refresh();
    unlisten = await getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "drop") {
        const paths = e.payload.paths;
        void act(() => files(paths));
      }
    });
    timer = setInterval(
      () => void refresh().catch((e) => (error.value = String(e))),
      1000,
    );
  });
});
onUnmounted(() => {
  if (timer) clearInterval(timer);
  unlisten?.();
});
</script>
<template>
  <div class="shell">
    <aside>
      <div class="brand"><span class="mark">M</span> MeshClip</div>
      <div class="tagline">你自己的设备，你自己的网络。</div>
      <nav>
        <button
          v-for="(item, i) in tabs"
          :key="item"
          :class="{ active: tab === item }"
          @click="tab = item"
        >
          <span>{{ ["◈", "⊞", "↗", "≡", "▣", "⚙"][i] }}</span
          >{{ item }}
        </button>
      </nav>
      <div class="sidebar-bottom">
        <i :class="{ live: state?.listening }"></i
        >{{ state?.listening ? "应用服务监听中" : "服务未启动"
        }}<small>EasyTier · 外部客户端模式</small>
      </div>
    </aside>
    <main>
      <header>
        <div>
          <div class="eyebrow">PRIVATE DEVICE MESH / 01</div>
          <h1>{{ tab }}</h1>
        </div>
        <button
          :disabled="!cfg"
          @click="
            act(async () => {
              cfg!.enabled = !cfg!.enabled;
              await save();
            })
          "
          :class="{ enabled: cfg?.enabled }"
        >
          {{ cfg?.enabled ? "● 自动同步已开启" : "Ⅱ 自动同步已暂停" }}
        </button>
      </header>
      <div v-if="!desktop" class="banner">
        浏览器仅预览界面。Rust
        后端未连接，所有设备、同步和传输状态均为空。请使用 Tauri 桌面应用。
      </div>
      <div v-if="error || state?.error" class="banner danger">
        {{ error || state?.error }}
      </div>
      <div v-if="notice" class="banner">{{ notice }}</div>
      <div v-if="state?.clipboard_error" class="banner danger">
        剪切板：{{ state.clipboard_error }}
      </div>
      <template v-if="tab === '概览'"
        ><div class="hero">
          <div>
            <span class="pill">EASYTIER VIRTUAL NETWORK</span>
            <h2>复制一下，<br />连接你的每一台设备。</h2>
            <p>加密直连 · 显式授权 · 无中心文件服务器</p>
            <button @click="tab = '设备'">连接新设备 ↗</button>
          </div>
          <div class="mesh">
            <b>M</b><span>⌘</span><span>▣</span><span>◈</span
            ><small>TRUSTED CONNECTIONS</small>
          </div>
        </div>
        <div class="stats">
          <article>
            <label>本机设备</label><strong>{{ cfg?.name ?? "等待后端" }}</strong
            ><small>{{ cfg?.listen ?? "—" }}</small>
          </article>
          <article>
            <label>MeshClip 在线设备</label
            ><strong
              >{{ online }}
              <em>/ {{ Object.keys(cfg?.peers ?? {}).length }}</em></strong
            ><small>应用层认证探测</small>
          </article>
          <article>
            <label>进行中的任务</label
            ><strong>{{
              Object.values(state?.tasks ?? {}).filter((t) =>
                ["sending", "receiving", "connecting"].includes(t.status),
              ).length
            }}</strong
            ><small>分块传输 / SHA-256 校验</small>
          </article>
        </div>
        <section>
          <h3>连接状态</h3>
          <p>
            EasyTier 节点状态：未接入管理接口。请先在 EasyTier
            客户端确认组网，再通过设备页验证 MeshClip 服务。
          </p>
          <p>默认暂停自动同步。首次配对后需逐台开启剪切板权限。</p>
        </section></template
      >
      <template v-if="tab === '设备'"
        ><section>
          <h3>添加可信设备</h3>
          <p>
            在对端生成 120 秒一次性邀请，通过可信渠道传递。邀请包含虚拟
            IP、证书及一次性密钥，请勿公开。
          </p>
          <button
            :disabled="!desktop"
            @click="
              act(
                async () =>
                  (ticket = JSON.stringify(await invoke('invitation'))),
              )
            "
          >
            生成本机邀请</button
          ><textarea
            v-if="ticket"
            readonly
            :value="ticket"
            aria-label="本机配对邀请"
          ></textarea
          ><textarea
            v-model="incoming"
            placeholder="粘贴对端的完整 JSON 配对邀请"
            aria-label="对端配对邀请"
          ></textarea
          ><button
            :disabled="!desktop"
            @click="
              act(async () => {
                await invoke('pair', { ticket: JSON.parse(incoming) });
                incoming = '';
                await load();
              })
            "
          >
            验证身份并配对
          </button>
        </section>
        <section v-for="p in cfg?.peers" :key="p.id" class="peer">
          <div class="device-icon">▣</div>
          <div>
            <h3>{{ p.name }}</h3>
            <p>{{ p.address }} · {{ state?.peers[p.id] ?? "等待探测" }}</p>
            <small>{{ p.id.slice(0, 24) }}…</small>
          </div>
          <label
            ><input
              type="checkbox"
              v-model="p.clipboard"
              @change="act(save)"
            />剪切板授权</label
          ><button
            @click="
              act(async () => {
                await invoke('revoke', { id: p.id });
                await load();
              })
            "
          >
            撤销信任
          </button>
        </section>
        <div v-if="!Object.keys(cfg?.peers ?? {}).length" class="empty">
          尚无可信设备。<small
            >请先在设置中配置本机 EasyTier 虚拟 IP 和端口。</small
          >
        </div></template
      >
      <template v-if="tab === '文件传输'"
        ><section>
          <h3>直接发送文件</h3>
          <select v-model="target" aria-label="目标设备">
            <option value="">选择可信目标设备</option>
            <option v-for="p in cfg?.peers" :key="p.id" :value="p.id">
              {{ p.name }} · {{ p.address }}
            </option></select
          ><button
            :disabled="!desktop"
            @click="act(() => files(undefined, true))"
          >
            选择文件夹（TAR）
          </button>
          <div class="drop" @click="act(() => files())">
            <span>↗</span>
            <h3>选择文件，或拖放到窗口</h3>
            <p>支持多选；对端确认后，在任务中点击重试开始传输。</p>
          </div>
          <p>
            文件夹显式打包为 TAR
            发送，接收后不会自动解压。同名文件默认拒绝覆盖，请先移开同名文件再重试。
          </p>
        </section>
        <section v-for="t in state?.tasks" :key="t.offer.id">
          <div class="row">
            <h3>{{ t.offer.name }}</h3>
            <span
              >{{ t.direction === "send" ? "发送" : "接收" }} ·
              {{ t.status }}</span
            >
          </div>
          <progress :value="t.offset" :max="t.offer.size || 1"></progress>
          <p>
            {{ size(t.offset) }} / {{ size(t.offer.size) }} ·
            {{ size(t.bytes_per_second) }}/s · {{ t.eta_seconds ?? "—" }} 秒
          </p>
          <p v-if="t.error" class="error">{{ t.error }}</p>
          <div v-if="t.status === 'pending'">
            <button
              @click="
                act(() => invoke('decide', { id: t.offer.id, accept: true }))
              "
            >
              接受</button
            ><button
              @click="
                act(() => invoke('decide', { id: t.offer.id, accept: false }))
              "
            >
              拒绝
            </button>
          </div>
          <button
            v-if="
              t.direction === 'send' &&
              ['interrupted', 'cancelled'].includes(t.status)
            "
            @click="act(() => invoke('retry', { id: t.offer.id }))"
          >
            重试 / 续传</button
          ><button
            v-if="!['complete', 'rejected', 'cancelled'].includes(t.status)"
            @click="act(() => invoke('cancel', { id: t.offer.id }))"
          >
            取消
          </button>
        </section>
        <div v-if="!Object.keys(state?.tasks ?? {}).length" class="empty">
          暂无传输任务<small>文件内容直接发送到目标设备。</small>
        </div></template
      >
      <template v-if="tab === '剪切板历史'"
        ><div class="row">
          <input
            v-model="query"
            placeholder="搜索本次运行的历史"
            aria-label="搜索历史"
          /><button
            :disabled="!desktop"
            @click="act(() => invoke('clear_history'))"
          >
            清空历史
          </button>
        </div>
        <section v-for="c in history" :key="c.id">
          <pre>{{ c.text }}</pre>
          <div class="row">
            <small
              >{{ new Date(c.timestamp).toLocaleString() }} ·
              {{ c.source.slice(0, 12) }}</small
            ><button @click="act(() => invoke('copy_text', { text: c.text }))">
              重新复制
            </button>
          </div>
        </section>
        <div v-if="!history.length" class="empty">
          暂无历史记录<small>默认不保留正文，可在设置中开启内存历史。</small>
        </div></template
      >
      <template v-if="tab === '手机网页'">
        <section>
          <h3>手机扫码 · 网页剪切板与文件中转</h3>
          <p>手机连接相同 Wi-Fi / 局域网，或自行加入与 Windows 相同的 EasyTier 网络。填写本机对应网卡的 IP 和独立网页端口；无需 MeshClip 手机客户端。</p>
          <label>本机局域网 / EasyTier IP:端口<input v-model="mobileAddress" :disabled="!!mobile" placeholder="例如 192.168.1.10:47832" /></label>
          <p>默认仅监听回环地址。请改为本机真实网卡 IP，并按需允许 Windows 防火墙入站；不支持 0.0.0.0。切换局域网 / EasyTier 地址前请关闭再开启共享。</p>
          <button v-if="!mobile" :disabled="!desktop" @click="act(async () => { mobile = await invoke<MobileInvitation>('start_mobile', { address: mobileAddress }); })">开启 HTTPS 网页并生成二维码</button>
          <template v-else>
            <button @click="act(async () => { mobile = await invoke<MobileInvitation>('mobile_invitation'); })">刷新一次性二维码</button>
            <button @click="act(async () => { await invoke('stop_mobile'); mobile = undefined; })">关闭共享并撤销全部手机会话</button>
            <div class="mobile-qr" v-html="mobile.qr_svg"></div>
            <p>手机使用系统相机扫码。邀请 120 秒有效且仅可兑换一次；已连接会话最长 1 小时。二维码即访问授权，请勿公开。</p>
            <textarea readonly :value="mobile.url" aria-label="手机访问链接"></textarea>
            <p>临时 HTTPS 证书 SHA-256（首次访问请核对，重开共享会变化）：</p><code class="fingerprint">{{ mobile.certificate_sha256 }}</code>
            <p>证书为自签名，浏览器可能提示不受信任。须通过可信渠道核对指纹；系统限制无法继续时需部署可信 HTTPS 证书，暂不宣称兼容全部手机浏览器。</p>
            <button @click="act(async () => { await invoke('mobile_share', { path: null }); notice = '当前剪切板已明确共享给所有已授权网页会话。'; })">主动共享当前 Windows 剪切板</button>
            <button @click="act(shareMobileFiles)">选择文件共享给手机（每个 ≤ 64 MiB）</button>
          </template>
        </section>
        <section><h3>权限与使用方式</h3><p>网页只显示你主动共享的文本与文件，不读取剪切板历史。手机发送文本会进入 Windows 剪切板写入队列；手机上传文件需在“文件传输”页确认，随后在手机点击“开始 / 继续”。</p><p>手机浏览器通常要求点击才能访问剪切板，支持手动粘贴 / 复制；锁屏与后台连续同步尚不支持。文件支持分块进度、取消、同会话续传和 SHA-256 校验。会话失效或重启后须重新扫码、重新提交文件请求。</p></section>
      </template>
      <template v-if="tab === '设置' && cfg"
        ><section>
          <h3>本机与网络</h3>
          <label>设备名称<input v-model="cfg.name" /></label
          ><label
            >本机 EasyTier 虚拟 IP:端口<input v-model="cfg.listen"
          /></label>
          <p>
            当前服务启动时绑定此地址。更改后需从托盘退出并重启；默认只监听
            127.0.0.1。
          </p>
          <label>接收目录<input v-model="cfg.receive_dir" /></label>
        </section>
        <section>
          <h3>隐私与剪切板</h3>
          <label
            ><input
              type="checkbox"
              v-model="cfg.enabled"
            />自动同步纯文本</label
          >
          <div class="row">
            <label
              >历史条数（0 = 关闭）<input
                type="number"
                min="0"
                max="1000"
                v-model.number="cfg.history_limit" /></label
            ><label
              >历史保留小时<input
                type="number"
                min="0"
                v-model.number="cfg.history_hours"
            /></label>
          </div>
          <label
            >敏感关键词，每行一条<textarea v-model="blocked"></textarea></label
          ><label>排除应用，每行一条<textarea v-model="apps"></textarea></label>
          <p>
            Windows 按前台进程名过滤（如
            1password.exe），尚未实机验证；无法识别应用时自动采集暂停。关键词过滤不能识别所有密码和令牌。历史仅保存在内存中。
          </p>
        </section>
        <section>
          <h3>后台运行</h3>
          <label
            ><input
              type="checkbox"
              v-model="auto"
              @change="
                act(async () => {
                  if (auto) await enable();
                  else await disable();
                })
              "
            />登录后自动启动</label
          >
          <p>关闭窗口后继续在系统托盘运行，使用托盘菜单退出。</p>
        </section>
        <button class="primary" @click="act(save)">保存设置</button></template
      >
      <footer>
        <span>MeshClip 0.1.0 · Development preview</span
        ><span>端到端 TLS / 显式设备信任</span>
      </footer>
    </main>
  </div>
</template>
