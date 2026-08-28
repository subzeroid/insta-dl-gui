<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";

import type { LibraryCardView } from "../stores/library";

const props = defineProps<{ card: LibraryCardView }>();
const emit = defineEmits<{
  select: [card: LibraryCardView, origin: HTMLElement];
}>();

const root = ref<HTMLElement | null>(null);
const nearViewport = ref(false);
const previewFailed = ref(false);
let observer: IntersectionObserver | null = null;

const label = computed(() => {
  const owner = props.card.owner_username ? `@${props.card.owner_username}` : "Unknown owner";
  return `Open ${props.card.kind} from ${owner}`;
});

const date = computed(() => {
  const timestamp = props.card.taken_at ?? props.card.imported_at;
  return new Intl.DateTimeFormat(undefined, {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(new Date(timestamp * 1_000));
});

const showsPreview = computed(
  () => nearViewport.value && props.card.previewUrl !== null && !previewFailed.value,
);

function select(origin: EventTarget | null) {
  if (origin instanceof HTMLElement) emit("select", props.card, origin);
}

function onKeydown(event: KeyboardEvent) {
  if (event.key !== "Enter" && event.key !== " ") return;
  event.preventDefault();
  select(event.currentTarget);
}

function disconnectObserver() {
  observer?.disconnect();
  observer = null;
}

function observePreview() {
  disconnectObserver();
  if (!root.value || props.card.previewUrl === null) {
    nearViewport.value = false;
    return;
  }
  if (nearViewport.value) return;
  if (typeof IntersectionObserver === "undefined") {
    nearViewport.value = true;
    return;
  }
  observer = new IntersectionObserver(
    (entries) => {
      if (!entries.some((entry) => entry.isIntersecting)) return;
      nearViewport.value = true;
      disconnectObserver();
    },
    {
      root: root.value.closest("main"),
      rootMargin: "600px 0px",
    },
  );
  observer.observe(root.value);
}

watch(
  [() => props.card.previewUrl, () => props.card.previewFileKind],
  () => {
    previewFailed.value = false;
    observePreview();
  },
  { flush: "post" },
);

onMounted(() => {
  observePreview();
});

onBeforeUnmount(() => disconnectObserver());
</script>

<template>
  <button
    ref="root"
    type="button"
    class="library-card group text-left"
    :data-library-card-id="card.id"
    :aria-label="label"
    @click="select($event.currentTarget)"
    @keydown="onKeydown"
  >
    <span class="relative block h-[216px] overflow-hidden bg-surface-2">
      <video
        v-if="showsPreview && card.previewFileKind === 'video'"
        :src="card.previewUrl ?? undefined"
        class="h-full w-full object-cover"
        muted
        playsinline
        preload="metadata"
        @error="previewFailed = true"
      />
      <img
        v-else-if="showsPreview"
        :src="card.previewUrl ?? undefined"
        :alt="card.caption || `${card.kind} preview`"
        class="h-full w-full object-cover"
        @error="previewFailed = true"
      />
      <span
        v-else
        class="library-preview-placeholder absolute inset-0 flex items-center justify-center text-xs font-medium uppercase tracking-[0.18em] text-slate-500"
        aria-hidden="true"
      >
        {{ card.previewFileKind === "video" ? "Video" : "Local media" }}
      </span>
      <span class="absolute left-2 top-2 rounded-md bg-black/70 px-2 py-1 text-[10px] font-semibold uppercase tracking-wide text-slate-100">
        {{ card.kind }}
      </span>
      <span
        v-if="card.resource_count > 1"
        class="absolute right-2 top-2 rounded-md bg-black/70 px-2 py-1 text-[10px] font-semibold text-slate-100"
      >
        {{ card.resource_count }} files
      </span>
      <span
        v-if="card.availability === 'missing'"
        class="absolute bottom-2 left-2 rounded-md border border-warn/50 bg-black/80 px-2 py-1 text-[10px] font-semibold uppercase tracking-wide text-warn"
      >
        Missing
      </span>
    </span>
    <span class="block space-y-1 p-3">
      <span class="block truncate text-sm font-medium text-slate-200">
        {{ card.owner_username ? `@${card.owner_username}` : "Unknown owner" }}
      </span>
      <span class="block text-xs text-slate-500">{{ date }}</span>
      <span v-if="card.caption" class="block truncate text-xs text-slate-400">{{ card.caption }}</span>
    </span>
  </button>
</template>
