/** @vitest-environment happy-dom */

import { afterEach, describe, expect, it } from "vitest";
import {
  cancelLibraryScan,
  ensureConfiguredLibraryRoot,
  getLibraryItem,
  libraryMediaUrl,
  listLibraryRoots,
  onLibraryScanProgress,
  openLibraryFile,
  queryLibrary,
  requestLibraryPreviewAccess,
  revealLibraryFile,
  startLibraryScan,
  type LibraryCard,
  type LibraryPage,
  type LibraryQuery,
  type LibraryScanProgress,
  type Post,
  type ProfilePreview,
} from "./ipc";
import { installTauriMock } from "./mock";

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function invoke(): Invoke {
  return (window as unknown as { __TAURI_INTERNALS__: { invoke: Invoke } }).__TAURI_INTERNALS__.invoke;
}

afterEach(() => {
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  delete (window as unknown as { __TAURI_EVENT_PLUGIN_INTERNALS__?: unknown })
    .__TAURI_EVENT_PLUGIN_INTERNALS__;
  window.history.replaceState({}, "", "/");
});

describe("profile pagination mock", () => {
  it("returns a distinct final page for the supplied end cursor", async () => {
    installTauriMock();
    const first = (await invoke()("fetch_profile", {
      username: "instagram",
      endCursor: null,
    })) as ProfilePreview;
    const second = (await invoke()("fetch_profile", {
      username: "instagram",
      endCursor: first.end_cursor,
    })) as ProfilePreview;

    const allIds = [...first.recent_posts, ...second.recent_posts].map((post) => post.pk);
    expect(first.end_cursor).toBe("cursor");
    expect(second.end_cursor).toBeNull();
    expect(first.recent_posts).toHaveLength(12);
    expect(second.recent_posts).toHaveLength(12);
    expect(new Set(allIds).size).toBe(24);
  });

  it("returns cursor-paged reels independently from profile posts", async () => {
    installTauriMock();
    const first = (await invoke()("fetch_reels", {
      userId: "42",
      endCursor: null,
    })) as { posts: Post[]; end_cursor: string | null };
    const second = (await invoke()("fetch_reels", {
      userId: "42",
      endCursor: first.end_cursor,
    })) as { posts: Post[]; end_cursor: string | null };

    expect(first.posts).toHaveLength(11);
    expect(
      first.posts.every((post) =>
        post.resources.some((resource) => resource.kind === "video"),
      ),
    ).toBe(true);
    expect(first.end_cursor).toBe("reels-cursor");
    expect(second.end_cursor).toBeNull();
  });
});

