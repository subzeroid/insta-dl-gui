<script setup lang="ts">
import { computed } from "vue";
import { useJobsStore, type JobView } from "../stores/jobs";
import { formatBytes } from "../lib/ipc";

const props = defineProps<{ job: JobView }>();
const jobs = useJobsStore();

const active = computed(() => props.job.state === "downloading" || props.job.state === "fetching");
const statusText = computed(() => {
  switch (props.job.state) {
    case "downloading":
      return "downloading…";
    case "fetching":
      return "fetching…";
    case "done":
      return `✓ ${props.job.resultCount ?? 0} file${(props.job.resultCount ?? 0) === 1 ? "" : "s"}`;
    case "cancelled":
      return "cancelled";
    case "failed":
      return "failed";
  }
});
</script>

<template>
  <div class="card p-4">
    <div class="flex items-center justify-between gap-3">
      <span class="truncate font-medium text-slate-200">{{ job.label }}</span>
      <span class="flex shrink-0 items-center gap-2">
        <span
          v-if="active"
          class="animate-pulse text-xs text-accent"
          >{{ statusText }}</span
        >
        <span v-else-if="job.state === 'done'" class="text-xs text-ok">{{ statusText }}</span>
        <span v-else-if="job.state === 'cancelled'" class="text-xs text-warn">{{ statusText }}</span>
        <span v-else class="text-xs text-err">{{ statusText }}</span>
        <button
          v-if="active"
          class="rounded-md border border-line px-2 py-0.5 text-xs text-slate-400 hover:border-err hover:text-err"
          @click="jobs.cancel(job.id)"
        >
          Cancel
        </button>
      </span>
    </div>

    <div v-if="active" class="mt-2 space-y-1.5">
      <div class="h-1.5 overflow-hidden rounded-full bg-surface-3">
        <div
          class="h-full w-1/3 animate-[slide_1.2s_ease-in-out_infinite] rounded-full bg-gradient-to-r from-[var(--color-accent-2)] to-[var(--color-accent)]"
        ></div>
      </div>
      <div class="flex justify-between gap-3 text-xs tabular-nums text-slate-500">
        <span class="truncate">{{ job.fileName }}</span>
        <span class="shrink-0">
          <template v-if="job.totalFiles > 0">file {{ job.currentFile }}/{{ job.totalFiles }} · </template>
          <template v-else>file {{ job.currentFile }} · </template>
          {{ formatBytes(job.bytesDone) }}
        </span>
      </div>
    </div>

    <p v-if="job.error" class="mt-2 text-xs text-err">{{ job.error }}</p>
    <p v-else-if="job.state === 'done' && job.resultDir" class="mt-1.5 truncate font-mono text-xs text-slate-500">
      {{ job.resultDir }}
    </p>
  </div>
</template>
