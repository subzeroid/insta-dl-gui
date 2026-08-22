<script setup lang="ts">
import { computed } from "vue";
import { useJobsStore } from "../stores/jobs";
import JobCard from "../components/JobCard.vue";

const jobs = useJobsStore();
const list = computed(() => [...jobs.jobs.values()].sort((a, b) => a.id.localeCompare(b.id)));
const anyFinished = computed(() =>
  list.value.some((j) => j.state === "done" || j.state === "failed" || j.state === "cancelled"),
);
</script>

<template>
  <div class="mx-auto max-w-3xl space-y-4 p-6">
    <div class="flex items-center justify-between">
      <h2 class="text-lg font-semibold">Queue</h2>
      <button v-if="anyFinished" class="btn-secondary text-xs" @click="jobs.clearFinished()">Clear finished</button>
    </div>
    <div v-if="list.length === 0" class="card flex items-center justify-center p-12 text-sm text-slate-500">
      No downloads yet.
    </div>
    <div v-else class="space-y-3">
      <JobCard v-for="job in list" :key="job.id" :job="job" />
    </div>
  </div>
</template>
