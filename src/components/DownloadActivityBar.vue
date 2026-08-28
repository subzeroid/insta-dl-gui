<script setup lang="ts">
import { computed } from "vue";

import { formatBytes } from "../lib/ipc";
import { useJobsStore } from "../stores/jobs";

const jobs = useJobsStore();
const activeJobs = computed(() =>
  [...jobs.jobs.values()].filter(
    (job) => job.state === "fetching" || job.state === "downloading",
  ),
);
const primary = computed(() => activeJobs.value[0] ?? null);
const activeCount = computed(() => activeJobs.value.length);
const progressText = computed(() => {
  const job = primary.value;
  if (!job) return "No active downloads";
  if (job.state === "fetching") return "Fetching…";
  const parts: string[] = [];
  if (job.currentFile > 0) {
    parts.push(job.totalFiles > 0 ? `file ${job.currentFile}/${job.totalFiles}` : `file ${job.currentFile}`);
  }
  if (job.bytesDone > 0) parts.push(formatBytes(job.bytesDone));
  return parts.join(" · ") || "Starting…";
});
</script>

<template>
  <RouterLink
    to="/queue"
    data-testid="download-activity"
    class="group relative flex min-h-11 shrink-0 items-center justify-between gap-4 overflow-hidden border-t border-line bg-surface-1 px-4 py-2 text-sm text-slate-400 hover:bg-surface-2 hover:text-slate-200"
  >
    <span class="flex min-w-0 items-center gap-3">
      <span class="shrink-0 font-medium text-slate-300 group-hover:text-slate-100">Downloads</span>
      <span v-if="primary" class="truncate text-xs text-slate-400">{{ primary.label }}</span>
    </span>
    <span class="flex shrink-0 items-center gap-3 text-xs tabular-nums text-slate-500">
      <span v-if="primary">{{ activeCount }} active</span>
      <span>{{ progressText }}</span>
    </span>
    <span
      v-if="primary"
      data-testid="download-progress"
      class="absolute inset-x-0 bottom-0 h-0.5 overflow-hidden bg-surface-3"
      aria-hidden="true"
    >
      <span
        class="block h-full w-1/3 animate-[slide_1.2s_ease-in-out_infinite] rounded-full bg-gradient-to-r from-[var(--color-accent-2)] to-[var(--color-accent)]"
      />
    </span>
  </RouterLink>
</template>
