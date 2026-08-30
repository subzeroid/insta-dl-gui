<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { useJobsStore, type JobView } from "../stores/jobs";
import DownloadJobDetails from "../components/DownloadJobDetails.vue";
import JobCard from "../components/JobCard.vue";

const jobs = useJobsStore();
const list = computed(() => [...jobs.jobs.values()].sort((a, b) => a.id.localeCompare(b.id)));
const anyFinished = computed(() =>
  list.value.some((j) => j.state === "done" || j.state === "failed" || j.state === "cancelled"),
);
const selectedJobId = ref<string | null>(null);
const selectedJob = computed(() =>
  selectedJobId.value ? jobs.jobs.get(selectedJobId.value) : undefined,
);
let selectionOrigin: HTMLElement | null = null;

function openDetails(job: JobView, origin: HTMLElement) {
  if (job.state !== "done" || !job.outputs?.length) return;
  selectedJobId.value = job.id;
  selectionOrigin = origin;
}

function closeDetails(restoreFocus = true) {
  const origin = selectionOrigin;
  selectedJobId.value = null;
  selectionOrigin = null;
  if (restoreFocus && origin?.isConnected) void nextTick(() => origin.focus());
}

function clearFinished() {
  closeDetails(false);
  jobs.clearFinished();
}

watch(selectedJob, (current) => {
  if (!current && selectedJobId.value) closeDetails(false);
});
</script>

<template>
  <div class="mx-auto max-w-3xl space-y-4 p-6">
    <div class="flex items-center justify-between">
      <h2 class="text-lg font-semibold">Queue</h2>
      <button
        v-if="anyFinished"
        data-action="clear-finished"
        class="btn-secondary text-xs"
        @click="clearFinished"
      >
        Clear finished
      </button>
    </div>
    <div v-if="list.length === 0" class="card flex items-center justify-center p-12 text-sm text-slate-500">
      No downloads yet.
    </div>
    <div v-else class="space-y-3">
      <JobCard v-for="job in list" :key="job.id" :job="job" @inspect="openDetails" />
    </div>
    <DownloadJobDetails v-if="selectedJob" :job="selectedJob" @close="closeDetails" />
  </div>
</template>
