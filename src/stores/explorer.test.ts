import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it } from "vitest";

import type { Post, ProfilePreview } from "../lib/ipc";
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

beforeEach(() => {
  setActivePinia(createPinia());
});

describe("Explore session state", () => {
  it("retains the current profile and tab in the same Pinia session", () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const first = useExplorerStore();
    first.query = "@nike";
    first.commitProfile(preview("nike", "42"));
    first.activeTab = "reels";
    first.commitReelsPage("42", [post("r1")], "next");

    const remounted = useExplorerStore(pinia);

    expect(remounted.query).toBe("@nike");
    expect(remounted.profilePreview?.profile.username).toBe("nike");
    expect(remounted.activeTab).toBe("reels");
    expect(remounted.reels.map((item) => item.pk)).toEqual(["r1"]);
    expect(remounted.reelsCursor).toBe("next");
  });

  it("clears profile media atomically when a different profile starts loading", () => {
    const store = useExplorerStore();
    store.commitProfile(preview("nike", "42", [post("p1")]));
    store.activeTab = "reels";
    store.commitReelsPage("42", [post("r1")], "next");
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

    expect(store.commitReelsPage("42", [post("r1")], "reels-next")).toBe(true);
    expect(
      store.commitReelsPage("42", [post("r1", "replacement"), post("r2")], null),
    ).toBe(true);
    expect(store.reels.map((item) => item.pk)).toEqual(["r1", "r2"]);
    expect(store.reels[0]?.thumbnail_url).toContain("r1.jpg");

    expect(store.commitReelsPage("other", [post("stale")], null)).toBe(false);
    expect(store.reels.map((item) => item.pk)).toEqual(["r1", "r2"]);
  });
});
