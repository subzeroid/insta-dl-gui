<script setup lang="ts">
import { computed } from "vue";

import type { PostDisplayType } from "../lib/postDisplay";

const props = defineProps<PostDisplayType>();

const validCarousel = computed(
  () => props.kind === "carousel" && Number.isInteger(props.count) && props.count > 0,
);

const label = computed(() => {
  if (props.kind === "photo") return "PHOTO";
  if (props.kind === "video") return "VIDEO";
  if (validCarousel.value) return `CAROUSEL · ${props.count}`;
  return "POST";
});

const colorClass = computed(() => {
  if (props.kind === "photo") return "bg-sky-700";
  if (props.kind === "video") return "bg-rose-700";
  if (validCarousel.value) return "bg-amber-700";
  return "bg-slate-700";
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
