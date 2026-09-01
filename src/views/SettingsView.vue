<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../stores/app";
import { formatBalance } from "../lib/ipc";

const app = useAppStore();
const sidecar = ref(app.sidecar);
const sidecarSaving = ref(false);
const saveError = ref<string | null>(null);
const replacementToken = ref("");
const tokenBusy = ref(false);
const tokenError = ref<string | null>(null);
const tokenSuccess = ref<string | null>(null);
const replacementProxy = ref("");
const proxyInput = ref<HTMLInputElement | null>(null);
const proxyError = ref<string | null>(null);
const proxySuccess = ref<string | null>(null);
const PROXY_VALIDATION_ERROR = "Enter a valid HTTP, HTTPS, SOCKS5, or SOCKS5H proxy URL";
const PROXY_SAVE_ERROR = "Proxy settings could not be saved. The previous proxy is still active.";
let proxyViewMounted = true;

onMounted(() => {
  sidecar.value = app.sidecar;
});

onBeforeUnmount(() => {
  proxyViewMounted = false;
});

async function changeSidecar(event: Event) {
  const input = event.currentTarget as HTMLInputElement;
  if (sidecarSaving.value) {
    input.checked = sidecar.value;
    return;
  }
  const requested = input.checked;
  sidecar.value = requested;
  sidecarSaving.value = true;
  saveError.value = null;
  try {
    await app.saveSettings({ sidecar: requested });
  } catch {
    saveError.value = "Settings could not be saved. Your previous settings are still active.";
  } finally {
    sidecar.value = app.sidecar;
    sidecarSaving.value = false;
  }
}

async function pickDir() {
  const dir = await open({ directory: true });
  if (typeof dir === "string") {
    saveError.value = null;
    try {
      await app.saveSettings({ dest_dir: dir });
    } catch {
      saveError.value = "Settings could not be saved. Your previous settings are still active.";
    }
  }
}

async function replaceToken() {
  const token = replacementToken.value.trim();
  if (!token || tokenBusy.value) return;
  tokenBusy.value = true;
  tokenError.value = null;
  tokenSuccess.value = null;
  try {
    const balance = await app.replaceToken(token);
    replacementToken.value = "";
    tokenSuccess.value = `Token replaced · ${formatBalance(balance)}`;
  } catch (cause) {
    tokenError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    tokenBusy.value = false;
  }
}

async function applyProxy() {
  const proxyUrl = replacementProxy.value.trim();
  if (!proxyUrl || app.proxySaving) return;
  proxyError.value = null;
  proxySuccess.value = null;
  try {
    await app.setProxy(proxyUrl);
    replacementProxy.value = "";
    proxySuccess.value = "Proxy applied to HikerAPI and Instagram CDN";
  } catch (cause) {
    const message = cause instanceof Error ? cause.message : cause;
    proxyError.value = message === PROXY_VALIDATION_ERROR ? message : PROXY_SAVE_ERROR;
  }
}

async function clearProxy() {
  if (app.proxySaving) return;
  proxyError.value = null;
  proxySuccess.value = null;
  try {
    await app.setProxy(null);
    replacementProxy.value = "";
    proxySuccess.value = "Proxy cleared";
    if (proxyViewMounted) {
      await nextTick();
      proxyInput.value?.focus();
    }
  } catch {
    proxyError.value = PROXY_SAVE_ERROR;
  }
}
</script>

