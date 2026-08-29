<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";

import LibraryDetail from "../components/LibraryDetail.vue";
import LibraryGrid, { type LibraryViewport } from "../components/LibraryGrid.vue";
import type { FileAvailability, LibraryScanProgress, LibrarySort, MediaItemKind } from "../lib/ipc";
import { useLibraryStore, type LibraryCardView } from "../stores/library";

defineProps<{ testViewport?: LibraryViewport }>();

const library = useLibraryStore();
const initialized = ref(false);
const initError = ref<string | null>(null);
const searchDraft = ref(library.search);
const availabilityDraft = ref<FileAvailability | "">(library.availability ?? "");
const sortDraft = ref<LibrarySort>(library.sort);
const afterDraft = ref(formatLocalDate(library.takenAfter));
const beforeDraft = ref(formatLocalDate(library.takenBefore));
const dateError = ref<string | null>(null);
const loadMoreSentinel = ref<HTMLElement | null>(null);
const backgroundContent = ref<HTMLElement | null>(null);
const libraryGrid = ref<{ focus: () => void } | null>(null);
let loadMoreObserver: IntersectionObserver | null = null;
let searchTimer: ReturnType<typeof setTimeout> | null = null;
let selectionOrigin: HTMLElement | null = null;
let disposed = false;
let lifecycleGeneration = 0;
let backgroundWasInert = false;
let backgroundInertApplied = false;
const refreshedScans = new Set<string>();

const hasCompletedScan = computed(
  () =>
    library.activeRoot !== null &&
    (library.activeRoot.last_scan_completed_at !== null || library.scanSummary !== null),
);
const firstVisit = computed(
  () =>
    initialized.value &&
    library.activeRoot !== null &&
    !hasCompletedScan.value &&
    !library.scanActive &&
    !library.loading &&
    library.cards.length === 0,
);
const hasActiveFilters = computed(
  () =>
    library.search.trim().length > 0 ||
    library.kinds.length > 0 ||
    library.sourceId !== null ||
    library.availability !== null ||
    library.takenAfter !== null ||
    library.takenBefore !== null,
);
const emptyAfterScan = computed(
  () =>
    initialized.value &&
    hasCompletedScan.value &&
    !library.loading &&
    library.cards.length === 0 &&
    library.error === null &&
    !hasActiveFilters.value,
);
const emptyFilteredResults = computed(
  () =>
    initialized.value &&
    hasCompletedScan.value &&
    !library.loading &&
    library.cards.length === 0 &&
    library.error === null &&
    hasActiveFilters.value,
);

const scanningProgress = computed(() =>
  library.scanProgress?.state === "scanning" ? library.scanProgress : null,
);

const kindFilters: Array<{ label: string; value: MediaItemKind }> = [
  { label: "Posts", value: "post" },
  { label: "Reels", value: "reel" },
  { label: "Stories", value: "story" },
  { label: "Avatars", value: "avatar" },
];

