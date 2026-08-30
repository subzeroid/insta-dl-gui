<script setup lang="ts">
import { useId } from "vue";

const props = withDefaults(defineProps<{
  selected: boolean;
  label: string;
  disabled?: boolean;
  disabledReason?: string;
}>(), {
  disabled: false,
  disabledReason: undefined,
});

const emit = defineEmits<{
  toggle: [];
}>();

const disabledReasonId = `media-selection-disabled-${useId()}`;

function onChange() {
  if (!props.disabled) emit("toggle");
}
</script>

<template>
  <label
    class="absolute left-2 top-2 z-10 flex size-7 items-center justify-center rounded-md border border-line bg-surface-1/90 text-sm text-slate-100 shadow-sm"
    :class="props.disabled
      ? 'cursor-not-allowed opacity-60'
      : 'cursor-pointer focus-within:ring-2 focus-within:ring-accent focus-within:ring-offset-2 focus-within:ring-offset-surface-0'"
    :title="props.disabledReason"
    @pointerdown.stop
    @click.stop
  >
    <input
      type="checkbox"
      class="sr-only"
      :checked="props.selected"
      :disabled="props.disabled"
      :aria-label="props.label"
      :aria-describedby="props.disabledReason ? disabledReasonId : undefined"
      @change="onChange"
    />
    <span v-if="props.selected" aria-hidden="true">✓</span>
    <span v-if="props.disabledReason" :id="disabledReasonId" class="sr-only">
      {{ props.disabledReason }}
    </span>
  </label>
</template>
