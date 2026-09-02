<script setup lang="ts">
import { onBeforeUnmount, ref } from "vue";
import { useRouter } from "vue-router";
import { useAppStore } from "../stores/app";
import { formatBalance } from "../lib/ipc";
import { openUrl } from "@tauri-apps/plugin-opener";
import AppBrand from "../components/AppBrand.vue";
import TokenInput from "../components/TokenInput.vue";

const app = useAppStore();
const router = useRouter();

const token = ref("");
const busy = ref(false);
const redirectPending = ref(false);
const error = ref<string | null>(null);
const okBalance = ref<string | null>(null);
let redirectTimer: ReturnType<typeof setTimeout> | undefined;
let alive = true;

onBeforeUnmount(() => {
  alive = false;
  if (redirectTimer !== undefined) clearTimeout(redirectTimer);
});

async function submit() {
  const t = token.value.trim();
  if (!t || busy.value || redirectPending.value) return;
  busy.value = true;
  error.value = null;
  try {
    const balance = await app.replaceToken(t);
    if (!alive) return;
    okBalance.value = formatBalance(balance);
    redirectPending.value = true;
    redirectTimer = setTimeout(() => {
      redirectTimer = undefined;
      router.replace("/explore");
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
      <h1 class="flex justify-center"><AppBrand :version="app.appVersion" size="large" align="center" /></h1>
      <p class="mt-2 text-center text-sm text-slate-400">
        Download Instagram posts, reels and stories — no Instagram login required.
      </p>

      <form class="mt-8 space-y-4" @submit.prevent="submit">
        <label class="block text-sm font-medium text-slate-300" for="token">HikerAPI token</label>
        <TokenInput id="token" v-model="token" class="font-mono" placeholder="Paste your token…" autocomplete="off" :disabled="busy || redirectPending" />
        <p class="text-xs text-slate-500">
          Get a free token at
          <a href="#" class="text-accent hover:underline" @click.prevent="openUrl('https://hikerapi.com/p/uk064a1b')"
            >hikerapi.com</a
          >
          — first 100 requests are free.
        </p>
        <button class="btn-primary w-full" type="submit" :disabled="busy || redirectPending || !token.trim()">
          {{ okBalance ? "✓ " + okBalance : busy ? "Validating…" : "Connect" }}
        </button>
        <p v-if="error" class="rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-xs text-err">{{ error }}</p>
      </form>
    </div>
  </div>
</template>
