<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { useAppStore } from "../stores/app";
import {
  downloadPost,
  formatBytes,
  resolveInput,
  onJobProgress,
  type JobProgress,
} from "../lib/ipc";

const app = useAppStore();
const input = ref("");
const busy = ref(false);
const error = ref<string | null>(null);
const profileNotice = ref<string | null>(null);

interface JobView {
  id: string;
  label: string;
  state: JobProgress["state"];
  currentFile: number;
  totalFiles: number;
  bytesDone: number;
  fileName: string;
  error?: string;
}

const jobs = reactive(new Map<string, JobView>());

let unlisten: (() => void) | null = null;

onMounted(async () => {
  unlisten = await onJobProgress((p) => {
    const existing = jobs.get(p.job_id);
    const job =
      existing ??
      reactive({
        id: p.job_id,
        label: p.label,
        state: "fetching" as JobProgress["state"],
        currentFile: 0,
        totalFiles: 0,
        bytesDone: 0,
        fileName: "",
        error: undefined,
      });
    if (!existing) jobs.set(p.job_id, job);
    job.state = p.state;
    if (p.state === "downloading") {
      job.currentFile = p.current_file ?? job.currentFile;
      job.totalFiles = p.total_files ?? job.totalFiles;
      job.bytesDone = Math.max(job.bytesDone, p.bytes_done ?? 0);
      job.fileName = p.file_name ?? job.fileName;
    }
    if (p.state === "done" || p.state === "failed") {
      if (p.error) job.error = p.error;
      app.refreshBalance().catch(() => {});
    }
  });
});

onBeforeUnmount(() => {
  unlisten?.();
});

async function submit() {
  const raw = input.value.trim();
  if (!raw || busy.value) return;
  busy.value = true;
  error.value = null;
  profileNotice.value = null;
  try {
    const target = await resolveInput(raw);
    if (target.kind === "post") {
      input.value = "";
      await downloadPost(target.code);
    } else {
      profileNotice.value = `@${target.username}: profile downloads arrive in the next build.`;
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="mx-auto max-w-2xl space-y-6 p-6">
    <form class="flex gap-2" @submit.prevent="submit">
      <input
        v-model="input"
        class="input"
        placeholder="@username or instagram.com/p/… link"
        spellcheck="false"
        autocomplete="off"
      />
      <button class="btn-primary shrink-0" type="submit" :disabled="busy || !input.trim()">
        {{ busy ? "…" : "Fetch" }}
      </button>
    </form>

    <p v-if="error" class="rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-sm text-err">{{ error }}</p>
    <p v-if="profileNotice" class="rounded-lg border border-warn/40 bg-warn/10 px-3 py-2 text-sm text-warn">
      {{ profileNotice }}
    </p>

    <div v-if="jobs.size > 0" class="space-y-3">
      <TransitionGroup name="job">
        <div v-for="[id, job] in jobs" :key="id" class="card p-4">
          <div class="flex items-center justify-between gap-3">
            <span class="truncate font-medium text-slate-200">{{ job.label }}</span>
            <span
              v-if="job.state === 'downloading' || job.state === 'fetching'"
              class="shrink-0 animate-pulse text-xs text-accent"
              >downloading…</span
            >
            <span v-else-if="job.state === 'done'" class="shrink-0 text-xs text-ok">✓ done</span>
            <span v-else class="shrink-0 text-xs text-err">failed</span>
          </div>
          <div v-if="job.state === 'downloading'" class="mt-2 space-y-1.5">
            <div class="h-1.5 overflow-hidden rounded-full bg-surface-3">
              <div class="h-full w-1/3 animate-[slide_1.2s_ease-in-out_infinite] rounded-full bg-gradient-to-r from-[var(--color-accent-2)] to-[var(--color-accent)]"></div>
            </div>
            <div class="flex justify-between text-xs tabular-nums text-slate-500">
              <span>{{ job.fileName }}</span>
              <span>file {{ job.currentFile }}/{{ job.totalFiles }} · {{ formatBytes(job.bytesDone) }}</span>
            </div>
          </div>
          <p v-if="job.error" class="mt-2 text-xs text-err">{{ job.error }}</p>
        </div>
      </TransitionGroup>
    </div>

    <div v-if="jobs.size === 0" class="card flex items-center justify-center p-12 text-sm text-slate-500">
      Paste a post or reel link to download it.
    </div>
  </div>
</template>

<style>
.job-enter-active,
.job-leave-active {
  transition: all 0.25s ease;
}
.job-enter-from,
.job-leave-to {
  opacity: 0;
  transform: translateY(6px);
}
@keyframes slide {
  0% {
    transform: translateX(-100%);
  }
  100% {
    transform: translateX(300%);
  }
}
</style>
