<script setup lang="ts">
import { ref, watch } from "vue";

defineOptions({ inheritAttrs: false });

const props = withDefaults(defineProps<{
  disabled?: boolean;
}>(), {
  disabled: false,
});

const model = defineModel<string>({ required: true });
const visible = ref(false);

watch([model, () => props.disabled], () => {
  if (props.disabled || !model.value) visible.value = false;
});

function toggleVisibility() {
  if (!props.disabled) visible.value = !visible.value;
}
</script>

<template>
  <div class="relative min-w-0 flex-1">
    <input
      v-bind="$attrs"
      v-model="model"
      class="input w-full pr-11"
      :type="visible ? 'text' : 'password'"
      :disabled="props.disabled"
    />
    <button
      class="absolute inset-y-0 right-0 flex w-11 items-center justify-center text-slate-400 hover:text-slate-200 disabled:cursor-not-allowed disabled:opacity-50"
      type="button"
      :aria-label="visible ? 'Hide token' : 'Show token'"
      :aria-pressed="visible"
      :disabled="props.disabled"
      @click="toggleVisibility"
    >
      <svg
        aria-hidden="true"
        class="size-4"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        stroke-width="2"
      >
        <path stroke-linecap="round" stroke-linejoin="round" d="M2.25 12s3.5-6 9.75-6 9.75 6 9.75 6-3.5 6-9.75 6-9.75-6-9.75-6Z" />
        <circle cx="12" cy="12" r="2.5" />
        <path v-if="visible" stroke-linecap="round" d="m4 4 16 16" />
      </svg>
    </button>
  </div>
</template>
