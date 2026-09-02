<script setup lang="ts">
import { computed, ref, watch } from "vue";

import { remoteMediaUrl } from "../lib/ipc";
import { useRemoteMediaHealthStore } from "../stores/remoteMediaHealth";
import MediaPreviewPlaceholder from "./MediaPreviewPlaceholder.vue";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  source?: string | null;
  label: string;
  controls?: boolean;
}>(), {
  source: null,
  controls: true,
});

type RemoteVideoState = "loading" | "loaded" | "failed";

const health = useRemoteMediaHealthStore();
const state = ref<RemoteVideoState>("failed");
const requestKey = ref(0);
const originalSource = computed(() => props.source?.trim() ?? "");
const resolvedSource = computed(() =>
  originalSource.value ? remoteMediaUrl(originalSource.value) : "",
);
const canAttempt = computed(() => resolvedSource.value.length > 0);

watch(
  [originalSource, () => health.retryGeneration],
  () => {
    requestKey.value += 1;
    state.value = canAttempt.value ? "loading" : "failed";
  },
  { immediate: true },
);

function onLoadedMetadata() {
  state.value = "loaded";
  health.reportSuccess(originalSource.value);
}

function onError() {
  state.value = "failed";
  health.reportFailure(originalSource.value);
}
</script>

<template>
  <span
    v-bind="$attrs"
    data-remote-video
    :data-state="state"
    class="relative block overflow-hidden bg-black"
  >
    <MediaPreviewPlaceholder
      v-if="state !== 'loaded'"
      variant="modal"
      :label="props.label"
      :unavailable="state === 'failed'"
    />
    <video
      v-if="canAttempt && state !== 'failed'"
      :key="requestKey"
      :src="resolvedSource"
      :controls="props.controls && state === 'loaded'"
      :aria-label="state === 'loaded' ? props.label : undefined"
      :aria-hidden="state === 'loaded' ? undefined : true"
      :tabindex="state === 'loaded' ? undefined : -1"
      preload="metadata"
      class="absolute inset-0 h-full w-full object-contain transition-opacity"
      :class="state === 'loaded' ? 'opacity-100' : 'pointer-events-none opacity-0'"
      @loadedmetadata="onLoadedMetadata"
      @error="onError"
    />
  </span>
</template>
