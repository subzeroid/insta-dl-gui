<script setup lang="ts">
import { ref } from "vue";

const jobs = ref<{ id: string; label: string }[]>([]);
</script>

<template>
  <div class="mx-auto max-w-3xl space-y-4 p-6">
    <h2 class="text-lg font-semibold">Queue</h2>
    <div v-if="jobs.length === 0" class="card flex items-center justify-center p-12 text-sm text-slate-500">
      No downloads yet.
    </div>
  </div>
</template>
