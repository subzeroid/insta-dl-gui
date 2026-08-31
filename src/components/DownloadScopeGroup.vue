<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId, watch } from "vue";

const props = defineProps<{
  shownCount: number;
  selectedCount: number;
  busy: boolean;
  allTitle?: string;
  shownDisabledReason?: string;
  selectedDisabledReason?: string;
}>();

const emit = defineEmits<{
  "download-all": [];
  "download-shown": [];
  "download-selected": [];
}>();

const descriptionBase = useId().replace(/:/g, "");
const helpId = `${descriptionBase}-download-help`;
const shownReasonId = `${descriptionBase}-shown-reason`;
const selectedReasonId = `${descriptionBase}-selected-reason`;
const helpOpen = ref(false);
const helpButton = ref<HTMLButtonElement>();
const shownDescribedBy = computed(() =>
  [props.shownDisabledReason ? shownReasonId : ""]
    .filter(Boolean)
    .join(" ") || undefined,
);
const selectedDescribedBy = computed(() =>
  [props.selectedDisabledReason ? selectedReasonId : ""]
    .filter(Boolean)
    .join(" ") || undefined,
);
const allAvailable = computed(() => !props.busy);
const shownAvailable = computed(
  () => !props.busy && props.shownCount > 0 && !props.shownDisabledReason,
);
const selectedAvailable = computed(
  () => !props.busy && props.selectedCount > 0 && !props.selectedDisabledReason,
);

function closeHelp(restoreFocus = false) {
  if (!helpOpen.value) return;
  helpOpen.value = false;
  if (restoreFocus) void nextTick(() => helpButton.value?.focus());
}

function onDocumentPointerDown(event: PointerEvent) {
  const target = event.target;
  if (target instanceof Node && !helpButton.value?.parentElement?.contains(target)) closeHelp();
}

function onHelpKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  event.preventDefault();
  closeHelp(true);
}

function onDocumentKeydown(event: KeyboardEvent) {
  if (helpOpen.value) onHelpKeydown(event);
}

watch(
  helpOpen,
  (isOpen) => {
    if (isOpen) document.addEventListener("keydown", onDocumentKeydown);
    else document.removeEventListener("keydown", onDocumentKeydown);
  },
  { flush: "sync" },
);

onMounted(() => document.addEventListener("pointerdown", onDocumentPointerDown));
onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", onDocumentPointerDown);
  document.removeEventListener("keydown", onDocumentKeydown);
});
</script>

<template>
  <div class="space-y-1">
    <div class="flex items-center gap-1.5">
      <div role="group" aria-label="Download" class="flex items-center gap-1.5">
        <span class="text-sm font-medium text-slate-300">Download</span>
        <div class="inline-flex overflow-hidden rounded-md border border-line bg-surface-1">
          <button
            type="button"
            class="border-r border-white/15 px-2 py-1 text-xs font-medium transition last:border-r-0 focus-visible:relative focus-visible:z-10 focus-visible:outline-2 focus-visible:outline-accent-3 disabled:cursor-not-allowed"
            :class="allAvailable ? 'download-scope-enabled' : 'download-scope-disabled'"
            :data-download-available="allAvailable"
            :disabled="!allAvailable"
            :title="props.allTitle ?? 'Download all fetched items'"
            @click="emit('download-all')"
          >
            All
          </button>
          <button
            type="button"
            class="border-r border-white/15 px-2 py-1 text-xs font-medium transition last:border-r-0 focus-visible:relative focus-visible:z-10 focus-visible:outline-2 focus-visible:outline-accent-3 disabled:cursor-not-allowed"
            :class="shownAvailable ? 'download-scope-enabled' : 'download-scope-disabled'"
            :data-download-available="shownAvailable"
            :disabled="!shownAvailable"
            :title="props.shownDisabledReason ?? 'Download the items currently shown'"
            :aria-describedby="shownDescribedBy"
            @click="emit('download-shown')"
          >
            Shown {{ props.shownCount }}
          </button>
          <button
            type="button"
            class="border-r border-white/15 px-2 py-1 text-xs font-medium transition last:border-r-0 focus-visible:relative focus-visible:z-10 focus-visible:outline-2 focus-visible:outline-accent-3 disabled:cursor-not-allowed"
            :class="selectedAvailable ? 'download-scope-enabled' : 'download-scope-disabled'"
            :data-download-available="selectedAvailable"
            :disabled="!selectedAvailable"
            :title="props.selectedDisabledReason ?? 'Download selected items'"
            :aria-describedby="selectedDescribedBy"
            @click="emit('download-selected')"
          >
            Selected {{ props.selectedCount }}
          </button>
        </div>
      </div>
      <div class="relative">
        <button
          ref="helpButton"
          type="button"
          data-action="scope-help"
          class="flex size-6 items-center justify-center rounded-full border border-line text-xs font-semibold text-slate-400 hover:bg-surface-2 hover:text-slate-100"
          aria-label="Explain download scopes"
          :aria-expanded="helpOpen"
          :aria-controls="helpId"
          @click="helpOpen = !helpOpen"
          @keydown="onHelpKeydown"
        >
          ?
        </button>
        <div
          v-if="helpOpen"
          :id="helpId"
          role="dialog"
          aria-label="Download scope help"
          class="absolute right-0 top-8 z-20 w-80 rounded-lg border border-line bg-surface-2 p-3 text-left text-xs leading-5 text-slate-300 shadow-xl"
          @keydown="onHelpKeydown"
        >
          <p><span class="font-medium text-slate-100">All</span> downloads the complete category archive and may make API requests.</p>
          <p class="mt-2"><span class="font-medium text-slate-100">Shown</span> downloads exactly the currently visible items and reflects the active Posts media filter.</p>
          <p class="mt-2"><span class="font-medium text-slate-100">Selected</span> downloads every selected item, including items hidden by the current filter.</p>
          <p class="mt-2 text-slate-400">Exact Shown and Selected snapshots are limited to 500 items.</p>
        </div>
      </div>
    </div>
    <div
      v-if="props.shownDisabledReason || props.selectedDisabledReason"
      class="max-w-sm text-right text-[11px] leading-4 text-slate-500"
    >
      <p v-if="props.shownDisabledReason" :id="shownReasonId" class="text-amber-400">
        {{ props.shownDisabledReason }}
      </p>
      <p v-if="props.selectedDisabledReason" :id="selectedReasonId" class="text-amber-400">
        {{ props.selectedDisabledReason }}
      </p>
    </div>
  </div>
</template>

<style scoped>
.download-scope-enabled {
  color: white;
  background: linear-gradient(45deg, var(--color-accent-2), var(--color-accent));
}

.download-scope-enabled:hover {
  filter: brightness(1.1);
}

.download-scope-disabled {
  color: rgb(71 85 105);
  background: var(--color-surface-1);
}
</style>
