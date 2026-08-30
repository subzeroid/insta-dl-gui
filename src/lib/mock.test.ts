/** @vitest-environment happy-dom */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cancelJob,
  downloadDirect,
  downloadPost,
  cancelLibraryScan,
  enqueueFetchedPostDownload,
  enqueueProfileDownload,
  ensureConfiguredLibraryRoot,
  fetchStories,
  getLibraryItem,
  libraryMediaUrl,
  listLibraryRoots,
  onJobProgress,
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
  type JobProgress,
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
  it("handles the official clipboard manager write command", async () => {
    installTauriMock();

    await expect(
      invoke()("plugin:clipboard-manager|write_text", { text: "Copy this caption" }),
    ).resolves.toBeNull();
  });

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

  it("supports exact fetched-media downloads in the Explore demo", async () => {
    installTauriMock();

    const jobId = await enqueueFetchedPostDownload("natgeo", "posts", "shown", [
        {
          pk: "1",
          code: "POST1",
          resources: [{ url: "https://cdn.example/photo.jpg", kind: "photo" }],
        },
      ]);

    expect(jobId).toMatch(/^mock-job-\d+$/);
  });
});

describe("download journey mock", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  async function collectJobEvents() {
    const events: JobProgress[] = [];
    const unlisten = await onJobProgress((event) => events.push(event));
    return { events, unlisten };
  }

  it("emits ordered exact outputs for four fetched posts containing five resources", async () => {
    installTauriMock();
    const { events, unlisten } = await collectJobEvents();
    const posts: Post[] = [
      {
        pk: "1",
        code: "PHOTO1",
        resources: [{ url: "https://cdn.example/one.jpg", kind: "photo" }],
      },
      {
        pk: "2",
        code: "VIDEO2",
        resources: [{ url: "https://cdn.example/two.mp4", kind: "video" }],
      },
      {
        pk: "3",
        code: "ALBUM3",
        resources: [
          { url: "https://cdn.example/three.jpg", kind: "photo" },
          { url: "https://cdn.example/four.mp4", kind: "video" },
        ],
      },
      {
        pk: "4",
        code: "PHOTO4",
        resources: [{ url: "https://cdn.example/five.jpg", kind: "photo" }],
      },
    ];

    const jobId = await enqueueFetchedPostDownload("nike", "posts", "selected", posts);
    expect(events).toEqual([]);

    await vi.advanceTimersByTimeAsync(15);
    expect(events).toEqual([
      expect.objectContaining({
        job_id: jobId,
        state: "downloading",
        current_file: 1,
        total_files: 5,
        bytes_done: expect.any(Number),
        file_name: "PHOTO1_0.jpg",
      }),
    ]);
    await vi.advanceTimersByTimeAsync(500);
    expect(events).toHaveLength(1);

    await vi.runAllTimersAsync();
    const done = events.at(-1);
    expect(done).toEqual(
      expect.objectContaining({
        job_id: jobId,
        state: "done",
        count: 5,
        dir: "/mock/instagram-archive/nike/posts",
        requested_items: 4,
        outputs: [
          { file_id: 10101, basename: "PHOTO1_0.jpg", kind: "photo", byte_size: 1_500_000, ordinal: 0 },
          { file_id: 10102, basename: "VIDEO2_0.mp4", kind: "video", byte_size: 2_000_000, ordinal: 0 },
          { file_id: 10103, basename: "ALBUM3_0.jpg", kind: "photo", byte_size: 1_500_000, ordinal: 0 },
          { file_id: 10104, basename: "ALBUM3_1.mp4", kind: "video", byte_size: 2_000_000, ordinal: 1 },
          { file_id: 10105, basename: "PHOTO4_0.jpg", kind: "photo", byte_size: 1_500_000, ordinal: 0 },
        ],
      }),
    );
    expect(done?.outputs?.every((output) => output.file_id && output.file_id > 0)).toBe(true);
    expect(done?.outputs?.every((output) => !("path" in output))).toBe(true);
    await unlisten();
  });

  it("returns a unique deterministic ID for each enqueue", async () => {
    installTauriMock();

    const first = await downloadPost("ONE");
    const second = await downloadPost("TWO");
    const third = await enqueueProfileDownload("nike", {
      posts: true,
      reels: false,
      stories: false,
      highlights: false,
      avatar: false,
    });

    expect([first, second, third]).toEqual(["mock-job-1", "mock-job-2", "mock-job-3"]);
  });

  it("reports requested item semantics for profile, standalone, and direct downloads", async () => {
    installTauriMock();
    const { events, unlisten } = await collectJobEvents();

    const profileId = await enqueueProfileDownload("nike", {
      posts: true,
      reels: false,
      stories: false,
      highlights: false,
      avatar: false,
    });
    const postId = await downloadPost("POSTCODE");
    const stories = await fetchStories("42");
    const directId = await downloadDirect(
      "nike",
      "stories",
      stories.map((story) => ({ pk: story.pk, taken_at: story.taken_at, url: story.media_url })),
    );

    await vi.runAllTimersAsync();
    const completed = events.filter((event) => event.state === "done");
    const profile = completed.find((event) => event.job_id === profileId);
    const post = completed.find((event) => event.job_id === postId);
    const direct = completed.find((event) => event.job_id === directId);

    expect(profile).not.toHaveProperty("requested_items");
    expect(profile?.outputs?.length).toBeGreaterThan(0);
    expect(post).toEqual(expect.objectContaining({ requested_items: 1, count: 1 }));
    expect(direct).toEqual(expect.objectContaining({ requested_items: 3, count: 3 }));
    expect(direct?.outputs?.map((output) => output.kind)).toEqual(["photo", "video", "photo"]);
    await unlisten();
  });

  it("cancels an active job and prevents its later Done event", async () => {
    installTauriMock();
    const { events, unlisten } = await collectJobEvents();

    const jobId = await downloadPost("CANCELME");
    await vi.advanceTimersByTimeAsync(15);
    await expect(cancelJob(jobId)).resolves.toBe(true);
    await vi.runAllTimersAsync();

    expect(events.map((event) => event.state)).toEqual(["downloading", "cancelled"]);
    expect(events.at(-1)).toEqual(expect.objectContaining({ job_id: jobId, state: "cancelled" }));
    await expect(cancelJob(jobId)).resolves.toBe(false);
    await expect(cancelJob("missing-job")).resolves.toBe(false);
    await unlisten();
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
