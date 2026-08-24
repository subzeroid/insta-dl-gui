<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";

import type { LibraryCardView } from "../stores/library";
import LibraryCard from "./LibraryCard.vue";

export interface LibraryViewport {
  width: number;
  height: number;
  scrollTop: number;
}

const props = defineProps<{
  cards: LibraryCardView[];
  testViewport?: LibraryViewport;
}>();

const emit = defineEmits<{
  select: [card: LibraryCardView, origin: HTMLElement];
}>();

const host = ref<HTMLElement | null>(null);
const width = ref(props.testViewport?.width ?? 0);
const height = ref(props.testViewport?.height ?? 0);
const scrollTop = ref(props.testViewport?.scrollTop ?? 0);
let viewport: HTMLElement | null = null;
let resizeObserver: ResizeObserver | null = null;

const CARD_MIN_WIDTH = 216;
const COLUMN_GAP = 16;
const ROW_HEIGHT = 316;
const OVERSCAN_ROWS = 2;

const columns = computed(() =>
  Math.max(1, Math.floor((Math.max(width.value, CARD_MIN_WIDTH) + COLUMN_GAP) / (CARD_MIN_WIDTH + COLUMN_GAP))),
);
const totalRows = computed(() => Math.ceil(props.cards.length / columns.value));
const visibleRows = computed(() => Math.max(1, Math.ceil(height.value / ROW_HEIGHT)));
const firstVisibleRow = computed(() =>
  Math.floor(Math.max(0, scrollTop.value) / ROW_HEIGHT),
);
const windowRows = computed(() => Math.min(totalRows.value, visibleRows.value + OVERSCAN_ROWS));
const startRow = computed(() => {
  const centeredStart = Math.max(0, firstVisibleRow.value - Math.floor(OVERSCAN_ROWS / 2));
  return Math.min(Math.max(0, totalRows.value - windowRows.value), centeredStart);
});
const endRow = computed(() =>
  Math.min(totalRows.value, startRow.value + windowRows.value),
);
const startIndex = computed(() => startRow.value * columns.value);
const endIndex = computed(() => Math.min(props.cards.length, endRow.value * columns.value));
const visibleCards = computed(() => props.cards.slice(startIndex.value, endIndex.value));
const topSpace = computed(() => startRow.value * ROW_HEIGHT);
const bottomSpace = computed(() => Math.max(0, (totalRows.value - endRow.value) * ROW_HEIGHT));

function measure() {
  if (props.testViewport) {
    width.value = props.testViewport.width;
    height.value = props.testViewport.height;
    scrollTop.value = props.testViewport.scrollTop;
    return;
  }
  if (!host.value) return;
  viewport = host.value.closest("main");
  const hostRect = host.value.getBoundingClientRect();
  width.value = host.value.clientWidth || hostRect.width;
  if (viewport) {
    const viewportRect = viewport.getBoundingClientRect();
    const gridTop = hostRect.top - viewportRect.top + viewport.scrollTop;
    height.value = viewport.clientHeight || viewportRect.height;
    scrollTop.value = Math.max(0, viewport.scrollTop - gridTop);
  } else {
    height.value = window.innerHeight;
    scrollTop.value = Math.max(0, window.scrollY + hostRect.top * -1);
  }
}

function onScroll() {
  measure();
}

function focus() {
  host.value?.focus();
}

defineExpose({ focus });

watch(
  () => props.testViewport,
  () => measure(),
  { deep: true },
);

onMounted(() => {
  measure();
  if (props.testViewport) return;
  viewport = host.value?.closest("main") ?? null;
  (viewport ?? window).addEventListener("scroll", onScroll, { passive: true });
  window.addEventListener("resize", measure);
  if (typeof ResizeObserver !== "undefined" && host.value) {
    resizeObserver = new ResizeObserver(measure);
    resizeObserver.observe(host.value);
    if (viewport) resizeObserver.observe(viewport);
  }
});

onBeforeUnmount(() => {
  (viewport ?? window).removeEventListener("scroll", onScroll);
  window.removeEventListener("resize", measure);
  resizeObserver?.disconnect();
});
</script>

<template>
  <div ref="host" data-testid="library-virtual-grid" tabindex="-1">
    <div :style="{ height: `${topSpace}px` }" aria-hidden="true" />
    <div
      class="grid gap-4"
      :style="{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }"
    >
      <LibraryCard
        v-for="card in visibleCards"
        :key="card.id"
        :card="card"
        @select="(selected, origin) => emit('select', selected, origin)"
      />
    </div>
    <div :style="{ height: `${bottomSpace}px` }" aria-hidden="true" />
  </div>
</template>
