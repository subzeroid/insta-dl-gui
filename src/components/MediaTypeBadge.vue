<script setup lang="ts">
import { computed } from "vue";

import type { PostDisplayKind } from "../lib/postDisplay";

const props = withDefaults(
  defineProps<{
    kind: PostDisplayKind;
    count?: number;
  }>(),
  { count: 0 },
);

const label = computed(() => {
  if (props.kind === "photo") return "PHOTO";
  if (props.kind === "video") return "VIDEO";
  if (props.kind === "carousel") return `CAROUSEL · ${props.count}`;
  return "POST";
});

const colorClass = computed(() => {
  if (props.kind === "photo") return "bg-sky-500/80";
  if (props.kind === "video") return "bg-rose-500/80";
  if (props.kind === "carousel") return "bg-amber-500/80";
  return "bg-slate-700/80";
});
</script>

<template>
  <span
    role="img"
    :aria-label="label"
    :class="[colorClass, 'inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-semibold tracking-wide text-white shadow-sm']"
  >
    {{ label }}
  </span>
</template>