<template>
  <div class="mx-auto max-w-2xl space-y-6 p-6">
    <h2 class="text-lg font-semibold">Settings</h2>

    <form data-testid="token-form" class="card space-y-3 p-5" @submit.prevent="replaceToken">
      <div>
        <div class="text-sm font-medium text-slate-300">HikerAPI token</div>
        <p class="mt-1 text-xs text-slate-500">
          Current: <span data-testid="token-hint" class="font-mono text-slate-400">{{ app.tokenHint || "Not configured" }}</span>
        </p>
      </div>
      <div class="flex gap-2">
        <input
          v-model="replacementToken"
          name="hiker-token"
          class="input font-mono text-xs"
          type="text"
          placeholder="Paste a new token…"
          autocomplete="off"
          :disabled="tokenBusy"
        />
        <button
          data-testid="replace-token"
          class="btn-primary shrink-0"
          type="submit"
          :disabled="tokenBusy || !replacementToken.trim()"
        >
          {{ tokenBusy ? "Validating…" : "Replace token" }}
        </button>
      </div>
      <p v-if="tokenError" data-testid="token-error" class="text-xs text-err" role="alert">
        {{ tokenError }}
      </p>
      <p v-else-if="tokenSuccess" data-testid="token-success" class="text-xs text-ok" role="status">
        {{ tokenSuccess }}
      </p>
    </form>

    <form
      data-testid="proxy-form"
      class="card space-y-3 p-5"
      :aria-busy="app.proxySaving"
      @submit.prevent="applyProxy"
    >
      <div>
        <label for="network-proxy" class="text-sm font-medium text-slate-300">Network proxy</label>
        <p id="proxy-explanation" class="mt-1 text-xs text-slate-500">
          Routes both HikerAPI and Instagram CDN requests.
        </p>
        <p id="proxy-current" class="mt-1 text-xs text-slate-500">
          Current:
          <span data-testid="proxy-hint" class="font-mono text-slate-400">
            {{ app.proxyHint || "Direct connection" }}
          </span>
        </p>
      </div>
      <p id="proxy-support" class="text-xs text-slate-500">
        Supports HTTP, HTTPS, SOCKS5, SOCKS5H including credentials.
      </p>
      <div data-testid="proxy-controls" class="flex flex-col gap-2 sm:flex-row">
        <input
          v-model="replacementProxy"
          ref="proxyInput"
          id="network-proxy"
          name="network-proxy"
          class="input min-w-0 flex-1 font-mono text-xs"
          type="text"
          placeholder="http://proxy.example:8080"
          autocomplete="off"
          spellcheck="false"
          :readonly="app.proxySaving"
          :aria-disabled="app.proxySaving"
          aria-describedby="proxy-explanation proxy-current proxy-support"
        />
        <button
          data-testid="apply-proxy"
          class="btn-primary shrink-0"
          type="submit"
          :disabled="app.proxySaving || !replacementProxy.trim()"
        >
          {{ app.proxySaving ? "Saving…" : "Apply proxy" }}
        </button>
        <button
          v-if="app.hasProxy"
          data-testid="clear-proxy"
          class="btn-secondary shrink-0"
          type="button"
          :disabled="app.proxySaving"
          @click="clearProxy"
        >
          Clear proxy
        </button>
      </div>
      <p v-if="proxyError" data-testid="proxy-error" class="text-xs text-err" role="alert">
        {{ proxyError }}
      </p>
      <p v-else-if="proxySuccess" data-testid="proxy-success" class="text-xs text-ok" role="status">
        {{ proxySuccess }}
      </p>
    </form>

    <div class="card space-y-1 p-5">
      <div class="text-sm font-medium text-slate-300">Download folder</div>
      <p class="text-xs text-slate-500">Files are saved as dest/&lt;username&gt;/posts|stories|highlights/…</p>
      <div class="mt-2 flex gap-2">
        <input class="input font-mono text-xs" :value="app.destDir" readonly />
        <button class="btn-secondary shrink-0" @click="pickDir">Browse…</button>
      </div>
    </div>

    <p
      v-if="saveError"
      class="rounded-lg border border-err/40 bg-err/10 px-4 py-3 text-sm text-err"
      role="alert"
    >
      {{ saveError }}
    </p>

    <div
      v-if="app.catalogWarning"
      class="card border-warn/30 bg-warn/5 p-5 text-sm text-warn"
      role="alert"
    >
      <p>{{ app.catalogWarning }}</p>
      <RouterLink class="mt-2 inline-block font-medium underline" to="/library">
        Open Library to scan
      </RouterLink>
    </div>

    <div class="card flex items-center justify-between p-5">
      <div>
        <div class="text-sm font-medium text-slate-300">Save JSON metadata</div>
        <p class="text-xs text-slate-500">Writes a &lt;file&gt;.json sidecar with caption, likes and owner next to every post.</p>
      </div>
      <label class="relative inline-flex cursor-pointer items-center">
        <input
          type="checkbox"
          class="peer sr-only"
          :checked="sidecar"
          :aria-disabled="sidecarSaving"
          @change="changeSidecar"
        />
        <div class="h-6 w-11 rounded-full bg-surface-3 after:absolute after:left-0.5 after:top-0.5 after:h-5 after:w-5 after:rounded-full after:bg-slate-400 after:transition-all peer-checked:bg-accent peer-checked:after:translate-x-5 peer-checked:after:bg-white"></div>
      </label>
    </div>
  </div>
</template>
