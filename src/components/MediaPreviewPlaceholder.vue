<script setup lang="ts">
import { computed } from "vue";

import type { RemoteMediaVariant } from "./remoteMedia";

const props = withDefaults(defineProps<{
  variant: RemoteMediaVariant;
  label?: string;
  unavailable?: boolean;
}>(), {
  label: "",
  unavailable: false,
});

const isAvatarLike = computed(() =>
  ["compact-avatar", "avatar", "story"].includes(props.variant),
);
const isAccessibleImage = computed(() => props.unavailable && Boolean(props.label));
</script>

<template>
  <span
    class="absolute inset-0 flex flex-col items-center justify-center gap-2 bg-surface-2 text-slate-500"
    :role="isAccessibleImage ? 'img' : undefined"
    :aria-label="isAccessibleImage ? props.label : undefined"
    :aria-hidden="isAccessibleImage ? undefined : 'true'"
  >
    <svg
      v-if="isAvatarLike"
      data-glyph="user-outline"
      class="size-6"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      aria-hidden="true"
      focusable="false"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        d="M15.75 6a3.75 3.75 0 1 1-7.5 0 3.75 3.75 0 0 1 7.5 0ZM4.5 20.12a7.5 7.5 0 0 1 15 0A17.93 17.93 0 0 1 12 21.75a17.93 17.93 0 0 1-7.5-1.63Z"
      />
    </svg>
    <svg
      v-else
      data-glyph="image-outline"
      class="size-6"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      aria-hidden="true"
      focusable="false"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        d="m2.25 15.75 5.16-5.16a2.25 2.25 0 0 1 3.18 0l5.16 5.16m-1.5-1.5 1.41-1.41a2.25 2.25 0 0 1 3.18 0l2.91 2.91M3.75 19.5h16.5a1.5 1.5 0 0 0 1.5-1.5V6a1.5 1.5 0 0 0-1.5-1.5H3.75A1.5 1.5 0 0 0 2.25 6v12a1.5 1.5 0 0 0 1.5 1.5Zm12-11.25h.008v.008h-.008V8.25Z"
      />
    </svg>
    <span v-if="props.unavailable && props.variant === 'modal'">
      Preview unavailable
    </span>
  </span>
</template>