function message(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

async function initialize() {
  try {
    await library.init();
    if (disposed) return;
    await library.refresh();
    if (disposed) return;
    initialized.value = true;
  } catch (cause) {
    if (disposed) return;
    initError.value = message(cause);
  }
}

async function startScan() {
  if (!library.activeRoot || library.scanActive) return;
  try {
    await library.startScan(library.activeRoot.id);
  } catch {
    // The store exposes a sanitized scanError for the inline status panel.
  }
}

async function cancelScan() {
  try {
    await library.cancelScan();
  } catch {
    // The running scan remains visible and may still emit its terminal event.
  }
}

async function retryPreviewAccess() {
  await library.retryPreviewAccess();
}

function toggleKind(kind: MediaItemKind) {
  const next = library.kinds.includes(kind)
    ? library.kinds.filter((candidate) => candidate !== kind)
    : [...library.kinds, kind];
  library.setKinds(next);
  void library.refresh();
}

function applyAvailability() {
  library.setAvailability(availabilityDraft.value || null);
  void library.refresh();
}

function applySort() {
  library.setSort(sortDraft.value);
  void library.refresh();
}

function clearFilters() {
  if (searchTimer !== null) {
    clearTimeout(searchTimer);
    searchTimer = null;
  }
  library.setSearch("");
  library.setKinds([]);
  library.setSourceId(null);
  library.setAvailability(null);
  library.setDateRange(null, null);
  searchDraft.value = "";
  availabilityDraft.value = "";
  afterDraft.value = "";
  beforeDraft.value = "";
  dateError.value = null;
  void library.refresh();
}

function formatLocalDate(timestamp: number | null) {
  if (timestamp === null) return "";
  const date = new Date(timestamp * 1_000);
  const year = String(date.getFullYear()).padStart(4, "0");
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function parseLocalDate(value: string, endOfDay: boolean) {
  if (!value) return { timestamp: null, valid: true };
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!match) return { timestamp: null, valid: false };
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(
    year,
    month - 1,
    day,
    endOfDay ? 23 : 0,
    endOfDay ? 59 : 0,
    endOfDay ? 59 : 0,
    endOfDay ? 999 : 0,
  );
  const valid =
    date.getFullYear() === year &&
    date.getMonth() === month - 1 &&
    date.getDate() === day;
  return {
    timestamp: valid ? Math.floor(date.getTime() / 1_000) : null,
    valid,
  };
}

function applyDateRange() {
  const after = parseLocalDate(afterDraft.value, false);
  const before = parseLocalDate(beforeDraft.value, true);
  if (!after.valid || !before.valid) {
    dateError.value = "Enter a valid calendar date.";
    return;
  }
  if (
    after.timestamp !== null &&
    before.timestamp !== null &&
    after.timestamp > before.timestamp
  ) {
    dateError.value = "The start date must be before the end date.";
    return;
  }
  dateError.value = null;
  library.setDateRange(after.timestamp, before.timestamp);
  void library.refresh();
}

async function selectCard(card: LibraryCardView, origin: HTMLElement) {
  selectionOrigin = origin;
  await library.selectItem(card.id);
  if (!disposed && library.detailError !== null) {
    if (origin.isConnected) origin.focus();
    else libraryGrid.value?.focus();
    selectionOrigin = null;
  }
}

function setBackgroundInert(active: boolean) {
  const background = backgroundContent.value;
  if (!background) return;
  if (active && !backgroundInertApplied) {
    backgroundWasInert = background.hasAttribute("inert");
    background.setAttribute("inert", "");
    backgroundInertApplied = true;
  } else if (!active && backgroundInertApplied) {
    if (!backgroundWasInert) background.removeAttribute("inert");
    backgroundInertApplied = false;
  }
}

async function closeDetail() {
  library.clearSelection();
  setBackgroundInert(false);
  await nextTick();
  if (selectionOrigin?.isConnected) selectionOrigin.focus();
  else libraryGrid.value?.focus();
  selectionOrigin = null;
}

function setupLoadMoreObserver() {
  loadMoreObserver?.disconnect();
  loadMoreObserver = null;
  if (!loadMoreSentinel.value || typeof IntersectionObserver === "undefined") return;
  loadMoreObserver = new IntersectionObserver(
    (entries) => {
      if (!entries.some((entry) => entry.isIntersecting)) return;
      if (library.cursor === null || library.loadingMore) return;
      void library.loadMore();
    },
    {
      root: loadMoreSentinel.value.closest("main"),
      rootMargin: "500px 0px",
    },
  );
  loadMoreObserver.observe(loadMoreSentinel.value);
}

watch(searchDraft, (value) => {
  if (value.trim() === library.search.trim()) return;
  if (searchTimer !== null) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    searchTimer = null;
    library.setSearch(value);
    void library.refresh();
  }, 250);
});

watch(loadMoreSentinel, () => setupLoadMoreObserver());

watch(
  () => library.selected,
  async (selected) => {
    await nextTick();
    if (!disposed) setBackgroundInert(selected !== null);
  },
);

watch(
  () => library.scanProgress,
  async (progress: LibraryScanProgress | null) => {
    if (!progress || progress.state === "scanning" || progress.state === "failed") return;
    if (refreshedScans.has(progress.scan_id)) return;
    refreshedScans.add(progress.scan_id);
    const generation = lifecycleGeneration;
    await library.loadRoots();
    if (disposed || generation !== lifecycleGeneration) return;
    await library.refresh();
    if (disposed || generation !== lifecycleGeneration) return;
  },
);

onMounted(async () => {
  await initialize();
});

onBeforeUnmount(() => {
  disposed = true;
  lifecycleGeneration += 1;
  if (searchTimer !== null) clearTimeout(searchTimer);
  loadMoreObserver?.disconnect();
  setBackgroundInert(false);
  library.clearSelection();
  library.dispose();
});
</script>

