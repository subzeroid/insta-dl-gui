import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type {
  LibraryCard,
  LibraryItemDetail,
  LibraryPage,
  LibraryRoot,
  LibraryScanProgress,
} from "../lib/ipc";

const ipc = vi.hoisted(() => ({
  ensureConfiguredLibraryRoot: vi.fn(),
  listLibraryRoots: vi.fn(),
  startLibraryScan: vi.fn(),
  cancelLibraryScan: vi.fn(),
  queryLibrary: vi.fn(),
  getLibraryItem: vi.fn(),
  openLibraryFile: vi.fn(),
  revealLibraryFile: vi.fn(),
  onLibraryScanProgress: vi.fn(),
  libraryMediaUrl: vi.fn(),
  listener: undefined as ((progress: LibraryScanProgress) => void) | undefined,
}));

vi.mock("../lib/ipc", () => ({
  ensureConfiguredLibraryRoot: ipc.ensureConfiguredLibraryRoot,
  listLibraryRoots: ipc.listLibraryRoots,
  startLibraryScan: ipc.startLibraryScan,
  cancelLibraryScan: ipc.cancelLibraryScan,
  queryLibrary: ipc.queryLibrary,
  getLibraryItem: ipc.getLibraryItem,
  openLibraryFile: ipc.openLibraryFile,
  revealLibraryFile: ipc.revealLibraryFile,
  onLibraryScanProgress: ipc.onLibraryScanProgress,
  libraryMediaUrl: ipc.libraryMediaUrl,
}));

import { useLibraryStore, type LibraryCardView } from "./library";

const root: LibraryRoot = {
  id: 7,
  path: "/mock/archive",
  label: "Archive",
  created_at: 1_700_000_000,
  last_scan_started_at: null,
  last_scan_completed_at: null,
};

function card(id: number): LibraryCard {
  return {
    id,
    kind: "post",
    shortcode: `CODE${id}`,
    owner_username: "library_owner",
    taken_at: 1_700_000_000 + id,
    caption: `Card ${id}`,
    imported_at: 1_700_000_100 + id,
    updated_at: 1_700_000_200 + id,
    preview_file_id: 100 + id,
    resource_count: 1,
    availability: "available",
  };
}

function page(ids: number[], next_cursor: string | null): LibraryPage {
  return { items: ids.map(card), next_cursor };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  setActivePinia(createPinia());
  ipc.listener = undefined;
  for (const mock of [
    ipc.ensureConfiguredLibraryRoot,
    ipc.listLibraryRoots,
    ipc.startLibraryScan,
    ipc.cancelLibraryScan,
    ipc.queryLibrary,
    ipc.getLibraryItem,
    ipc.openLibraryFile,
    ipc.revealLibraryFile,
    ipc.onLibraryScanProgress,
    ipc.libraryMediaUrl,
  ]) {
    mock.mockReset();
  }
  ipc.ensureConfiguredLibraryRoot.mockResolvedValue(root);
  ipc.listLibraryRoots.mockResolvedValue([root]);
  ipc.startLibraryScan.mockResolvedValue("scan-7");
  ipc.cancelLibraryScan.mockResolvedValue(true);
  ipc.queryLibrary.mockResolvedValue(page([], null));
  ipc.getLibraryItem.mockResolvedValue({ id: 1 } as LibraryItemDetail);
  ipc.openLibraryFile.mockResolvedValue(undefined);
  ipc.revealLibraryFile.mockResolvedValue(undefined);
  ipc.onLibraryScanProgress.mockImplementation(
    async (listener: (progress: LibraryScanProgress) => void) => {
      ipc.listener = listener;
      return () => {};
    },
  );
  ipc.libraryMediaUrl.mockImplementation((fileId: number) =>
    `library://localhost/media/${fileId}`,
  );
});

