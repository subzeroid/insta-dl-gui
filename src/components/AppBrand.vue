<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    version?: string | null;
    size?: "compact" | "large";
    align?: "start" | "center";
  }>(),
  {
    size: "compact",
    align: "start",
  },
);

const normalizedVersion = computed(() => props.version?.trim() || null);
</script>

<template>
  <span
    data-app-brand
    class="inline-flex flex-col"
    :class="align === 'center' ? 'items-center text-center' : 'items-start'"
  >
    <span
      data-app-name
      class="bg-gradient-to-r from-[var(--color-accent-2)] to-[var(--color-accent)] bg-clip-text font-bold text-transparent select-none"
      :class="size === 'large' ? 'text-2xl' : 'text-lg'"
      >insta-dl-gui</span
    >
    <span v-if="normalizedVersion" data-app-version class="text-[10px] tabular-nums text-slate-400">v{{ normalizedVersion }}</span>
  </span>
</template>
