import { defineStore } from "pinia";
import { ref } from "vue";

import {
  cancelLibraryScan,
  ensureConfiguredLibraryRoot,
  getLibraryItem,
  libraryMediaUrl,
  listLibraryRoots,
  onLibraryScanProgress,
  openLibraryFile,
  queryLibrary,
  revealLibraryFile,
  startLibraryScan,
  type FileAvailability,
  type LibraryCard,
  type LibraryItemDetail,
  type LibraryQuery,
  type LibraryRoot,
  type LibraryScanProgress,
  type LibrarySort,
  type MediaItemKind,
  type ScanSummary,
} from "../lib/ipc";

const PAGE_SIZE = 60;

export interface LibraryCardView extends LibraryCard {
  previewUrl: string | null;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function toLibraryCardView(card: LibraryCard): LibraryCardView {
  const mockPreview = (card as LibraryCard & { preview_url?: unknown }).preview_url;
  const previewUrl =
    typeof mockPreview === "string" && mockPreview.startsWith("data:image/svg+xml,")
      ? mockPreview
      : card.availability === "missing" || card.preview_file_id === null
        ? null
        : libraryMediaUrl(card.preview_file_id);
  return {
    id: card.id,
    kind: card.kind,
    shortcode: card.shortcode,
    owner_username: card.owner_username,
    taken_at: card.taken_at,
    caption: card.caption,
    imported_at: card.imported_at,
    updated_at: card.updated_at,
    preview_file_id: card.preview_file_id,
    resource_count: card.resource_count,
    availability: card.availability,
    previewUrl,
  };
}

export const useLibraryStore = defineStore("library", () => {
  const search = ref("");
  const kinds = ref<MediaItemKind[]>([]);
  const sourceId = ref<number | null>(null);
  const availability = ref<FileAvailability | null>(null);
  const takenAfter = ref<number | null>(null);
  const takenBefore = ref<number | null>(null);
  const sort = ref<LibrarySort>("taken_at_desc");

  const cards = ref<LibraryCardView[]>([]);
  const cursor = ref<string | null>(null);
  const loading = ref(false);
  const loadingMore = ref(false);
  const error = ref<string | null>(null);
  const requestGeneration = ref(0);

  const selected = ref<LibraryItemDetail | null>(null);
  const detailLoading = ref(false);
  const detailError = ref<string | null>(null);
  let detailGeneration = 0;

  const roots = ref<LibraryRoot[]>([]);
  const activeRoot = ref<LibraryRoot | null>(null);
  const rootsLoading = ref(false);
  const rootsError = ref<string | null>(null);
  const scanId = ref<string | null>(null);
  const scanActive = ref(false);
  const scanProgress = ref<LibraryScanProgress | null>(null);
  const scanSummary = ref<ScanSummary | null>(null);
  const scanError = ref<string | null>(null);
  let initialized = false;
  let initPromise: Promise<void> | null = null;
  let lifecycleGeneration = 0;
  let rootsRequestGeneration = 0;
  let unlistenScan: (() => void) | null = null;
  let deferredListenerTeardown = false;
  let pendingScanRootId: number | null = null;
  const earlyScanEvents = new Map<string, LibraryScanProgress[]>();

  function resetPagination() {
    cursor.value = null;
    requestGeneration.value += 1;
    loading.value = false;
    loadingMore.value = false;
  }

  function setSearch(value: string) {
    search.value = value;
    resetPagination();
  }

  function setKinds(value: MediaItemKind[]) {
    kinds.value = [...value];
    resetPagination();
  }

  function setSourceId(value: number | null) {
    sourceId.value = value;
    resetPagination();
  }

  function setAvailability(value: FileAvailability | null) {
    availability.value = value;
    resetPagination();
  }

  function setDateRange(after: number | null, before: number | null) {
    takenAfter.value = after;
    takenBefore.value = before;
    resetPagination();
  }

  function setSort(value: LibrarySort) {
    sort.value = value;
    resetPagination();
  }

  function buildQuery(pageCursor: string | null): LibraryQuery {
    const normalizedSearch = search.value.trim();
    return {
      search: normalizedSearch || null,
      kinds: [...kinds.value],
      source_id: sourceId.value,
      availability: availability.value,
      taken_after: takenAfter.value,
      taken_before: takenBefore.value,
      sort: sort.value,
      cursor: pageCursor,
      limit: PAGE_SIZE,
    };
  }

  async function loadPage(append: boolean) {
    if (append && (cursor.value === null || loadingMore.value)) return;
    const pageCursor = append ? cursor.value : null;
    const generation = requestGeneration.value + 1;
    requestGeneration.value = generation;
    error.value = null;
    if (append) loadingMore.value = true;
    else loading.value = true;

    try {
      const page = await queryLibrary(buildQuery(pageCursor));
      if (generation !== requestGeneration.value) return;
      const incomingCards = page.items.map(toLibraryCardView);
      if (append) {
        const seenIds = new Set(cards.value.map((card) => card.id));
        cards.value = [
          ...cards.value,
          ...incomingCards.filter((card) => {
            if (seenIds.has(card.id)) return false;
            seenIds.add(card.id);
            return true;
          }),
        ];
      } else {
        cards.value = incomingCards;
      }
      cursor.value = page.next_cursor;
    } catch (cause) {
      if (generation === requestGeneration.value) error.value = errorMessage(cause);
    } finally {
      if (generation === requestGeneration.value) {
        if (append) loadingMore.value = false;
        else loading.value = false;
      }
    }
  }

  async function refresh() {
    resetPagination();
    await loadPage(false);
  }

  async function loadMore() {
    await loadPage(true);
  }

  async function selectItem(id: number) {
    const generation = ++detailGeneration;
    detailLoading.value = true;
    detailError.value = null;
    try {
      const detail = await getLibraryItem(id);
      if (generation === detailGeneration) selected.value = detail;
    } catch (cause) {
      if (generation === detailGeneration) detailError.value = errorMessage(cause);
    } finally {
      if (generation === detailGeneration) detailLoading.value = false;
    }
  }

  function clearSelection() {
    detailGeneration += 1;
    selected.value = null;
    detailLoading.value = false;
    detailError.value = null;
  }

  async function loadRoots(expectedGeneration = lifecycleGeneration) {
    const requestGeneration = ++rootsRequestGeneration;
    const isCurrent = () =>
      expectedGeneration === lifecycleGeneration &&
      requestGeneration === rootsRequestGeneration;
    rootsLoading.value = true;
    rootsError.value = null;
    try {
      const configured = await ensureConfiguredLibraryRoot();
      if (!isCurrent()) return;
      const listed = await listLibraryRoots();
      if (!isCurrent()) return;
      roots.value = listed.some((root) => root.id === configured.id)
        ? listed
        : [...listed, configured];
      activeRoot.value = roots.value.find((root) => root.id === configured.id) ?? configured;
    } catch (cause) {
      if (isCurrent()) rootsError.value = errorMessage(cause);
    } finally {
      if (isCurrent()) rootsLoading.value = false;
    }
  }

  function applyScanProgress(progress: LibraryScanProgress) {
    if (pendingScanRootId !== null) {
      if (progress.root_id === pendingScanRootId) {
        const buffered = earlyScanEvents.get(progress.scan_id) ?? [];
        buffered.push(progress);
        earlyScanEvents.set(progress.scan_id, buffered);
      }
      return;
    }
    if (scanId.value === null || progress.scan_id !== scanId.value) return;
    scanProgress.value = progress;
    scanActive.value = progress.state === "scanning";
    if (progress.state === "done" || progress.state === "cancelled") {
      scanSummary.value = progress.summary;
    }
    if (progress.state === "failed") scanError.value = progress.error;
    if (progress.state !== "scanning") completeDeferredListenerTeardown();
  }

  function trackInitPromise(pending: Promise<void>): Promise<void> {
    initPromise = pending;
    void pending.then(
      () => {
        if (initPromise === pending) initPromise = null;
      },
      () => {
        if (initPromise === pending) initPromise = null;
      },
    );
    return pending;
  }

  function teardownListener() {
    const unlisten = unlistenScan;
    unlistenScan = null;
    deferredListenerTeardown = false;
    initialized = false;
    initPromise = null;
    unlisten?.();
  }

  function completeDeferredListenerTeardown() {
    if (deferredListenerTeardown) teardownListener();
  }

  function init(): Promise<void> {
    deferredListenerTeardown = false;
    if (initialized) return Promise.resolve();
    if (initPromise !== null) return initPromise;
    if (unlistenScan !== null) {
      const generation = lifecycleGeneration;
      const registeredUnlisten = unlistenScan;
      return trackInitPromise(
        (async () => {
          await loadRoots(generation);
          if (
            generation === lifecycleGeneration &&
            unlistenScan === registeredUnlisten
          ) {
            initialized = true;
          }
        })(),
      );
    }
    const generation = lifecycleGeneration;
    const pending = (async () => {
      let registeredUnlisten: (() => void) | null = null;
      try {
        registeredUnlisten = await onLibraryScanProgress(applyScanProgress);
        if (generation !== lifecycleGeneration) {
          registeredUnlisten();
          return;
        }
        unlistenScan = registeredUnlisten;
        await loadRoots(generation);
        if (
          generation !== lifecycleGeneration ||
          unlistenScan !== registeredUnlisten
        ) {
          if (unlistenScan === registeredUnlisten) {
            unlistenScan = null;
          }
          registeredUnlisten();
          return;
        }
        initialized = true;
      } catch (cause) {
        if (generation === lifecycleGeneration) {
          initialized = false;
          if (registeredUnlisten !== null && unlistenScan === registeredUnlisten) {
            registeredUnlisten();
            unlistenScan = null;
          }
        }
        throw cause;
      }
    })();
    return trackInitPromise(pending);
  }

  function dispose() {
    lifecycleGeneration += 1;
    initPromise = null;
    initialized = false;
    rootsLoading.value = false;
    if (scanActive.value || pendingScanRootId !== null) {
      deferredListenerTeardown = true;
      return;
    }
    teardownListener();
  }

  async function startScan(rootId: number) {
    if (!initialized || unlistenScan === null) await init();
    if (!initialized || unlistenScan === null) {
      throw new Error("Library scan listener is not initialized");
    }
    if (pendingScanRootId !== null || scanActive.value) {
      throw new Error("Library scan is already active");
    }
    pendingScanRootId = rootId;
    earlyScanEvents.clear();
    scanId.value = null;
    scanProgress.value = null;
    scanSummary.value = null;
    scanError.value = null;
    scanActive.value = true;
    try {
      const startedScanId = await startLibraryScan(rootId);
      scanId.value = startedScanId;
      const buffered = earlyScanEvents.get(startedScanId) ?? [];
      pendingScanRootId = null;
      earlyScanEvents.clear();
      for (const progress of buffered) applyScanProgress(progress);
      return startedScanId;
    } catch (cause) {
      scanId.value = null;
      scanActive.value = false;
      scanProgress.value = null;
      scanSummary.value = null;
      scanError.value = errorMessage(cause);
      completeDeferredListenerTeardown();
      throw cause;
    } finally {
      pendingScanRootId = null;
      earlyScanEvents.clear();
    }
  }

  async function cancelScan() {
    if (scanId.value === null) return false;
    return cancelLibraryScan(scanId.value);
  }

  async function openFile(fileId: number) {
    await openLibraryFile(fileId);
  }

  async function revealFile(fileId: number) {
    await revealLibraryFile(fileId);
  }

  return {
    search,
    kinds,
    sourceId,
    availability,
    takenAfter,
    takenBefore,
    sort,
    cards,
    cursor,
    loading,
    loadingMore,
    error,
    requestGeneration,
    selected,
    detailLoading,
    detailError,
    roots,
    activeRoot,
    rootsLoading,
    rootsError,
    scanId,
    scanActive,
    scanProgress,
    scanSummary,
    scanError,
    init,
    dispose,
    refresh,
    loadMore,
    selectItem,
    clearSelection,
    setSearch,
    setKinds,
    setSourceId,
    setAvailability,
    setDateRange,
    setSort,
    loadRoots,
    startScan,
    cancelScan,
    openFile,
    revealFile,
  };
});