describe("library query state", () => {
  it("replaces cards on the first page and stores the next cursor", async () => {
    ipc.queryLibrary.mockResolvedValueOnce(page([1, 2], "next-page"));
    const store = useLibraryStore();

    await store.refresh();

    expect(store.cards.map((item) => item.id)).toEqual([1, 2]);
    expect(store.cursor).toBe("next-page");
  });

  it("appends a later page without duplicating catalog IDs", async () => {
    ipc.queryLibrary
      .mockResolvedValueOnce(page([1, 2], "next-page"))
      .mockResolvedValueOnce(page([2, 3], null));
    const store = useLibraryStore();

    await store.refresh();
    await store.loadMore();

    expect(store.cards.map((item) => item.id)).toEqual([1, 2, 3]);
    expect(store.cursor).toBeNull();
    expect(ipc.queryLibrary).toHaveBeenLastCalledWith(
      expect.objectContaining({ cursor: "next-page" }),
    );
  });

  it("keeps only the first occurrence of duplicate IDs within a later page", async () => {
    ipc.queryLibrary
      .mockResolvedValueOnce(page([1], "next-page"))
      .mockResolvedValueOnce(page([2, 2, 3, 2], null));
    const store = useLibraryStore();

    await store.refresh();
    await store.loadMore();

    expect(store.cards.map((item) => item.id)).toEqual([1, 2, 3]);
  });

  it("clears pagination when search, filters, or sorting change", async () => {
    const store = useLibraryStore();

    store.cursor = "search-cursor";
    store.setSearch("  sunrise  ");
    expect(store.cursor).toBeNull();
    store.cursor = "kind-cursor";
    store.setKinds(["reel"]);
    expect(store.cursor).toBeNull();
    store.cursor = "availability-cursor";
    store.setAvailability("missing");
    expect(store.cursor).toBeNull();
    store.cursor = "sort-cursor";
    store.setSort("imported_at_desc");
    expect(store.cursor).toBeNull();
    store.cursor = "source-cursor";
    store.setSourceId(42);
    expect(store.cursor).toBeNull();
    store.cursor = "date-cursor";
    store.setDateRange(1_700_000_000, 1_700_086_400);
    expect(store.cursor).toBeNull();

    await store.refresh();

    expect(ipc.queryLibrary).toHaveBeenLastCalledWith({
      search: "sunrise",
      kinds: ["reel"],
      source_id: 42,
      availability: "missing",
      taken_after: 1_700_000_000,
      taken_before: 1_700_086_400,
      sort: "imported_at_desc",
      cursor: null,
      limit: 60,
    });
  });

  it("does not let a stale slower response overwrite a newer query", async () => {
    const older = deferred<LibraryPage>();
    const newer = deferred<LibraryPage>();
    ipc.queryLibrary
      .mockImplementationOnce(() => older.promise)
      .mockImplementationOnce(() => newer.promise);
    const store = useLibraryStore();

    const oldRequest = store.refresh();
    store.setSearch("new query");
    const newRequest = store.refresh();
    newer.resolve(page([9], "new-cursor"));
    await newRequest;
    older.resolve(page([1], "old-cursor"));
    await oldRequest;

    expect(store.cards.map((item) => item.id)).toEqual([9]);
    expect(store.cursor).toBe("new-cursor");
    expect(store.requestGeneration).toBeGreaterThanOrEqual(3);
  });

  it("refresh invalidates a pending append and leaves future pagination callable", async () => {
    const staleAppend = deferred<LibraryPage>();
    const replacement = deferred<LibraryPage>();
    ipc.queryLibrary
      .mockResolvedValueOnce(page([1], "append-cursor"))
      .mockImplementationOnce(() => staleAppend.promise)
      .mockImplementationOnce(() => replacement.promise)
      .mockResolvedValueOnce(page([10], null));
    const store = useLibraryStore();
    await store.refresh();

    const appendRequest = store.loadMore();
    expect(store.loadingMore).toBe(true);
    const refreshRequest = store.refresh();
    replacement.resolve(page([9], "future-cursor"));
    await refreshRequest;

    expect(store.cards.map((item) => item.id)).toEqual([9]);
    expect(store.loadingMore).toBe(false);

    staleAppend.resolve(page([2], null));
    await appendRequest;
    expect(store.cards.map((item) => item.id)).toEqual([9]);
    expect(store.loadingMore).toBe(false);

    await store.loadMore();
    expect(store.cards.map((item) => item.id)).toEqual([9, 10]);
    expect(ipc.queryLibrary).toHaveBeenCalledTimes(4);
  });

  it("adapts numeric preview IDs and missing files to presentation URLs", async () => {
    const missing = { ...card(2), availability: "missing" as const };
    ipc.queryLibrary.mockResolvedValueOnce({
      items: [card(1), missing],
      next_cursor: null,
    });
    const store = useLibraryStore();

    await store.refresh();

    const first: LibraryCardView = store.cards[0];
    expect(first.previewUrl).toBe("library://localhost/media/101");
    expect(store.cards[1].previewUrl).toBeNull();
    expect(ipc.libraryMediaUrl).toHaveBeenCalledTimes(1);
    expect(ipc.libraryMediaUrl).toHaveBeenCalledWith(101);
  });

  it("uses a self-contained mock preview without converting it as a path", async () => {
    const previewUrl = "data:image/svg+xml,%3Csvg%20xmlns='http://www.w3.org/2000/svg'/%3E";
    const mockCard = { ...card(3), preview_url: previewUrl };
    ipc.queryLibrary.mockResolvedValueOnce({ items: [mockCard], next_cursor: null });
    const store = useLibraryStore();

    await store.refresh();

    expect(store.cards[0].previewUrl).toBe(previewUrl);
    expect(ipc.libraryMediaUrl).not.toHaveBeenCalled();
  });
});

