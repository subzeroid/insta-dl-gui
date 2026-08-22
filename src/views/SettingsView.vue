<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../stores/app";

const app = useAppStore();
const sidecar = ref(app.sidecar);

onMounted(() => {
  sidecar.value = app.sidecar;
});

watch(sidecar, async (v) => {
  await app.saveSettings({ sidecar: v });
});

async function pickDir() {
  const dir = await open({ directory: true });
  if (typeof dir === "string") {
    await app.saveSettings({ dest_dir: dir });
  }
}
</script>

<template>
  <div class="mx-auto max-w-2xl space-y-6 p-6">
    <h2 class="text-lg font-semibold">Settings</h2>

    <div class="card space-y-1 p-5">
      <div class="text-sm font-medium text-slate-300">Download folder</div>
      <p class="text-xs text-slate-500">Files are saved as dest/&lt;username&gt;/posts|stories|highlights/…</p>
      <div class="mt-2 flex gap-2">
        <input class="input font-mono text-xs" :value="app.destDir" readonly />
        <button class="btn-secondary shrink-0" @click="pickDir">Browse…</button>
      </div>
    </div>

    <div class="card flex items-center justify-between p-5">
      <div>
        <div class="text-sm font-medium text-slate-300">Save JSON metadata</div>
        <p class="text-xs text-slate-500">Writes a &lt;file&gt;.json sidecar with caption, likes and owner next to every post.</p>
      </div>
      <label class="relative inline-flex cursor-pointer items-center">
        <input v-model="sidecar" type="checkbox" class="peer sr-only" />
        <div class="h-6 w-11 rounded-full bg-surface-3 after:absolute after:left-0.5 after:top-0.5 after:h-5 after:w-5 after:rounded-full after:bg-slate-400 after:transition-all peer-checked:bg-accent peer-checked:after:translate-x-5 peer-checked:after:bg-white"></div>
      </label>
    </div>
  </div>
</template>
