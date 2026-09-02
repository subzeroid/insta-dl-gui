/** @vitest-environment happy-dom */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getVersion } from "@tauri-apps/api/app";
import { version as packageVersion } from "../../package.json";
import {
  cancelJob,
  checkDownloadStatuses,
  downloadDirect,
  downloadPost,
  cancelLibraryScan,
  enqueueFetchedPostDownload,
  enqueueProfileDownload,
  ensureConfiguredLibraryRoot,
  fetchProfile,
  fetchProfileSummary,
  fetchReels,
  fetchRelationships,
  fetchStories,
  getLibraryItem,
  libraryMediaUrl,
  listLibraryRoots,
  onJobProgress,
  onLibraryScanProgress,
  openLibraryFile,
  queryLibrary,
  requestLibraryPreviewAccess,
  remoteMediaUrl,
  revealLibraryFile,
  searchRelationships,
  configState,
  saveSettings,
  setProxy,
  startLibraryScan,
  type LibraryCard,
  type LibraryPage,
  type LibraryQuery,
  type LibraryScanProgress,
  type JobProgress,
  type Post,
  type ProfilePreview,
} from "./ipc";
import { installTauriMock, uninstallTauriMock } from "./mock";

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function invoke(): Invoke {
  return (window as unknown as { __TAURI_INTERNALS__: { invoke: Invoke } }).__TAURI_INTERNALS__.invoke;
}

afterEach(() => {
  uninstallTauriMock();
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  delete (window as unknown as { __TAURI_EVENT_PLUGIN_INTERNALS__?: unknown })
    .__TAURI_EVENT_PLUGIN_INTERNALS__;
  window.history.replaceState({}, "", "/");
});

describe("app plugin mock", () => {
  it("returns the package version from the app mock", async () => {
    installTauriMock();

    await expect(getVersion()).resolves.toBe(packageVersion);
  });
});

