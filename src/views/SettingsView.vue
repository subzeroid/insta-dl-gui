<script setup lang="ts">
import { onMounted, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../stores/app";

const app = useAppStore();
const sidecar = ref(app.sidecar);
const sidecarSaving = ref(false);
const saveError = ref<string | null>(null);

onMounted(() => {
  sidecar.value = app.sidecar;
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
          :disabled="sidecarSaving"
          @change="changeSidecar"
        />
        <div class="h-6 w-11 rounded-full bg-surface-3 after:absolute after:left-0.5 after:top-0.5 after:h-5 after:w-5 after:rounded-full after:bg-slate-400 after:transition-all peer-checked:bg-accent peer-checked:after:translate-x-5 peer-checked:after:bg-white"></div>
      </label>
    </div>
  </div>
</template>
