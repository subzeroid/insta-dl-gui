<script setup lang="ts">
import { computed } from "vue";
import { useJobsStore, type JobView } from "../stores/jobs";
import { formatBytes } from "../lib/ipc";

const props = defineProps<{ job: JobView }>();
const emit = defineEmits<{ inspect: [job: JobView, origin: HTMLElement] }>();
const jobs = useJobsStore();

const active = computed(() => props.job.state === "downloading" || props.job.state === "fetching");
const resultCount = computed(() => props.job.resultCount ?? 0);
const catalogWarnings = computed(() => props.job.catalogWarnings ?? 0);
const resourceFailures = computed(() => props.job.resourceFailures ?? 0);
const doneWithWarnings = computed(
  () => props.job.state === "done" && (catalogWarnings.value > 0 || resourceFailures.value > 0),
);
const actionable = computed(
  () => props.job.state === "done" && (props.job.outputs?.length ?? 0) > 0,
);
const fileCountText = computed(
  () => `${resultCount.value} file${resultCount.value === 1 ? "" : "s"}`,
);
const statusText = computed(() => {
  switch (props.job.state) {
    case "downloading":
      return "downloading…";
    case "fetching":
      return "fetching…";
    case "done": {
      if (resourceFailures.value > 0) {
        return `saved ${fileCountText.value} / ${resourceFailures.value} resource failure${resourceFailures.value === 1 ? "" : "s"}`;
      }
      if (catalogWarnings.value > 0) {
        return `saved ${fileCountText.value} with warnings`;
      }
      return `✓ ${fileCountText.value}`;
    }
    case "cancelled":
      return "cancelled";
    case "failed":
      return "failed";
  }
});

function inspect(event: MouseEvent | KeyboardEvent) {
  if (!actionable.value) return;
  const origin = event.currentTarget;
  if (!(origin instanceof HTMLElement)) return;
  emit("inspect", props.job, origin);
}

function onKeydown(event: KeyboardEvent) {
  if (event.key !== "Enter" && event.key !== " ") return;
  event.preventDefault();
  inspect(event);
}
</script>

<template>
  <div
    class="card p-4"
    :class="actionable ? 'cursor-pointer hover:border-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent' : ''"
    :data-job-id="job.id"
    :role="actionable ? 'button' : undefined"
    :tabindex="actionable ? 0 : undefined"
    :aria-label="actionable ? `Inspect downloaded files for ${job.label}` : undefined"
    @click="inspect"
    @keydown="onKeydown"
  >
    <div class="flex items-center justify-between gap-3">
      <span class="truncate font-medium text-slate-200">{{ job.label }}</span>
      <span class="flex shrink-0 items-center gap-2">
        <span
          v-if="active"
          class="animate-pulse text-xs text-accent"
          >{{ statusText }}</span
        >
        <span
          v-else-if="job.state === 'done'"
          class="text-xs"
          :class="doneWithWarnings ? 'text-warn' : 'text-ok'"
          >{{ statusText }}</span
        >
        <span v-else-if="job.state === 'cancelled'" class="text-xs text-warn">{{ statusText }}</span>
        <span v-else class="text-xs text-err">{{ statusText }}</span>
        <button
          v-if="active"
          class="rounded-md border border-line px-2 py-0.5 text-xs text-slate-400 hover:border-err hover:text-err"
          @click.stop="jobs.cancel(job.id)"
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
    <p v-else-if="job.state === 'done' && catalogWarnings > 0" class="mt-2 text-xs text-warn">
      Files are saved, but Library indexing failed for {{ catalogWarnings }}
      {{ catalogWarnings === 1 ? "item" : "items" }}. Rescan the Library.
    </p>
    <p v-else-if="job.state === 'done' && job.resultDir" class="mt-1.5 truncate font-mono text-xs text-slate-500">
      {{ job.resultDir }}
    </p>
  </div>
</template>