describe("library initialization lifecycle", () => {
  it("shares one subscription and initialization promise across concurrent callers", async () => {
    const registration = deferred<() => void>();
    const unlisten = vi.fn();
    ipc.onLibraryScanProgress.mockImplementationOnce(
      (listener: (progress: LibraryScanProgress) => void) => {
        ipc.listener = listener;
        return registration.promise;
      },
    );
    const store = useLibraryStore();

    const first = store.init();
    const second = store.init();
    let secondSettled = false;
    void second.then(() => {
      secondSettled = true;
    });
    await Promise.resolve();
    await Promise.resolve();

    expect(secondSettled).toBe(false);
    expect(ipc.onLibraryScanProgress).toHaveBeenCalledTimes(1);
    registration.resolve(unlisten);
    await Promise.all([first, second]);
    expect(ipc.ensureConfiguredLibraryRoot).toHaveBeenCalledTimes(1);
    expect(ipc.listLibraryRoots).toHaveBeenCalledTimes(1);
  });

  it("unsubscribes a late registration immediately after dispose", async () => {
    const registration = deferred<() => void>();
    const unlisten = vi.fn();
    ipc.onLibraryScanProgress.mockImplementationOnce(
      (listener: (progress: LibraryScanProgress) => void) => {
        ipc.listener = listener;
        return registration.promise;
      },
    );
    const store = useLibraryStore();

    const pendingInit = store.init();
    store.dispose();
    registration.resolve(unlisten);
    await pendingInit;

    expect(unlisten).toHaveBeenCalledTimes(1);
    expect(ipc.ensureConfiguredLibraryRoot).not.toHaveBeenCalled();

    await store.init();
    expect(ipc.onLibraryScanProgress).toHaveBeenCalledTimes(2);
    expect(ipc.ensureConfiguredLibraryRoot).toHaveBeenCalledTimes(1);
  });

  it("rolls back a rejected registration so initialization can retry", async () => {
    ipc.onLibraryScanProgress.mockRejectedValueOnce(new Error("listener unavailable"));
    const store = useLibraryStore();

    await expect(store.init()).rejects.toThrow("listener unavailable");
    await store.init();

    expect(ipc.onLibraryScanProgress).toHaveBeenCalledTimes(2);
    expect(ipc.ensureConfiguredLibraryRoot).toHaveBeenCalledTimes(1);
  });

  it("keeps the listener through an active scan terminal event before tearing down", async () => {
    const unlisten = vi.fn();
    ipc.onLibraryScanProgress.mockImplementationOnce(
      async (listener: (progress: LibraryScanProgress) => void) => {
        ipc.listener = listener;
        return unlisten;
      },
    );
    const store = useLibraryStore();
    await store.init();
    await store.startScan(root.id);

    store.dispose();
    expect(unlisten).not.toHaveBeenCalled();

    ipc.listener?.({
      state: "done",
      scan_id: "scan-7",
      root_id: root.id,
      summary: { imported: 5, updated: 1, missing: 0, warnings: 0 },
    });
    expect(store.scanActive).toBe(false);
    expect(store.scanSummary).toEqual({
      imported: 5,
      updated: 1,
      missing: 0,
      warnings: 0,
    });
    expect(unlisten).toHaveBeenCalledTimes(1);

    await store.init();
    expect(ipc.onLibraryScanProgress).toHaveBeenCalledTimes(2);
    await expect(store.startScan(root.id)).resolves.toBe("scan-7");
  });

  it("reuses the retained listener when initialized again before scan completion", async () => {
    const unlisten = vi.fn();
    ipc.onLibraryScanProgress.mockImplementationOnce(
      async (listener: (progress: LibraryScanProgress) => void) => {
        ipc.listener = listener;
        return unlisten;
      },
    );
    const store = useLibraryStore();
    await store.init();
    await store.startScan(root.id);

    store.dispose();
    await store.init();

    expect(ipc.onLibraryScanProgress).toHaveBeenCalledTimes(1);
    expect(unlisten).not.toHaveBeenCalled();
    ipc.listener?.({
      state: "done",
      scan_id: "scan-7",
      root_id: root.id,
      summary: { imported: 1, updated: 0, missing: 0, warnings: 0 },
    });
    expect(store.scanActive).toBe(false);
    expect(unlisten).not.toHaveBeenCalled();

    store.dispose();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("finishes deferred teardown when a pending scan start rejects", async () => {
    const unlisten = vi.fn();
    const started = deferred<string>();
    ipc.onLibraryScanProgress.mockImplementationOnce(
      async (listener: (progress: LibraryScanProgress) => void) => {
        ipc.listener = listener;
        return unlisten;
      },
    );
    ipc.startLibraryScan.mockImplementationOnce(() => started.promise);
    const store = useLibraryStore();
    await store.init();

    const pendingStart = store.startScan(root.id);
    store.dispose();
    expect(unlisten).not.toHaveBeenCalled();
    started.reject(new Error("start failed"));
    await expect(pendingStart).rejects.toThrow("start failed");

    expect(store.scanId).toBeNull();
    expect(store.scanActive).toBe(false);
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("starts only after the current listener wins an adversarial re-initialization", async () => {
    const registration1 = deferred<() => void>();
    const registration2 = deferred<() => void>();
    const unlisten1 = vi.fn();
    const unlisten2 = vi.fn();
    let listener2: ((progress: LibraryScanProgress) => void) | undefined;
    ipc.onLibraryScanProgress
      .mockImplementationOnce((_listener: (progress: LibraryScanProgress) => void) => {
        return registration1.promise;
      })
      .mockImplementationOnce((listener: (progress: LibraryScanProgress) => void) => {
        listener2 = listener;
        return registration2.promise;
      });
    const store = useLibraryStore();

    const staleInit = store.init();
    const pendingScan = store.startScan(root.id);
    store.dispose();
    const currentInit = store.init();

    expect(ipc.onLibraryScanProgress).toHaveBeenCalledTimes(2);
    expect(ipc.startLibraryScan).not.toHaveBeenCalled();
    registration2.resolve(unlisten2);
    await currentInit;
    expect(ipc.startLibraryScan).not.toHaveBeenCalled();

    registration1.resolve(unlisten1);
    await staleInit;
    await expect(pendingScan).resolves.toBe("scan-7");
    expect(ipc.startLibraryScan).toHaveBeenCalledTimes(1);
    expect(unlisten1).toHaveBeenCalledTimes(1);
    expect(unlisten2).not.toHaveBeenCalled();

    listener2?.({
      state: "done",
      scan_id: "scan-7",
      root_id: root.id,
      summary: { imported: 2, updated: 0, missing: 0, warnings: 0 },
    });
    expect(store.scanActive).toBe(false);
    expect(unlisten2).not.toHaveBeenCalled();
    store.dispose();
    expect(unlisten2).toHaveBeenCalledTimes(1);
  });

  it("does not call the scan backend when listener initialization rejects", async () => {
    ipc.onLibraryScanProgress.mockRejectedValueOnce(new Error("listener failed"));
    const store = useLibraryStore();

    await expect(store.startScan(root.id)).rejects.toThrow("listener failed");

    expect(ipc.startLibraryScan).not.toHaveBeenCalled();
    expect(store.scanId).toBeNull();
    expect(store.scanActive).toBe(false);
  });
});

describe("library scans and file actions", () => {
  it("updates progress and the final summary from scan events", async () => {
    const store = useLibraryStore();
    await store.init();
    await store.startScan(root.id);

    ipc.listener?.({
      state: "scanning",
      scan_id: "scan-7",
      root_id: root.id,
      discovered: 12,
      processed: 5,
      warnings: 1,
    });
    expect(store.scanProgress).toMatchObject({
      state: "scanning",
      discovered: 12,
      processed: 5,
      warnings: 1,
    });
    expect(store.scanActive).toBe(true);

    ipc.listener?.({
      state: "done",
      scan_id: "scan-7",
      root_id: root.id,
      summary: { imported: 8, updated: 2, missing: 1, warnings: 1 },
    });
    expect(store.scanSummary).toEqual({
      imported: 8,
      updated: 2,
      missing: 1,
      warnings: 1,
    });
    expect(store.scanActive).toBe(false);
  });

  it("retains events from a new scan that arrive before its command resolves", async () => {
    const nextScan = deferred<string>();
    ipc.startLibraryScan
      .mockResolvedValueOnce("old-scan")
      .mockImplementationOnce(() => nextScan.promise);
    const store = useLibraryStore();
    await store.init();
    await store.startScan(root.id);
    ipc.listener?.({
      state: "done",
      scan_id: "old-scan",
      root_id: root.id,
      summary: { imported: 1, updated: 0, missing: 0, warnings: 0 },
    });

    const pendingStart = store.startScan(root.id);
    expect(store.scanId).toBeNull();

    ipc.listener?.({
      state: "scanning",
      scan_id: "new-scan",
      root_id: root.id,
      discovered: 4,
      processed: 2,
      warnings: 0,
    });
    ipc.listener?.({
      state: "done",
      scan_id: "new-scan",
      root_id: root.id,
      summary: { imported: 3, updated: 1, missing: 0, warnings: 0 },
    });
    expect(store.scanProgress).toBeNull();
    expect(store.scanSummary).toBeNull();
    nextScan.resolve("new-scan");
    await pendingStart;

    expect(store.scanId).toBe("new-scan");
    expect(store.scanProgress).toMatchObject({ state: "done", scan_id: "new-scan" });
    expect(store.scanSummary).toEqual({
      imported: 3,
      updated: 1,
      missing: 0,
      warnings: 0,
    });
    expect(store.scanActive).toBe(false);
  });

  it("discards an unrelated same-root event buffered while a scan starts", async () => {
    const started = deferred<string>();
    ipc.startLibraryScan.mockImplementationOnce(() => started.promise);
    const store = useLibraryStore();
    await store.init();

    const pendingStart = store.startScan(root.id);
    ipc.listener?.({
      state: "done",
      scan_id: "foreign-scan",
      root_id: root.id,
      summary: { imported: 99, updated: 99, missing: 99, warnings: 99 },
    });
    started.resolve("expected-scan");
    await pendingStart;

    expect(store.scanId).toBe("expected-scan");
    expect(store.scanProgress).toBeNull();
    expect(store.scanSummary).toBeNull();
    expect(store.scanActive).toBe(true);
  });

  it("clears buffered state when starting a scan fails", async () => {
    const started = deferred<string>();
    ipc.startLibraryScan.mockImplementationOnce(() => started.promise);
    const store = useLibraryStore();
    await store.init();

    const pendingStart = store.startScan(root.id);
    ipc.listener?.({
      state: "scanning",
      scan_id: "foreign-scan",
      root_id: root.id,
      discovered: 4,
      processed: 1,
      warnings: 0,
    });
    started.reject(new Error("scan registration failed"));
    await expect(pendingStart).rejects.toThrow("scan registration failed");

    expect(store.scanId).toBeNull();
    expect(store.scanActive).toBe(false);
    expect(store.scanProgress).toBeNull();
    await expect(store.cancelScan()).resolves.toBe(false);
    expect(ipc.cancelLibraryScan).not.toHaveBeenCalled();
  });

  it("rejects a concurrent start without corrupting the pending scan", async () => {
    const started = deferred<string>();
    ipc.startLibraryScan.mockImplementationOnce(() => started.promise);
    const store = useLibraryStore();
    await store.init();

    const firstStart = store.startScan(root.id);
    await expect(store.startScan(root.id)).rejects.toThrow("already active");
    expect(ipc.startLibraryScan).toHaveBeenCalledTimes(1);

    started.resolve("first-scan");
    await expect(firstStart).resolves.toBe("first-scan");
    expect(store.scanId).toBe("first-scan");
    expect(store.scanActive).toBe(true);
  });

  it("passes numeric catalog IDs to open and reveal actions", async () => {
    const store = useLibraryStore();

    await store.openFile(101);
    await store.revealFile(202);

    expect(ipc.openLibraryFile).toHaveBeenCalledWith(101);
    expect(ipc.revealLibraryFile).toHaveBeenCalledWith(202);
    expect(typeof ipc.openLibraryFile.mock.calls[0][0]).toBe("number");
    expect(typeof ipc.revealLibraryFile.mock.calls[0][0]).toBe("number");
  });
});
