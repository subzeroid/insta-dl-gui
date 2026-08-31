import { defineStore } from "pinia";
import { ref } from "vue";
import * as ipc from "../lib/ipc";

export const useAppStore = defineStore("app", () => {
  const ready = ref(false);
  const hasToken = ref(false);
  const tokenHint = ref<string | null>(null);
  const hasProxy = ref(false);
  const proxyHint = ref<string | null>(null);
  const proxySaving = ref(false);
  const destDir = ref("");
  const sidecar = ref(true);
  const catalogWarning = ref<string | null>(null);
  const balance = ref<ipc.Balance | null>(null);

  async function init() {
    const s = await ipc.configState();
    applyInitialState(s);
    ready.value = true;
    if (s.has_token) {
      refreshBalance().catch(() => {});
    }
  }

  function applyTokenState(s: ipc.ConfigState) {
    hasToken.value = s.has_token;
    tokenHint.value = s.token_hint;
  }

  function applyProxyState(s: ipc.ConfigState) {
    hasProxy.value = s.has_proxy ?? false;
    proxyHint.value = s.proxy_hint ?? null;
  }

  function applyInitialState(s: ipc.ConfigState) {
    applyTokenState(s);
    applyProxyState(s);
    destDir.value = s.dest_dir;
    sidecar.value = s.sidecar;
    catalogWarning.value = s.catalog_warning ?? null;
  }

  async function saveSettings(opts: { dest_dir?: string; sidecar?: boolean }) {
    const s = await ipc.saveSettings(opts);
    if (opts.dest_dir !== undefined) {
      destDir.value = s.dest_dir;
      catalogWarning.value = s.catalog_warning ?? null;
    }
    if (opts.sidecar !== undefined) sidecar.value = s.sidecar;
  }

  async function setProxy(proxyUrl: string | null) {
    if (proxySaving.value) return;
    proxySaving.value = true;
    try {
      applyProxyState(await ipc.setProxy(proxyUrl));
    } finally {
      proxySaving.value = false;
    }
  }

  async function refreshBalance() {
    balance.value = await ipc.getBalance();
  }

  async function replaceToken(token: string) {
    const nextBalance = await ipc.validateToken(token.trim());
    applyTokenState(await ipc.configState());
    balance.value = nextBalance;
    return nextBalance;
  }

  function onTokenSet() {
    hasToken.value = true;
  }

  return {
    ready,
    hasToken,
    tokenHint,
    hasProxy,
    proxyHint,
    proxySaving,
    destDir,
    sidecar,
    catalogWarning,
    balance,
    init,
    saveSettings,
    setProxy,
    refreshBalance,
    replaceToken,
    onTokenSet,
  };
});
