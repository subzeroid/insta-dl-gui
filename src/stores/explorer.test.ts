import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it } from "vitest";

import type { Post, ProfilePreview, StoryItem } from "../lib/ipc";
import { useExplorerStore } from "./explorer";

function post(pk: string, thumbnail = pk): Post {
  return {
    pk,
    code: pk.toUpperCase(),
    resources: [{ url: `https://cdninstagram.com/${pk}.mp4`, kind: "video" }],
    thumbnail_url: `https://cdninstagram.com/${thumbnail}.jpg`,
  };
}

function preview(username: string, pk: string, posts: Post[] = []): ProfilePreview {
  return {
    profile: {
      pk,
      username,
      media_count: posts.length,
      is_private: false,
      is_verified: false,
    },
    recent_posts: posts,
    end_cursor: null,
  };
}

function story(pk: string): StoryItem {
  return { pk, kind: "photo", media_url: `https://cdninstagram.com/${pk}.jpg` };
}

beforeEach(() => {
  setActivePinia(createPinia());
});

describe("Explore session state", () => {
  it("persists a Posts filter without changing retained selections and resets it for a replacement profile", () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const first = useExplorerStore();

    expect(first.postFilter).toBe("all");
    first.toggleSelected("posts", "photo-1");
    first.postFilter = "videos";

    const remounted = useExplorerStore(pinia);
    expect(remounted.postFilter).toBe("videos");
    expect(first.isSelected("posts", "photo-1")).toBe(true);

    first.beginProfileLoad();
    expect(first.postFilter).toBe("all");
    expect(first.isSelected("posts", "photo-1")).toBe(false);
  });

  it("retains the current profile and tab in the same Pinia session", () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const first = useExplorerStore();
    first.query = "@nike";
    first.commitProfile(preview("nike", "42"));
    first.activeTab = "reels";
    first.commitReelsPage("42", [post("r1")], null, "next");
    first.toggleSelected("posts", "p1");
    first.toggleSelected("stories", "s1");

    const remounted = useExplorerStore(pinia);

    expect(remounted.query).toBe("@nike");
    expect(remounted.profilePreview?.profile.username).toBe("nike");
    expect(remounted.activeTab).toBe("reels");
    expect(remounted.reels.map((item) => item.pk)).toEqual(["r1"]);
    expect(remounted.reelsCursor).toBe("next");
    expect(remounted.isSelected("posts", "p1")).toBe(true);
    expect(remounted.isSelected("stories", "s1")).toBe(true);
  });

  it("tracks selections independently per tab and clears unchanged submitted entries", () => {
    const store = useExplorerStore();
    store.toggleSelected("posts", "p1");
    store.toggleSelected("posts", "p2");
    store.toggleSelected("reels", "r1");
    store.toggleSelected("stories", "s1");
    store.toggleSelected("stories", "s2");

    expect(store.isSelected("posts", "p1")).toBe(true);
    expect(store.isSelected("reels", "p1")).toBe(false);
    store.toggleSelected("posts", "p1");
    expect(store.isSelected("posts", "p1")).toBe(false);

    const submitted = store.selectionSnapshot("stories").filter((item) => item.pk === "s1");
    store.clearSubmitted("stories", submitted);
    expect(store.isSelected("stories", "s1")).toBe(false);
    expect(store.isSelected("stories", "s2")).toBe(true);
    expect(store.isSelected("posts", "p2")).toBe(true);
    expect(store.isSelected("reels", "r1")).toBe(true);
  });

  it("preserves IDs selected after a selection snapshot", () => {
    const store = useExplorerStore();
    store.toggleSelected("posts", "p1");
    const submitted = store.selectionSnapshot("posts");

    store.toggleSelected("posts", "p2");
    store.clearSubmitted("posts", submitted);

    expect(store.selected.posts).toEqual(["p2"]);
    expect(store.selectionSnapshot("posts")).toHaveLength(1);
  });

  it("does not clear an unrelated public selection without a captured revision", () => {
    const store = useExplorerStore();
    store.toggleSelected("posts", "p1");
    const submitted = store.selectionSnapshot("posts");
    store.selected.posts.push("p2");

    store.clearSubmitted("posts", submitted);

    expect(store.selected.posts).toEqual(["p2"]);
  });

  it("preserves the same ID when it is deselected and reselected while pending", () => {
    const store = useExplorerStore();
    store.toggleSelected("posts", "p1");
    const submitted = store.selectionSnapshot("posts");

    store.toggleSelected("posts", "p1");
    store.toggleSelected("posts", "p1");
    const reselected = store.selectionSnapshot("posts");
    store.clearSubmitted("posts", submitted);

    expect(reselected[0]!.revision).toBeGreaterThan(submitted[0]!.revision);
    expect(store.selected.posts).toEqual(["p1"]);
    expect(store.selectionSnapshot("posts")).toEqual(reselected);
  });

  it("deduplicates a pending Stories request and clears loading on matching completion", () => {
    const store = useExplorerStore();
    store.commitProfile(preview("nike", "42"));

    const token = store.beginStoriesRequest("nike");
    expect(token).not.toBeNull();
    expect(store.storiesLoading).toBe(true);
    expect(store.beginStoriesRequest("nike")).toBeNull();
    expect(store.commitStories("nike", token!, [story("s1")])).toBe(true);
    expect(store.stories).toEqual([story("s1")]);
    expect(store.storiesLoading).toBe(false);
  });

  it("clears a Stories error for retry while retaining the last snapshot", () => {
    const store = useExplorerStore();
    store.commitProfile(preview("nike", "42"));
    const initial = store.beginStoriesRequest("nike")!;
    store.commitStories("nike", initial, [story("s1")]);
    const failed = store.beginStoriesRequest("nike")!;
    expect(store.failStories("nike", failed, "request failed")).toBe(true);
    expect(store.stories).toEqual([story("s1")]);
    expect(store.storiesError).toBe("request failed");
    expect(store.storiesLoading).toBe(false);

    const retry = store.beginStoriesRequest("nike");
    expect(retry).not.toBeNull();
    expect(store.stories).toEqual([story("s1")]);
    expect(store.storiesError).toBeNull();
    expect(store.storiesLoading).toBe(true);
  });

  it("rejects stale Stories tokens even when the same profile returns", () => {
    const store = useExplorerStore();
    store.commitProfile(preview("nike", "42"));
    const firstNike = store.beginStoriesRequest("nike")!;
    store.beginProfileLoad();
    store.commitProfile(preview("adidas", "84"));
    const adidas = store.beginStoriesRequest("adidas")!;
    store.beginProfileLoad();
    store.commitProfile(preview("nike", "42"));
    const currentNike = store.beginStoriesRequest("nike")!;

    expect(store.commitStories("nike", firstNike, [story("stale")])).toBe(false);
    expect(store.failStories("adidas", adidas, "stale failure")).toBe(false);
    expect(store.stories).toBeNull();
    expect(store.storiesError).toBeNull();
    expect(store.storiesLoading).toBe(true);
    expect(store.commitStories("nike", currentNike, [story("fresh")])).toBe(true);
    expect(store.stories).toEqual([story("fresh")]);
    expect(store.storiesLoading).toBe(false);
  });

  it("invalidates a pending Stories token when profile replacement starts", () => {
    const store = useExplorerStore();
    store.commitProfile(preview("nike", "42"));
    const token = store.beginStoriesRequest("nike")!;

    store.beginProfileLoad();

    expect(store.storiesLoading).toBe(false);
    expect(store.commitStories("nike", token, [story("stale")])).toBe(false);
    expect(store.failStories("nike", token, "stale failure")).toBe(false);
    expect(store.stories).toBeNull();
    expect(store.storiesError).toBeNull();
  });

  it("rejects Stories requests that do not match the committed profile", () => {
    const store = useExplorerStore();
    store.commitProfile(preview("nike", "42"));

    expect(store.beginStoriesRequest("other")).toBeNull();
    expect(store.stories).toBeNull();
    expect(store.storiesLoading).toBe(false);
  });

  it("prunes vanished Story selections and clears all transient state on profile load", () => {
    const store = useExplorerStore();
    store.commitProfile(preview("nike", "42"));
    store.toggleSelected("stories", "s1");
    store.toggleSelected("stories", "s2");
    store.toggleSelected("posts", "p1");
    const commitToken = store.beginStoriesRequest("nike")!;
    store.commitStories("nike", commitToken, [story("s1")]);
    expect(store.isSelected("stories", "s1")).toBe(true);
    expect(store.isSelected("stories", "s2")).toBe(false);
    expect(store.selectionSnapshot("stories").map((item) => item.pk)).toEqual(["s1"]);
    const failToken = store.beginStoriesRequest("nike")!;
    store.failStories("nike", failToken, "oops");

    store.beginProfileLoad();

    expect(store.stories).toBeNull();
    expect(store.storiesError).toBeNull();
    expect(store.storiesLoading).toBe(false);
    expect(store.isSelected("stories", "s1")).toBe(false);
    expect(store.isSelected("posts", "p1")).toBe(false);
    expect(store.selectionSnapshot("stories")).toEqual([]);
    expect(store.selectionSnapshot("posts")).toEqual([]);
  });

  it("clears profile media atomically when a different profile starts loading", () => {
    const store = useExplorerStore();
    store.commitProfile(preview("nike", "42", [post("p1")]));
    store.activeTab = "reels";
    store.commitReelsPage("42", [post("r1")], null, "next");
    store.stories = [
      { pk: "s1", kind: "photo", media_url: "https://cdninstagram.com/s1.jpg" },
    ];

    store.beginProfileLoad();

    expect(store.profilePreview).toBeNull();
    expect(store.activeTab).toBe("posts");
    expect(store.reels).toEqual([]);
    expect(store.reelsCursor).toBeNull();
    expect(store.reelsLoaded).toBe(false);
    expect(store.stories).toBeNull();
  });

  it("deduplicates pages and rejects media from a different profile", () => {
    const store = useExplorerStore();
    store.commitProfile({ ...preview("nike", "42", [post("p1")]), end_cursor: "posts-next" });

    expect(
      store.commitMorePosts("nike", {
        ...preview("nike", "42", [post("p1", "replacement"), post("p2")]),
        end_cursor: null,
      }),
    ).toBe(true);
    expect(store.profilePreview?.recent_posts.map((item) => item.pk)).toEqual(["p1", "p2"]);
    expect(store.profilePreview?.recent_posts[0]?.thumbnail_url).toContain("p1.jpg");

    expect(store.commitReelsPage("42", [post("r1")], null, "reels-next")).toBe(true);
    expect(
      store.commitReelsPage(
        "42",
        [post("r1", "replacement"), post("r2")],
        "reels-next",
        null,
      ),
    ).toBe(true);
    expect(store.reels.map((item) => item.pk)).toEqual(["r1", "r2"]);
    expect(store.reels[0]?.thumbnail_url).toContain("r1.jpg");

    expect(store.commitReelsPage("other", [post("stale")], null, null)).toBe(false);
    expect(store.reels.map((item) => item.pk)).toEqual(["r1", "r2"]);
  });

  it("stops offering load more when the clips API repeats a requested cursor", () => {
    const store = useExplorerStore();
    store.commitProfile(preview("nike", "42"));

    expect(store.commitReelsPage("42", [post("r1")], null, "same")).toBe(true);
    expect(store.reelsCursor).toBe("same");

    expect(store.commitReelsPage("42", [post("r2")], "same", "same")).toBe(true);
    expect(store.reels.map((item) => item.pk)).toEqual(["r1", "r2"]);
    expect(store.reelsCursor).toBeNull();
  });

  it("stops offering load more when clips cursors form a longer cycle", () => {
    const store = useExplorerStore();
    store.commitProfile(preview("nike", "42"));

    store.commitReelsPage("42", [post("r1")], null, "a");
    store.commitReelsPage("42", [post("r2")], "a", "b");
    store.commitReelsPage("42", [post("r3")], "b", "a");

    expect(store.reelsCursor).toBeNull();
  });
});
