<script setup lang="ts">
import { computed, ref, watch } from "vue";

import { remoteMediaUrl } from "../lib/ipc";
import { useRemoteMediaHealthStore } from "../stores/remoteMediaHealth";
import MediaPreviewPlaceholder from "./MediaPreviewPlaceholder.vue";
import type { RemoteMediaVariant } from "./remoteMedia";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  source?: string | null;
  alt: string;
  loading?: "eager" | "lazy";
  variant: RemoteMediaVariant;
}>(), {
  source: null,
  loading: "eager",
});

type RemoteImageState = "loading" | "loaded" | "failed";

const health = useRemoteMediaHealthStore();
const state = ref<RemoteImageState>("failed");
const requestKey = ref(0);
const originalSource = computed(() => props.source?.trim() ?? "");
const resolvedSource = computed(() =>
  originalSource.value ? remoteMediaUrl(originalSource.value) : "",
);
const canAttempt = computed(() => resolvedSource.value.length > 0);
const geometryClass = computed(() => {
  if (["compact-avatar", "avatar", "story"].includes(props.variant)) {
    return "rounded-full";
  }
  if (props.variant === "thumbnail") return "rounded-lg";
  return "";
});

watch(
  [originalSource, () => health.retryGeneration],
  () => {
    requestKey.value += 1;
    state.value = canAttempt.value ? "loading" : "failed";
  },
  { immediate: true },
);

function onLoad() {
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
    data-remote-image
    :data-state="state"
    :data-variant="props.variant"
    class="relative block overflow-hidden bg-surface-2"
    :class="geometryClass"
  >
    <MediaPreviewPlaceholder
      v-if="state !== 'loaded'"
      :variant="props.variant"
      :label="props.alt"
      :unavailable="state === 'failed'"
    />
    <img
      v-if="canAttempt && state !== 'failed'"
      :key="requestKey"
      :src="resolvedSource"
      :alt="props.alt"
      :loading="props.loading"
      referrerpolicy="no-referrer"
      class="absolute inset-0 h-full w-full"
      :class="[
        props.variant === 'modal' ? 'object-contain' : 'object-cover',
        state === 'loaded' ? 'opacity-100' : 'opacity-0',
      ]"
      @load="onLoad"
      @error="onError"
    />
  </span>
</template>