describe("library mock", () => {
  type MockLibraryCard = LibraryCard & { preview_url: string };
  type MockLibraryPage = Omit<LibraryPage, "items"> & { items: MockLibraryCard[] };

  const query: LibraryQuery = {
    search: null,
    kinds: [],
    source_id: null,
    availability: null,
    taken_after: null,
    taken_before: null,
    sort: "taken_at_desc",
    cursor: null,
    limit: 60,
  };

  it("implements every library command with deterministic fixtures", async () => {
    installTauriMock();

    const configured = await ensureConfiguredLibraryRoot();
    const roots = await listLibraryRoots();
    const page = await queryLibrary(query);
    const mockPage = (await invoke()("query_library", { query })) as MockLibraryPage;
    const detail = await getLibraryItem(page.items[0].id);

    await expect(openLibraryFile(detail.files[0].id)).resolves.toBeNull();
    await expect(revealLibraryFile(detail.files[0].id)).resolves.toBeNull();
    await expect(requestLibraryPreviewAccess(detail.files[0].id)).resolves.toBe(true);
    await expect(cancelLibraryScan("mock-library-scan")).resolves.toBe(true);

    expect(roots).toEqual([configured]);
    expect(page.items).toHaveLength(4);
    expect(mockPage.items.map((item) => item.preview_url)).toSatisfy(
      (previews: string[]) =>
        previews.every((preview) => preview.startsWith("data:image/svg+xml,")),
    );
    expect(page.items).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: 1, resource_count: 1, availability: "available" }),
        expect.objectContaining({ id: 2, kind: "reel", availability: "available" }),
        expect.objectContaining({ id: 3, resource_count: 3, availability: "available" }),
        expect.objectContaining({ id: 4, availability: "missing" }),
      ]),
    );
    expect(detail.id).toBe(page.items[0].id);
    expect(libraryMediaUrl(42)).toBe("library://localhost/media/42");
  });

  it("builds a canonical custom-protocol URL without encoding path separators", () => {
    installTauriMock();
    const internals = (window as unknown as {
      __TAURI_INTERNALS__: { convertFileSrc: (path: string, protocol?: string) => string };
    }).__TAURI_INTERNALS__;
    internals.convertFileSrc = (path, protocol = "asset") =>
      `${protocol}://localhost/${encodeURIComponent(path)}`;

    const url = libraryMediaUrl(42);

    expect(url).toBe("library://localhost/media/42");
    expect(url).not.toContain("%2F");
  });

  it("provides an unscanned empty root for the first-scan library demo", async () => {
    window.history.replaceState({}, "", "/library?mock=1&demo=library-first-scan");
    installTauriMock();

    const configured = await ensureConfiguredLibraryRoot();
    const roots = await listLibraryRoots();
    const page = await queryLibrary(query);

    expect(configured).toEqual(
      expect.objectContaining({
        last_scan_started_at: null,
        last_scan_completed_at: null,
      }),
    );
    expect(roots).toEqual([configured]);
    expect(page).toEqual({ items: [], next_cursor: null });
  });

  it("emits the same deterministic scan result after starting a scan", async () => {
    installTauriMock();
    const events: LibraryScanProgress[] = [];
    const unlisten = await onLibraryScanProgress((event) => events.push(event));

    await expect(startLibraryScan(1)).resolves.toBe("mock-library-scan");
    await Promise.resolve();
    await Promise.resolve();

    expect(events).toEqual([
      {
        state: "scanning",
        scan_id: "mock-library-scan",
        root_id: 1,
        discovered: 4,
        processed: 2,
        warnings: 0,
      },
      {
        state: "done",
        scan_id: "mock-library-scan",
        root_id: 1,
        summary: { imported: 4, updated: 0, missing: 1, warnings: 0 },
      },
    ]);
    await unlisten();
  });

  it("emits an empty scan result for the first-scan library demo", async () => {
    window.history.replaceState({}, "", "/library?mock=1&demo=library-first-scan");
    installTauriMock();
    const events: LibraryScanProgress[] = [];
    const unlisten = await onLibraryScanProgress((event) => events.push(event));

    await expect(startLibraryScan(1)).resolves.toBe("mock-library-scan");
    await Promise.resolve();
    await Promise.resolve();

    expect(events).toEqual([
      {
        state: "scanning",
        scan_id: "mock-library-scan",
        root_id: 1,
        discovered: 0,
        processed: 0,
        warnings: 0,
      },
      {
        state: "done",
        scan_id: "mock-library-scan",
        root_id: 1,
        summary: { imported: 0, updated: 0, missing: 0, warnings: 0 },
      },
    ]);
    await unlisten();
  });

  it("sorts mock cards by the selected timestamp with a stable ID tie-break", async () => {
    installTauriMock();

    const taken = await queryLibrary({ ...query, sort: "taken_at_desc" });
    const imported = await queryLibrary({ ...query, sort: "imported_at_desc" });

    expect(taken.items.map((item) => item.id)).toEqual([1, 2, 3, 4]);
    expect(imported.items.map((item) => item.id)).toEqual([4, 3, 2, 1]);
    expect(imported.items[1].imported_at).toBe(imported.items[2].imported_at);
  });
});
