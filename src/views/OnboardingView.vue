<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import { useAppStore } from "../stores/app";
import { formatBalance, validateToken } from "../lib/ipc";
import { openUrl } from "@tauri-apps/plugin-opener";

const app = useAppStore();
const router = useRouter();

const token = ref("");
const busy = ref(false);
const error = ref<string | null>(null);
const okBalance = ref<string | null>(null);

async function submit() {
  const t = token.value.trim();
  if (!t) return;
  busy.value = true;
  error.value = null;
  try {
    const balance = await validateToken(t);
    okBalance.value = formatBalance(balance);
    setTimeout(async () => {
      await app.refreshBalance().catch(() => {});
      app.onTokenSet();
      router.push("/download");
    }, 700);
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="flex h-full items-center justify-center p-6">
    <div class="card w-full max-w-md p-8">
      <h1 class="text-center text-2xl font-bold">
        <span class="bg-gradient-to-r from-[var(--color-accent-2)] to-[var(--color-accent)] bg-clip-text text-transparent"
          >insta-dl-gui</span
        >
      </h1>
      <p class="mt-2 text-center text-sm text-slate-400">
        Download Instagram posts, reels and stories — no Instagram login required.
      </p>

      <form class="mt-8 space-y-4" @submit.prevent="submit">
        <label class="block text-sm font-medium text-slate-300" for="token">HikerAPI token</label>
        <input id="token" v-model="token" class="input font-mono" type="password" placeholder="Paste your token…" autocomplete="off" />
        <p class="text-xs text-slate-500">
          Get a free token at
          <a href="#" class="text-accent hover:underline" @click.prevent="openUrl('https://hikerapi.com/p/uk064a1b')"
            >hikerapi.com</a
          >
          — first 100 requests are free.
        </p>
        <button class="btn-primary w-full" type="submit" :disabled="busy || !token.trim()">
          {{ okBalance ? "✓ " + okBalance : busy ? "Validating…" : "Connect" }}
        </button>
        <p v-if="error" class="rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-xs text-err">{{ error }}</p>
      </form>
    </div>
  </div>
</template>