describe("profile pagination mock", () => {
  it("provides cursor-paged relationship lists and server-side search", async () => {
    installTauriMock();

    await expect(fetchProfileSummary("natgeo")).resolves.toMatchObject({
      pk: "25025320",
      username: "natgeo",
      follower_count: 713_000_000,
      following_count: 234,
    });
    const first = await fetchRelationships("25025320", "following", null);
    const second = await fetchRelationships("25025320", "following", first.next_cursor);
    expect(first.users).toHaveLength(12);
    expect(first.next_cursor).toBe("following-cursor");
    expect(second.users).toHaveLength(12);
    expect(second.next_cursor).toBeNull();
    expect(new Set([...first.users, ...second.users].map((user) => user.pk)).size).toBe(24);

    const results = await searchRelationships("25025320", "following", "meta");
    expect(results.map((user) => user.username)).toEqual(["meta", "metaglasses"]);
  });

  it("permits only registered demo remote-media fixtures and revokes them on dispose", async () => {
    installTauriMock();
    const profile = (await invoke()("fetch_profile", {
      username: "instagram",
      endCursor: null,
    })) as ProfilePreview;
    const stories = await fetchStories("42");
    const avatar = profile.profile.avatar_url ?? "";
    const thumbnail = profile.recent_posts[0]?.thumbnail_url ?? "";
    const storyThumbnail = stories[0]?.thumb_url ?? "";

    expect(remoteMediaUrl(avatar)).toBe(avatar);
    expect(remoteMediaUrl(thumbnail)).toBe(thumbnail);
    expect(remoteMediaUrl(storyThumbnail)).toBe(storyThumbnail);
    expect(remoteMediaUrl("data:image/svg+xml,%3Csvg%3Eunregistered%3C/svg%3E")).toBe("");

    uninstallTauriMock();
    expect(remoteMediaUrl(avatar)).toBe("");
    expect(remoteMediaUrl(thumbnail)).toBe("");
    expect(remoteMediaUrl(storyThumbnail)).toBe("");
  });

  it("keeps remote-media failure demo sources original, distinct, and outside healthy fixtures", async () => {
    window.history.replaceState({}, "", "/explore?mock=1&demo=remote-media-failure");
    installTauriMock();

    const first = await fetchProfile("preview_demo", null);
    const second = await fetchProfile("preview_demo", first.end_cursor);
    const sources = [
      first.profile.avatar_url ?? "",
      ...first.recent_posts.map((post) => post.thumbnail_url ?? ""),
      ...second.recent_posts.map((post) => post.thumbnail_url ?? ""),
    ];
    const expectedSources = [
      "https://cdninstagram.com/mock-failure/avatar.jpg",
      ...Array.from(
        { length: 24 },
        (_, index) => `https://cdninstagram.com/mock-failure/post-${index}.jpg`,
      ),
    ];

    expect(first.profile).toMatchObject({
      username: "preview_demo",
      full_name: "Preview Demo",
      media_count: 24,
      follower_count: 1200,
      following_count: 40,
    });
    expect(first.recent_posts).toHaveLength(12);
    expect(first.end_cursor).toBe("cursor");
    expect(second.recent_posts).toHaveLength(12);
    expect(second.end_cursor).toBeNull();
    expect(sources).toEqual(expectedSources);
    expect(new Set(sources).size).toBe(expectedSources.length);
    expect(sources.every((source) => source.startsWith("https://cdninstagram.com/mock-failure/")))
      .toBe(true);
    expect(sources.every((source) => remoteMediaUrl(source) !== source)).toBe(true);
  });

  it("keeps a requested deep-link profile healthy when the failure demo parameter is also present", async () => {
    window.history.replaceState(
      {},
      "",
      "/explore?mock=1&profile=adidas&demo=remote-media-failure",
    );
    installTauriMock();

    const profile = await fetchProfile("adidas", null);
    const sources = [
      profile.profile.avatar_url ?? "",
      ...profile.recent_posts.map((post) => post.thumbnail_url ?? ""),
    ];

    expect(profile.profile).toMatchObject({
      username: "adidas",
      full_name: "Instagram",
      media_count: 7421,
      follower_count: 713_000_000,
      following_count: 234,
    });
    expect(sources.every((source) => source.startsWith("data:image/svg+xml,"))).toBe(true);
    expect(sources.every((source) => !source.includes("/mock-failure/"))).toBe(true);
    expect(sources.every((source) => remoteMediaUrl(source) === source)).toBe(true);
  });

  it("mirrors backend proxy URL validation and redaction", async () => {
    installTauriMock();

    await expect(configState()).resolves.toMatchObject({ has_proxy: false, proxy_hint: null });

    for (const [url, hint, secret] of [
      ["  http://proxy.example  ", "http://proxy.example/", ""],
      ["https://proxy.example", "https://proxy.example/", ""],
      ["http://proxy.example:80", "http://proxy.example/", ""],
      ["https://proxy.example:443", "https://proxy.example/", ""],
      ["socks5h://alice:secret@proxy.example:1080", "socks5h://***@proxy.example:1080/", "secret"],
      ["socks5://alice@proxy.example:1080", "socks5://***@proxy.example:1080/", "alice"],
      ["socks5h://alice%40example:se%2Fcret@proxy.example:1080", "socks5h://***@proxy.example:1080/", "se%2Fcret"],
      ["socks5://[::1]:1080", "socks5://[::1]:1080/", ""],
    ]) {
      const applied = await setProxy(url);
      expect(applied).toMatchObject({ has_proxy: true, proxy_hint: hint });
      if (secret) expect(JSON.stringify(applied)).not.toContain(secret);
    }

    for (const url of [
      "ftp://proxy.example:21",
      "http://",
      "http://proxy.example:0",
      "https://proxy.example:0",
      "socks5://proxy.example",
      "socks5h://proxy.example:0",
      "https://proxy.example/path",
      "https://proxy.example/?query=yes",
      "https://proxy.example/?",
      "https://proxy.example/#fragment",
      "https://proxy.example/#",
    ]) {
      await expect(setProxy(url)).rejects.toThrow(
        "Enter a valid HTTP, HTTPS, SOCKS5, or SOCKS5H proxy URL",
      );
    }

    await expect(setProxy(null)).resolves.toMatchObject({ has_proxy: false, proxy_hint: null });
    await expect(setProxy("   ")).resolves.toMatchObject({ has_proxy: false, proxy_hint: null });
    await expect(configState()).resolves.toMatchObject({ has_proxy: false, proxy_hint: null });
  });

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
          resources: [{ url: "https://cdninstagram.com/photo.jpg", kind: "photo" }],
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

  function validPost(pk: string, resourceCount = 1): Post {
    return {
      pk,
      code: `POST${pk}`,
      resources: Array.from({ length: resourceCount }, (_, ordinal) => ({
        url: `https://cdninstagram.com/${pk}-${ordinal}.jpg`,
        kind: "photo" as const,
      })),
    };
  }

  it("reports exact completed resource evidence and carousel counts", async () => {
    installTauriMock();
    const post: Post = {
      pk: "7001",
      code: "CAROUSEL7001",
      resources: [
        { url: "https://cdninstagram.com/7001-0.jpg", kind: "photo" },
        { url: "https://cdninstagram.com/7001-1.mp4", kind: "video" },
      ],
    };
    const request = [{ namespace: "post" as const, pk: post.pk, resources: ["photo", "video"] as const }];

    await expect(checkDownloadStatuses(request.map((item) => ({ ...item, resources: [...item.resources] }))))
      .resolves.toEqual([
        {
          namespace: "post",
          pk: "7001",
          state: "not_downloaded",
          available_resources: 0,
          expected_resources: 2,
        },
      ]);

    await enqueueFetchedPostDownload("nike", "posts", "selected", [post]);
    await vi.advanceTimersByTimeAsync(120);
    await expect(checkDownloadStatuses([{ namespace: "post", pk: "7001", resources: ["photo", "video"] }]))
      .resolves.toEqual([
        {
          namespace: "post",
          pk: "7001",
          state: "partial",
          available_resources: 1,
          expected_resources: 2,
        },
      ]);

    await vi.runAllTimersAsync();
    const [complete] = await checkDownloadStatuses([
      { namespace: "post", pk: "7001", resources: ["photo", "video"] },
    ]);
    expect(complete).toEqual({
      namespace: "post",
      pk: "7001",
      state: "downloaded",
      available_resources: 2,
      expected_resources: 2,
    });
    expect(Object.keys(complete).sort()).toEqual([
      "available_resources",
      "expected_resources",
      "namespace",
      "pk",
      "state",
    ]);
  });

  it("records direct story evidence with numeric PKs and exact video kinds", async () => {
    installTauriMock();
    const stories = await fetchStories("42");
    await downloadDirect(
      "nike",
      "stories",
      stories.map((story) => ({ pk: story.pk, taken_at: story.taken_at, url: story.media_url })),
    );
    await vi.runAllTimersAsync();

    const statuses = await checkDownloadStatuses(
      stories.map((story) => ({ namespace: "story", pk: story.pk, resources: [story.kind] })),
    );
    expect(statuses).toEqual(
      stories.map((story) => ({
        namespace: "story",
        pk: story.pk,
        state: "downloaded",
        available_resources: 1,
        expected_resources: 1,
      })),
    );
  });

  it("enumerates the same deterministic posts, reels, and stories for Profile All", async () => {
    installTauriMock();
    const { events, unlisten } = await collectJobEvents();
    const firstPosts = await fetchProfile("nike", null);
    const secondPosts = await fetchProfile("nike", firstPosts.end_cursor);
    const firstReels = await fetchReels("42", null);
    const secondReels = await fetchReels("42", firstReels.end_cursor);
    const stories = await fetchStories("42");
    const posts = [...firstPosts.recent_posts, ...secondPosts.recent_posts];
    const reels = [...firstReels.posts, ...secondReels.posts];

    await enqueueProfileDownload("nike", {
      posts: true,
      reels: true,
      stories: true,
      highlights: false,
      avatar: false,
    });
    await vi.runAllTimersAsync();

    const completed = events.find((event) => event.state === "done");
    expect(completed).toEqual(
      expect.objectContaining({
        count: 3,
        outputs: [
          expect.objectContaining({ basename: "nike_post_1.jpg", kind: "photo", ordinal: 0 }),
          expect.objectContaining({ basename: "nike_reel_1.mp4", kind: "video", ordinal: 0 }),
          expect.objectContaining({ basename: "nike_story_1.jpg", kind: "photo", ordinal: 0 }),
        ],
      }),
    );

    const statuses = await checkDownloadStatuses([
      ...posts.map((post) => ({
        namespace: "post" as const,
        pk: post.pk,
        resources: post.resources.map((resource) => resource.kind),
      })),
      ...reels.map((post) => ({
        namespace: "post" as const,
        pk: post.pk,
        resources: post.resources.map((resource) => resource.kind),
      })),
      ...stories.map((story) => ({
        namespace: "story" as const,
        pk: story.pk,
        resources: [story.kind],
      })),
    ]);

    expect(statuses).toHaveLength(posts.length + reels.length + stories.length);
    expect(statuses.every((status) => status.state === "downloaded")).toBe(true);
    expect(statuses.find((status) => status.pk === stories[1].pk)).toMatchObject({
      namespace: "story",
      available_resources: 1,
      expected_resources: 1,
    });
    await unlisten();
  });

  it("shares the post namespace between fetched post and reel downloads", async () => {
    installTauriMock();
    await enqueueFetchedPostDownload("nike", "reels", "shown", [
      {
        pk: "7002",
        code: "REEL7002",
        resources: [{ url: "https://cdninstagram.com/7002.mp4", kind: "video" }],
      },
    ]);
    await vi.runAllTimersAsync();

    await expect(
      checkDownloadStatuses([{ namespace: "post", pk: "7002", resources: ["video"] }]),
    ).resolves.toEqual([
      {
        namespace: "post",
        pk: "7002",
        state: "downloaded",
        available_resources: 1,
        expected_resources: 1,
      },
    ]);
  });

  it("keeps no evidence on early cancel and completed evidence on late cancel", async () => {
    installTauriMock();
    const earlyId = await enqueueFetchedPostDownload("nike", "posts", "selected", [
      validPost("7003", 2),
    ]);
    await vi.advanceTimersByTimeAsync(15);
    await cancelJob(earlyId);
    await expect(
      checkDownloadStatuses([{ namespace: "post", pk: "7003", resources: ["photo", "photo"] }]),
    ).resolves.toEqual([
      expect.objectContaining({ state: "not_downloaded", available_resources: 0 }),
    ]);

    const lateId = await enqueueFetchedPostDownload("nike", "posts", "selected", [
      validPost("7004", 2),
    ]);
    await vi.advanceTimersByTimeAsync(120);
    await cancelJob(lateId);
    await vi.runAllTimersAsync();
    await expect(
      checkDownloadStatuses([{ namespace: "post", pk: "7004", resources: ["photo", "photo"] }]),
    ).resolves.toEqual([
      expect.objectContaining({ state: "partial", available_resources: 1, expected_resources: 2 }),
    ]);
  });

  it("preserves completed evidence when the download later fails", async () => {
    window.history.replaceState({}, "", "/explore?mock=1&demo=download-failure");
    installTauriMock();
    const { events, unlisten } = await collectJobEvents();
    const jobId = await enqueueFetchedPostDownload("nike", "posts", "selected", [
      validPost("7005", 2),
    ]);

    await vi.advanceTimersByTimeAsync(350);

    expect(events.at(-1)).toEqual(expect.objectContaining({ job_id: jobId, state: "failed" }));
    await expect(
      checkDownloadStatuses([{ namespace: "post", pk: "7005", resources: ["photo", "photo"] }]),
    ).resolves.toEqual([
      expect.objectContaining({ state: "partial", available_resources: 1, expected_resources: 2 }),
    ]);
    await unlisten();
  });

  it("requires evidence from the currently configured download root", async () => {
    installTauriMock();
    await enqueueFetchedPostDownload("nike", "posts", "shown", [validPost("7006")]);
    await vi.runAllTimersAsync();
    await expect(
      checkDownloadStatuses([{ namespace: "post", pk: "7006", resources: ["photo"] }]),
    ).resolves.toEqual([expect.objectContaining({ state: "downloaded" })]);

    await saveSettings({ dest_dir: "/mock/another-root" });
    await expect(
      checkDownloadStatuses([{ namespace: "post", pk: "7006", resources: ["photo"] }]),
    ).resolves.toEqual([
      expect.objectContaining({ state: "not_downloaded", available_resources: 0 }),
    ]);
  });

  it("treats distinct candidates across roots as ambiguous without duplicating an exact retry", async () => {
    installTauriMock();
    const request = [{ namespace: "post" as const, pk: "7007", resources: ["photo" as const] }];

    await enqueueFetchedPostDownload("nike", "posts", "shown", [validPost("7007")]);
    await vi.runAllTimersAsync();
    await enqueueFetchedPostDownload("nike", "posts", "shown", [validPost("7007")]);
    await vi.runAllTimersAsync();
    await expect(checkDownloadStatuses(request)).resolves.toEqual([
      expect.objectContaining({ state: "downloaded", available_resources: 1 }),
    ]);

    await saveSettings({ dest_dir: "/mock/second-root" });
    await enqueueFetchedPostDownload("nike", "posts", "shown", [validPost("7007")]);
    await vi.runAllTimersAsync();
    await expect(checkDownloadStatuses(request)).resolves.toEqual([
      expect.objectContaining({ state: "not_downloaded", available_resources: 0 }),
    ]);
  });

  it("treats the same identity downloaded into two directories of one root as ambiguous", async () => {
    installTauriMock();
    const item = { pk: "7008", url: "https://cdninstagram.com/7008.jpg" };
    const request = [{ namespace: "story" as const, pk: "7008", resources: ["photo" as const] }];

    await downloadDirect("nike", "stories", [item]);
    await vi.runAllTimersAsync();
    await expect(checkDownloadStatuses(request)).resolves.toEqual([
      expect.objectContaining({ state: "downloaded", available_resources: 1 }),
    ]);

    await downloadDirect("adidas", "stories", [item]);
    await vi.runAllTimersAsync();
    await expect(checkDownloadStatuses(request)).resolves.toEqual([
      expect.objectContaining({ state: "not_downloaded", available_resources: 0 }),
    ]);
  });

  it("mirrors backend status validation limits, duplicate rules, and response order", async () => {
    installTauriMock();
    const statusInvoke = (items: unknown[]) => invoke()("check_download_statuses", { items });
    const post = (pk: string, resources: unknown[] = ["photo"]) => ({
      namespace: "post",
      pk,
      resources,
    });

    await expect(statusInvoke([])).resolves.toEqual([]);
    await expect(statusInvoke(Array.from({ length: 501 }, (_, index) => post(String(index + 1)))))
      .rejects.toThrow("Download status batch exceeds maximum of 500 items");
    await expect(statusInvoke([post("not-numeric")]))
      .rejects.toThrow("Download status PK must contain only ASCII digits");
    await expect(statusInvoke([post("1".repeat(65))]))
      .rejects.toThrow("Download status PK exceeds maximum of 64 bytes");
    await expect(statusInvoke([post("1"), post("1")]))
      .rejects.toThrow("Download status batch contains a duplicate namespace and PK");
    await expect(statusInvoke([post("2", [])]))
      .rejects.toThrow("Post download status must contain between 1 and 20 resources");
    await expect(statusInvoke([post("2", Array.from({ length: 21 }, () => "photo"))]))
      .rejects.toThrow("Post download status must contain between 1 and 20 resources");
    await expect(statusInvoke([{ namespace: "story", pk: "3", resources: [] }]))
      .rejects.toThrow("Story download status must contain exactly one resource");
    await expect(statusInvoke([{ namespace: "story", pk: "3", resources: ["photo", "video"] }]))
      .rejects.toThrow("Story download status must contain exactly one resource");
    await expect(statusInvoke([post("4", ["audio"])]))
      .rejects.toThrow("Download status resources must be photos or videos");
    const sparseResources: unknown[] = new Array(2);
    sparseResources[0] = "photo";
    await expect(statusInvoke([post("4", sparseResources)]))
      .rejects.toThrow("Download status resources must be photos or videos");
    await expect(statusInvoke([{ namespace: "other", pk: "5", resources: ["photo"] }]))
      .rejects.toThrow();

    const maximum = Array.from({ length: 500 }, (_, index) => post(String(index + 1)));
    await expect(statusInvoke(maximum)).resolves.toHaveLength(500);
    await expect(statusInvoke([
      post("12", ["video"]),
      { namespace: "story", pk: "12", resources: ["photo"] },
      post("11"),
    ])).resolves.toEqual([
      expect.objectContaining({ namespace: "post", pk: "12" }),
      expect.objectContaining({ namespace: "story", pk: "12" }),
      expect.objectContaining({ namespace: "post", pk: "11" }),
    ]);
  });

  it("emits ordered exact outputs for four fetched posts containing five resources", async () => {
    installTauriMock();
    const { events, unlisten } = await collectJobEvents();
    const posts: Post[] = [
      {
        pk: "1",
        code: "PHOTO1",
        resources: [{ url: "https://cdninstagram.com/one.jpg", kind: "photo" }],
      },
      {
        pk: "2",
        code: "VIDEO2",
        resources: [{ url: "https://cdninstagram.com/two.mp4", kind: "video" }],
      },
      {
        pk: "3",
        code: "ALBUM3",
        resources: [
          { url: "https://cdninstagram.com/three.jpg", kind: "photo" },
          { url: "https://cdninstagram.com/four.mp4", kind: "video" },
        ],
      },
      {
        pk: "4",
        code: "PHOTO4",
        resources: [{ url: "https://cdninstagram.com/five.jpg", kind: "photo" }],
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
        file_name: "PHOTO1_1.jpg",
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
          { file_id: 10101, basename: "PHOTO1_1.jpg", kind: "photo", byte_size: 1_500_000, ordinal: 0 },
          { file_id: 10102, basename: "VIDEO2_1.mp4", kind: "video", byte_size: 2_000_000, ordinal: 0 },
          { file_id: 10103, basename: "ALBUM3_1.jpg", kind: "photo", byte_size: 1_500_000, ordinal: 0 },
          { file_id: 10104, basename: "ALBUM3_2.mp4", kind: "video", byte_size: 2_000_000, ordinal: 1 },
          { file_id: 10105, basename: "PHOTO4_1.jpg", kind: "photo", byte_size: 1_500_000, ordinal: 0 },
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

  it("allocates globally unique file IDs when an earlier job has over 100 outputs", async () => {
    installTauriMock();
    const { events, unlisten } = await collectJobEvents();

    const firstId = await enqueueFetchedPostDownload(
      "nike",
      "posts",
      "shown",
      Array.from({ length: 101 }, (_, index) => validPost(String(index + 1))),
    );
    const secondId = await downloadPost("AFTER101");
    await vi.runAllTimersAsync();

    const outputs = events
      .filter((event) => event.state === "done" && (event.job_id === firstId || event.job_id === secondId))
      .flatMap((event) => event.outputs ?? []);
    const ids = outputs.map((output) => output.file_id);
    expect(outputs).toHaveLength(102);
    expect(ids.every((id) => typeof id === "number" && id > 0)).toBe(true);
    expect(new Set(ids).size).toBe(ids.length);
    await unlisten();
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

  it("keeps story fixtures browser-safe while preserving fixture media kinds", async () => {
    installTauriMock();
    const { events, unlisten } = await collectJobEvents();
    const stories = await fetchStories("42");

    expect(stories.every((story) => /^\d+$/.test(story.pk))).toBe(true);
    expect(stories.every((story) => story.media_url === "" || story.media_url.startsWith("data:")))
      .toBe(true);
    expect(stories.every((story) => story.thumb_url?.startsWith("data:image/svg+xml,")))
      .toBe(true);
    const jobId = await downloadDirect(
      "nike",
      "stories",
      stories.map((story) => ({ pk: story.pk, taken_at: story.taken_at, url: story.media_url })),
    );

    await vi.advanceTimersByTimeAsync(15);
    expect(events.at(-1)).toEqual(
      expect.objectContaining({ job_id: jobId, state: "downloading", file_name: "9200001_1.jpg" }),
    );
    await vi.runAllTimersAsync();
    expect(events.at(-1)?.outputs?.map((output) => output.kind)).toEqual([
      "photo",
      "video",
      "photo",
    ]);
    await unlisten();
  });

  it("rejects malformed download input without allocating a job or emitting progress", async () => {
    installTauriMock();
    const { events, unlisten } = await collectJobEvents();
    const valid = validPost("100");
    const directItem = { pk: "100", url: "https://cdninstagram.com/direct.jpg" };
    const validOptions = {
      posts: true,
      reels: false,
      stories: false,
      highlights: false,
      avatar: false,
      max_posts: null,
    };
    const invalidCalls: Array<() => Promise<unknown>> = [
      () => enqueueFetchedPostDownload("nike", "posts", "shown", []),
      () => enqueueFetchedPostDownload(
        "nike",
        "posts",
        "shown",
        Array.from({ length: 501 }, (_, index) => validPost(String(index + 1))),
      ),
      () => invoke()("enqueue_fetched_post_download", {
        username: "nike",
        category: "stories",
        scope: "shown",
        posts: [valid],
      }),
      () => invoke()("enqueue_fetched_post_download", {
        username: "nike",
        category: "posts",
        scope: "all",
        posts: [valid],
      }),
      () => invoke()("enqueue_fetched_post_download", {
        username: "nike",
        category: "posts",
        scope: "shown",
        posts: [{ ...valid, pk: "not-numeric" }],
      }),
      () => invoke()("enqueue_fetched_post_download", {
        username: "nike",
        category: "posts",
        scope: "shown",
        posts: [{ ...valid, resources: [{ url: "http://127.0.0.1/private", kind: "audio" }] }],
      }),
      () => downloadDirect("nike", "stories", []),
      () => downloadDirect(
        "nike",
        "stories",
        Array.from({ length: 501 }, (_, index) => ({ ...directItem, pk: String(index + 1) })),
      ),
      () => invoke()("download_direct", {
        label: "nike",
        subfolder: "stories",
        items: [{ pk: "", url: "not-a-url" }],
      }),
      () => invoke()("download_direct", {
        label: "nike",
        subfolder: "stories",
        items: [{ pk: "100", url: "mock://stories/unsupported.mp4" }],
      }),
      () => downloadPost("   "),
      () => enqueueProfileDownload("", validOptions),
      () => enqueueProfileDownload("nike", { ...validOptions, posts: false }),
      () => invoke()("enqueue_profile_download", {
        username: "nike",
        opts: { ...validOptions, posts: "yes" },
      }),
    ];

    for (const invalidCall of invalidCalls) {
      await expect(invalidCall()).rejects.toBeDefined();
    }
    await vi.runAllTimersAsync();
    expect(events).toEqual([]);

    const validJobId = await downloadPost("VALID");
    expect(validJobId).toBe("mock-job-1");
    await vi.runAllTimersAsync();
    expect(events.at(-1)).toEqual(
      expect.objectContaining({
        job_id: validJobId,
        state: "done",
        outputs: [expect.objectContaining({ file_id: 10101 })],
      }),
    );
    await unlisten();
  });

  it("disposes pending timers and listeners before reinstalling the mock", async () => {
    installTauriMock();
    const { events } = await collectJobEvents();
    await downloadPost("OLD");
    await vi.advanceTimersByTimeAsync(15);
    expect(events.map((event) => event.state)).toEqual(["downloading"]);

    installTauriMock();
    await vi.runAllTimersAsync();

    expect(events.map((event) => event.state)).toEqual(["downloading"]);
  });

  it("registers browser-loadable media fixtures by output kind and drops them on reinstall", async () => {
    installTauriMock();
    const { events, unlisten } = await collectJobEvents();
    const jobId = await enqueueFetchedPostDownload("nike", "posts", "shown", [
      {
        pk: "1",
        code: "MIXED",
        resources: [
          { url: "https://cdninstagram.com/photo.jpg", kind: "photo" },
          { url: "https://cdninstagram.com/video.mp4", kind: "video" },
        ],
      },
    ]);
    await vi.runAllTimersAsync();
    const outputs = events.find((event) => event.job_id === jobId && event.state === "done")?.outputs;
    const photoUrl = libraryMediaUrl(outputs?.[0]?.file_id ?? -1);
    const videoUrl = libraryMediaUrl(outputs?.[1]?.file_id ?? -1);

    expect(photoUrl).toMatch(/^data:image\/svg\+xml,/);
    expect(videoUrl).toMatch(/^data:video\/mp4;base64,/);
    const videoBytes = Uint8Array.from(atob(videoUrl.split(",")[1] ?? ""), (char) =>
      char.charCodeAt(0),
    );
    expect(new TextDecoder().decode(videoBytes.slice(4, 8))).toBe("ftyp");
    await expect(requestLibraryPreviewAccess(outputs?.[0]?.file_id ?? -1)).resolves.toBe(true);
    await expect(requestLibraryPreviewAccess(outputs?.[1]?.file_id ?? -1)).resolves.toBe(true);
    await unlisten();

    installTauriMock();
    expect(libraryMediaUrl(outputs?.[0]?.file_id ?? -1)).toMatch(/^library:\/\//);
    await expect(requestLibraryPreviewAccess(outputs?.[0]?.file_id ?? -1)).resolves.toBe(false);
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
        expect.objectContaining({
          id: 2,
          kind: "reel",
          preview_file_kind: "video",
          availability: "available",
        }),
        expect.objectContaining({ id: 3, resource_count: 3, availability: "available" }),
        expect.objectContaining({ id: 4, availability: "missing" }),
      ]),
    );
    expect(detail.id).toBe(page.items[0].id);
    expect(libraryMediaUrl(detail.files[0].id)).toMatch(/^data:image\/svg\+xml,/);
    await expect(requestLibraryPreviewAccess(999_999)).resolves.toBe(false);
    await expect(openLibraryFile(999_999)).rejects.toThrow("Library file is unavailable");
    await expect(revealLibraryFile(999_999)).rejects.toThrow("Library file is unavailable");
  });

  it("registers available static files and rejects known missing files", async () => {
    installTauriMock();
    const page = await queryLibrary(query);
    const details = await Promise.all(page.items.map((item) => getLibraryItem(item.id)));
    const files = details.flatMap((detail) => detail.files);
    const availableFiles = files.filter((file) => file.exists_on_disk);
    const missingFiles = files.filter((file) => !file.exists_on_disk);

    expect(availableFiles.length).toBeGreaterThan(0);
    expect(missingFiles.length).toBeGreaterThan(0);
    for (const file of availableFiles) {
      const url = libraryMediaUrl(file.id);
      expect(url).toMatch(
        file.kind === "video" ? /^data:video\/mp4;base64,/ : /^data:image\/svg\+xml,/,
      );
      await expect(requestLibraryPreviewAccess(file.id)).resolves.toBe(true);
      await expect(openLibraryFile(file.id)).resolves.toBeNull();
      await expect(revealLibraryFile(file.id)).resolves.toBeNull();
    }
    for (const file of missingFiles) {
      await expect(requestLibraryPreviewAccess(file.id)).resolves.toBe(false);
      await expect(openLibraryFile(file.id)).rejects.toThrow("Library file is unavailable");
      await expect(revealLibraryFile(file.id)).rejects.toThrow("Library file is unavailable");
    }
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
