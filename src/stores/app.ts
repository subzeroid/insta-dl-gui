import { defineStore } from "pinia";
import { ref } from "vue";
import * as ipc from "../lib/ipc";

export const useAppStore = defineStore("app", () => {
  const ready = ref(false);
  const hasToken = ref(false);
  const tokenHint = ref<string | null>(null);
  const destDir = ref("");
  const sidecar = ref(true);
  const balance = ref<ipc.Balance | null>(null);

  async function init() {
    const s = await ipc.configState();
    applyState(s);
    ready.value = true;
    if (s.has_token) {
      refreshBalance().catch(() => {});
    }
  }

  function applyState(s: ipc.ConfigState) {
    hasToken.value = s.has_token;
    tokenHint.value = s.token_hint;
    destDir.value = s.dest_dir;
    sidecar.value = s.sidecar;
  }

  async function saveSettings(opts: { dest_dir?: string; sidecar?: boolean }) {
    applyState(await ipc.saveSettings(opts));
  }

  async function refreshBalance() {
    balance.value = await ipc.getBalance();
  }

  function onTokenSet() {
    hasToken.value = true;
  }

  return { ready, hasToken, tokenHint, destDir, sidecar, balance, init, saveSettings, refreshBalance, onTokenSet };
});