<template>
  <div>
    <div ref="backgroundContent" data-testid="library-background" class="mx-auto max-w-7xl space-y-5 p-6">
    <header class="flex flex-wrap items-start justify-between gap-4">
      <div>
        <h1 class="text-xl font-semibold text-slate-100">Library</h1>
        <p class="mt-1 text-sm text-slate-500">Browse media stored in your configured download folder.</p>
      </div>
      <button
        v-if="initialized && library.activeRoot && (hasCompletedScan || library.cards.length > 0)"
        type="button"
        class="btn-secondary"
        data-action="scan"
        :disabled="library.scanActive"
        @click="startScan"
      >
        {{ library.scanActive ? "Scanning…" : "Rescan library" }}
      </button>
    </header>

    <p
      v-if="initError || library.rootsError"
      role="alert"
      class="rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-sm text-err"
    >
      {{ initError || library.rootsError }}
    </p>

    <section
      v-if="library.scanActive || library.scanSummary || library.scanError"
      class="card flex flex-wrap items-center justify-between gap-4 p-4"
      aria-live="polite"
    >
      <div v-if="scanningProgress" class="flex flex-wrap gap-x-5 gap-y-1 text-sm text-slate-300">
        <span>{{ scanningProgress.processed }} processed</span>
        <span>{{ scanningProgress.discovered }} discovered</span>
        <span :class="scanningProgress.warnings > 0 ? 'text-warn' : ''">
          {{ scanningProgress.warnings }} warnings
        </span>
      </div>
      <p v-else-if="library.scanActive" class="text-sm text-slate-400">Starting scan…</p>
      <div v-else-if="library.scanSummary" class="flex flex-wrap gap-x-5 gap-y-1 text-sm text-slate-300">
        <span>{{ library.scanSummary.imported }} imported</span>
        <span>{{ library.scanSummary.updated }} updated</span>
        <span>{{ library.scanSummary.missing }} missing</span>
        <span :class="library.scanSummary.warnings > 0 ? 'text-warn' : ''">
          {{ library.scanSummary.warnings }} warnings
        </span>
      </div>
      <p v-if="library.scanError" role="alert" class="text-sm text-err">{{ library.scanError }}</p>
      <button
        v-if="library.scanActive && library.scanId"
        type="button"
        class="btn-secondary"
        data-action="cancel-scan"
        @click="cancelScan"
      >
        Cancel scan
      </button>
    </section>

    <section v-if="firstVisit" class="card flex flex-col items-center px-6 py-14 text-center">
      <h2 class="text-lg font-semibold text-slate-100">Bring your local archive into view</h2>
      <p class="mt-2 max-w-lg text-sm leading-6 text-slate-500">
        Scan the configured download folder to index existing media. Files stay where they are.
      </p>
      <div class="mt-5 flex flex-wrap justify-center gap-3">
        <button type="button" class="btn-primary" data-action="scan" @click="startScan">
          Scan library
        </button>
        <RouterLink to="/download" class="btn-secondary">Go to Download</RouterLink>
      </div>
    </section>

    <template v-else-if="initialized && (hasCompletedScan || library.cards.length > 0)">
      <section class="card space-y-4 p-4" aria-label="Library filters">
        <div class="grid gap-3 lg:grid-cols-[minmax(220px,2fr)_repeat(3,minmax(150px,1fr))]">
          <label class="library-control-label">
            <span>Search</span>
            <input
              v-model="searchDraft"
              class="input"
              type="search"
              aria-label="Search library"
              placeholder="Username, shortcode, caption"
            />
          </label>
          <label class="library-control-label">
            <span>Source</span>
            <select class="input" aria-label="Source" disabled value="all">
              <option value="all">All sources</option>
            </select>
          </label>
          <label class="library-control-label">
            <span>Files</span>
            <select
              v-model="availabilityDraft"
              class="input"
              aria-label="File availability"
              @change="applyAvailability"
            >
              <option value="">All files</option>
              <option value="available">Available</option>
              <option value="missing">Missing</option>
            </select>
          </label>
          <label class="library-control-label">
            <span>Sort</span>
            <select v-model="sortDraft" class="input" aria-label="Sort library" @change="applySort">
              <option value="taken_at_desc">Publication date</option>
              <option value="imported_at_desc">Import date</option>
            </select>
          </label>
        </div>
        <div class="flex flex-wrap items-end gap-3">
          <fieldset class="flex flex-wrap gap-2">
            <legend class="mb-1 text-xs font-medium text-slate-500">Media type</legend>
            <button
              v-for="filter in kindFilters"
              :key="filter.value"
              type="button"
              class="rounded-full border px-3 py-1.5 text-xs font-medium transition-colors"
              :class="
                library.kinds.includes(filter.value)
                  ? 'border-accent bg-accent/15 text-slate-100'
                  : 'border-line bg-surface-2 text-slate-400 hover:text-slate-200'
              "
              :aria-label="`Filter ${filter.label}`"
              :aria-pressed="library.kinds.includes(filter.value)"
              @click="toggleKind(filter.value)"
            >
              {{ filter.label }}
            </button>
          </fieldset>
          <label class="library-control-label w-40">
            <span>Taken after</span>
            <input
              v-model="afterDraft"
              class="input"
              type="date"
              aria-label="Taken after"
              @change="applyDateRange"
            />
          </label>
          <label class="library-control-label w-40">
            <span>Taken before</span>
            <input
              v-model="beforeDraft"
              class="input"
              type="date"
              aria-label="Taken before"
              @change="applyDateRange"
            />
          </label>
          <p
            v-if="dateError"
            data-testid="library-date-error"
            class="self-end pb-2 text-xs text-err"
          >
            {{ dateError }}
          </p>
        </div>
      </section>

      <section
        v-if="library.previewAccess === 'denied'"
        data-testid="library-preview-access-notice"
        class="card flex flex-wrap items-center justify-between gap-4 border-warn/40 bg-warn/10 p-4"
        role="status"
        aria-live="polite"
      >
        <div>
          <p class="text-sm font-medium text-slate-200">Preview access is blocked</p>
          <p class="mt-1 text-xs text-slate-400">
            Enable insta-dl-gui in System Settings → Privacy &amp; Security → Files and Folders,
            then retry. Your files stay local.
          </p>
        </div>
        <button
          type="button"
          class="btn-secondary"
          data-action="retry-library-previews"
          @click="retryPreviewAccess"
        >
          Retry previews
        </button>
      </section>

      <p
        v-if="library.error"
        role="alert"
        class="rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-sm text-err"
      >
        {{ library.error }}
      </p>
      <p
        v-if="library.detailLoading"
        data-testid="library-detail-loading"
        class="py-3 text-center text-sm text-slate-500"
        aria-live="polite"
      >
        Loading media details…
      </p>
      <p
        v-if="library.detailError"
        data-testid="library-detail-error"
        role="alert"
        class="rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-sm text-err"
      >
        {{ library.detailError }}
      </p>
      <p v-if="library.loading" class="py-10 text-center text-sm text-slate-500">Loading library…</p>
      <LibraryGrid
        v-else-if="library.cards.length > 0"
        ref="libraryGrid"
        :cards="library.cards"
        :test-viewport="testViewport"
        @select="selectCard"
      />
      <section v-else-if="emptyFilteredResults" class="card flex flex-col items-center px-6 py-14 text-center">
        <h2 class="text-lg font-semibold text-slate-100">No matches</h2>
        <p class="mt-2 max-w-xl text-sm leading-6 text-slate-500">
          Try a different search or clear the active filters.
        </p>
        <button
          type="button"
          class="btn-secondary mt-5"
          data-action="clear-library-filters"
          @click="clearFilters"
        >
          Clear filters
        </button>
      </section>
      <section v-else-if="emptyAfterScan" class="card flex flex-col items-center px-6 py-14 text-center">
        <h2 class="text-lg font-semibold text-slate-100">No media in your local archive yet</h2>
        <p class="mt-2 max-w-xl text-sm leading-6 text-slate-500">
          Downloaded posts, reels, stories, and avatars will appear here after a library scan.
        </p>
        <RouterLink to="/download" class="btn-primary mt-5">Download media</RouterLink>
      </section>

      <div
        ref="loadMoreSentinel"
        data-testid="library-load-more-sentinel"
        class="flex h-10 items-center justify-center text-xs text-slate-500"
        aria-live="polite"
      >
        <span v-if="library.loadingMore">Loading more…</span>
      </div>
    </template>
    </div>

    <LibraryDetail v-if="library.selected" :detail="library.selected" @close="closeDetail" />
  </div>
</template>
