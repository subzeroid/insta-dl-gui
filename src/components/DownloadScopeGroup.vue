<script setup lang="ts">
const props = defineProps<{
  shownCount: number;
  selectedCount: number;
  busy: boolean;
}>();

const emit = defineEmits<{
  "download-all": [];
  "download-shown": [];
  "download-selected": [];
}>();
</script>

<template>
  <div role="group" aria-label="Download" class="flex items-center gap-2">
    <span class="text-sm font-medium text-slate-300">Download</span>
    <div class="inline-flex overflow-hidden rounded-md border border-line bg-surface-1">
      <button
        type="button"
        class="border-r border-line px-2.5 py-1 text-xs text-slate-300 hover:bg-surface-2 disabled:cursor-not-allowed disabled:text-slate-600"
        :disabled="props.busy"
        title="Download all fetched items"
        @click="emit('download-all')"
      >
        All
      </button>
      <button
        type="button"
        class="border-r border-line px-2.5 py-1 text-xs text-slate-300 hover:bg-surface-2 disabled:cursor-not-allowed disabled:text-slate-600"
        :disabled="props.busy || props.shownCount === 0"
        title="Download the items currently shown"
        @click="emit('download-shown')"
      >
        Shown {{ props.shownCount }}
      </button>
      <button
        type="button"
        class="px-2.5 py-1 text-xs text-slate-300 hover:bg-surface-2 disabled:cursor-not-allowed disabled:text-slate-600"
        :disabled="props.busy || props.selectedCount === 0"
        title="Download selected items"
        @click="emit('download-selected')"
      >
        Selected {{ props.selectedCount }}
      </button>
    </div>
  </div>
</template>
