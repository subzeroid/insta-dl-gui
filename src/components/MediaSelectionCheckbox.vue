<script setup lang="ts">
const props = defineProps<{
  selected: boolean;
  label: string;
}>();

const emit = defineEmits<{
  toggle: [];
}>();
</script>

<template>
  <label
    class="absolute left-2 top-2 z-10 flex size-7 cursor-pointer items-center justify-center rounded-md border border-line bg-surface-1/90 text-sm text-slate-100 shadow-sm focus-within:ring-2 focus-within:ring-accent focus-within:ring-offset-2 focus-within:ring-offset-surface-0"
    @pointerdown.stop
    @click.stop
  >
    <input
      type="checkbox"
      class="sr-only"
      :checked="props.selected"
      :aria-label="props.label"
      @change="emit('toggle')"
    />
    <span v-if="props.selected" aria-hidden="true">✓</span>
  </label>
</template>
