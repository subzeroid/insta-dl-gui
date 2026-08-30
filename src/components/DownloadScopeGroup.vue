<script setup lang="ts">
import { computed, useId } from "vue";

const props = defineProps<{
  shownCount: number;
  selectedCount: number;
  busy: boolean;
  allTitle?: string;
  helperText?: string;
  shownDisabledReason?: string;
  selectedDisabledReason?: string;
}>();

const emit = defineEmits<{
  "download-all": [];
  "download-shown": [];
  "download-selected": [];
}>();

const descriptionBase = useId().replace(/:/g, "");
const helperId = `${descriptionBase}-download-helper`;
const shownReasonId = `${descriptionBase}-shown-reason`;
const selectedReasonId = `${descriptionBase}-selected-reason`;
const shownDescribedBy = computed(() =>
  [props.helperText ? helperId : "", props.shownDisabledReason ? shownReasonId : ""]
    .filter(Boolean)
    .join(" ") || undefined,
);
const selectedDescribedBy = computed(() =>
  [props.helperText ? helperId : "", props.selectedDisabledReason ? selectedReasonId : ""]
    .filter(Boolean)
    .join(" ") || undefined,
);
</script>

<template>
  <div class="space-y-1">
    <div role="group" aria-label="Download" class="flex items-center gap-2">
      <span class="text-sm font-medium text-slate-300">Download</span>
      <div class="inline-flex overflow-hidden rounded-md border border-line bg-surface-1">
        <button
          type="button"
          class="border-r border-line px-2.5 py-1 text-xs text-slate-300 hover:bg-surface-2 disabled:cursor-not-allowed disabled:text-slate-600"
          :disabled="props.busy"
          :title="props.allTitle ?? 'Download all fetched items'"
          @click="emit('download-all')"
        >
          All
        </button>
        <button
          type="button"
          class="border-r border-line px-2.5 py-1 text-xs text-slate-300 hover:bg-surface-2 disabled:cursor-not-allowed disabled:text-slate-600"
          :disabled="props.busy || props.shownCount === 0 || Boolean(props.shownDisabledReason)"
          :title="props.shownDisabledReason ?? 'Download the items currently shown'"
          :aria-describedby="shownDescribedBy"
          @click="emit('download-shown')"
        >
          Shown {{ props.shownCount }}
        </button>
        <button
          type="button"
          class="px-2.5 py-1 text-xs text-slate-300 hover:bg-surface-2 disabled:cursor-not-allowed disabled:text-slate-600"
          :disabled="props.busy || props.selectedCount === 0 || Boolean(props.selectedDisabledReason)"
          :title="props.selectedDisabledReason ?? 'Download selected items'"
          :aria-describedby="selectedDescribedBy"
          @click="emit('download-selected')"
        >
          Selected {{ props.selectedCount }}
        </button>
      </div>
    </div>
    <div
      v-if="props.helperText || props.shownDisabledReason || props.selectedDisabledReason"
      class="max-w-sm text-right text-[11px] leading-4 text-slate-500"
    >
      <p v-if="props.helperText" :id="helperId">{{ props.helperText }}</p>
      <p v-if="props.shownDisabledReason" :id="shownReasonId" class="text-amber-400">
        {{ props.shownDisabledReason }}
      </p>
      <p v-if="props.selectedDisabledReason" :id="selectedReasonId" class="text-amber-400">
        {{ props.selectedDisabledReason }}
      </p>
    </div>
  </div>
</template>
