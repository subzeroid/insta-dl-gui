/** @vitest-environment happy-dom */

import { createPinia, setActivePinia, type Pinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => ({
  downloadDirect: vi.fn(),
  downloadPost: vi.fn(),
  enqueueProfileDownload: vi.fn(),
  fetchProfile: vi.fn(),
  fetchReels: vi.fn(),
  fetchStories: vi.fn(),
  resolveInput: vi.fn(),
  searchUsers: vi.fn(),
}));

vi.mock("../lib/ipc", () => ({
  ...ipc,
  cancelJob: vi.fn(),
  formatBytes: (n: number) => `${n} B`,
  onJobProgress: vi.fn(),
}));

import ExplorerView from "./ExplorerView.vue";
import type { MediaPage, ProfilePreview } from "../lib/ipc";
import { useExplorerStore } from "../stores/explorer";
import { useJobsStore } from "../stores/jobs";

const preview = {
  profile: {
    pk: "42",
    username: "nike",
    full_name: "Nike",
    media_count: 10,
    follower_count: 100,
    is_private: false,
    is_verified: true,
    avatar_url: "https://cdninstagram.com/avatar.jpg",
  },
  recent_posts: [],
  end_cursor: null,
};

const adidasPreview: ProfilePreview = {
  ...preview,
  profile: {
    ...preview.profile,
    pk: "84",
    username: "adidas",
    full_name: "Adidas",
  },
};

function videoPost(pk: string, thumbnail_url: string) {
  return {
    pk,
    code: pk.toUpperCase(),
    caption: pk,
    resources: [{ url: `${thumbnail_url}.mp4`, kind: "video" as const }],
    thumbnail_url,
  };
}

function story(pk: string, media_url: string) {
  return { pk, kind: "photo" as const, media_url };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function render(pinia: Pinia = createPinia()) {
  setActivePinia(pinia);
  return mount(ExplorerView, {
    global: {
      plugins: [pinia],
      stubs: { JobCard: true, PostModal: true },
    },
  });
}

async function loadProfile(
  wrapper: ReturnType<typeof render>,
  value: ProfilePreview = preview,
) {
  ipc.resolveInput.mockResolvedValueOnce({
    kind: "profile",
    username: value.profile.username,
  });
  ipc.fetchProfile.mockResolvedValueOnce(value);
  await wrapper.get("input").setValue(value.profile.username);
  await wrapper.get("form").trigger("submit");
  await flushPromises();
}

function button(wrapper: ReturnType<typeof render>, label: string) {
  const found = wrapper.findAll("button").find((item) => item.text() === label);
  if (!found) throw new Error(`Button not found: ${label}`);
  return found;
}

beforeEach(() => {
  vi.clearAllMocks();
  ipc.fetchStories.mockResolvedValue([]);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("ExplorerView async wiring", () => {
  it("leaves active download rendering to the global application footer", async () => {
    const wrapper = render();
    useJobsStore().addPlaceholder("job-1", "@instagram stories");
    await flushPromises();

    expect(wrapper.find("job-card-stub").exists()).toBe(false);
  });

  it("does not reopen autocomplete after Escape invalidates an in-flight response", async () => {
    vi.useFakeTimers();
    const pending = deferred<Array<{ pk: string; username: string; is_verified: boolean; is_private: boolean }>>();
    ipc.searchUsers.mockReturnValue(pending.promise);
    const wrapper = render();
    const input = wrapper.get("input");
    await input.setValue("nike");
    vi.advanceTimersByTime(250);
    await Promise.resolve();
    expect(ipc.searchUsers).toHaveBeenCalledWith("nike");

    await input.trigger("keydown", { key: "Escape" });
    pending.resolve([{ pk: "1", username: "nike", is_verified: true, is_private: false }]);
    await flushPromises();

    expect(wrapper.findAll("button").some((button) => button.text() === "nike")).toBe(false);
  });

  it("closes old suggestions as soon as a new query intent starts", async () => {
    vi.useFakeTimers();
    ipc.searchUsers.mockResolvedValueOnce([
      { pk: "1", username: "nike", is_verified: true, is_private: false },
    ]);
    ipc.fetchProfile.mockResolvedValue(preview);
    const wrapper = render();
    const input = wrapper.get("input");
    await input.setValue("nike");
    vi.advanceTimersByTime(250);
    await flushPromises();
    expect(wrapper.findAll("button").some((button) => button.text().includes("nike"))).toBe(true);

    await input.setValue("nikex");
    await input.trigger("keydown", { key: "Enter" });

    expect(ipc.fetchProfile).not.toHaveBeenCalled();
    expect(wrapper.findAll("button").some((button) => button.text().includes("nike"))).toBe(false);
  });

  it("does not commit a pending profile after the user edits the query", async () => {
    ipc.resolveInput.mockResolvedValue({ kind: "profile", username: "nike" });
    const pending = deferred<typeof preview>();
    ipc.fetchProfile.mockReturnValue(pending.promise);
    const wrapper = render();
    const input = wrapper.get("input");
    await input.setValue("nike");
    await wrapper.get("form").trigger("submit");
    await input.setValue("adidas");
    pending.resolve(preview);
    await flushPromises();

    expect(wrapper.text()).not.toContain("Nike");
    expect(wrapper.text()).not.toContain("Loading profile");
  });

  it("suppresses duplicate profile actions and releases the busy state after failure", async () => {
    ipc.resolveInput.mockResolvedValue({ kind: "profile", username: "nike" });
    ipc.fetchProfile.mockResolvedValue(preview);
    const pending = deferred<string>();
    ipc.enqueueProfileDownload.mockReturnValueOnce(pending.promise).mockResolvedValueOnce("job-2");
    const wrapper = render();
    await wrapper.get("input").setValue("nike");
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    const download = wrapper.findAll("button").find((button) => button.text() === "Download all posts");
    expect(download).toBeDefined();

    await download!.trigger("click");
    await download!.trigger("click");
    expect(ipc.enqueueProfileDownload).toHaveBeenCalledTimes(1);
    expect(download!.attributes("disabled")).toBeDefined();

    pending.reject(new Error("network"));
    await flushPromises();
    expect(download!.attributes("disabled")).toBeUndefined();
    await download!.trigger("click");
    await flushPromises();
    expect(ipc.enqueueProfileDownload).toHaveBeenCalledTimes(2);
  });

  it("loads exactly one dedicated clips page when Reels is first opened", async () => {
    const feedVideo = videoPost("feed", "https://cdninstagram.com/feed.jpg");
    ipc.fetchReels.mockResolvedValue({
      posts: [
        videoPost("r1", "https://cdninstagram.com/reel-one.jpg"),
        videoPost("r2", "https://cdninstagram.com/reel-two.jpg"),
      ],
      end_cursor: "next",
    });
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: [feedVideo] });

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();

    expect(ipc.fetchReels).toHaveBeenCalledTimes(1);
    expect(ipc.fetchReels).toHaveBeenCalledWith("42", null);
    expect(wrapper.findAll("button.aspect-square img").map((image) => image.attributes("src"))).toEqual([
      "https://cdninstagram.com/reel-one.jpg",
      "https://cdninstagram.com/reel-two.jpg",
    ]);
    expect(wrapper.text()).not.toContain("https://cdninstagram.com/feed.jpg");
  });

  it("auto-loads stories without blocking a public profile or its posts", async () => {
    const pending = deferred<ReturnType<typeof story>[]>();
    ipc.fetchStories.mockReturnValue(pending.promise);
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [videoPost("feed", "https://cdninstagram.com/feed.jpg")],
    });

    expect(ipc.fetchStories).toHaveBeenCalledTimes(1);
    expect(ipc.fetchStories).toHaveBeenCalledWith("nike");
    expect(wrapper.text()).toContain("Nike");
    expect(wrapper.text()).not.toContain("Loading profile");
    expect(wrapper.get("button.aspect-square img").attributes("src")).toBe(
      "https://cdninstagram.com/feed.jpg",
    );
    await wrapper.get("button.aspect-square").trigger("click");
    expect(wrapper.find("post-modal-stub").exists()).toBe(true);

    await button(wrapper, "Stories").trigger("click");
    expect(wrapper.text()).toContain("Loading stories…");
    expect(wrapper.text()).not.toContain("Load stories · costs 2 requests");

    pending.resolve([story("s1", "https://cdninstagram.com/story.jpg")]);
    await flushPromises();
    expect(wrapper.find("img[src='https://cdninstagram.com/story.jpg']").exists()).toBe(true);
  });

  it("keeps the visible profile's pending Stories request alive while the query is edited", async () => {
    const pending = deferred<ReturnType<typeof story>[]>();
    ipc.fetchStories.mockReturnValue(pending.promise);
    const wrapper = render();
    await loadProfile(wrapper);

    await wrapper.get("input").setValue("adidas");
    pending.resolve([story("s1", "https://cdninstagram.com/nike-story.jpg")]);
    await flushPromises();
    await button(wrapper, "Stories").trigger("click");

    expect(ipc.fetchStories).toHaveBeenCalledTimes(1);
    expect(wrapper.text()).toContain("Nike");
    expect(wrapper.find("img[src='https://cdninstagram.com/nike-story.jpg']").exists()).toBe(true);
  });

  it("isolates an automatic stories failure and retries only on request", async () => {
    const retry = deferred<ReturnType<typeof story>[]>();
    ipc.fetchStories
      .mockRejectedValueOnce(new Error("stories unavailable"))
      .mockReturnValueOnce(retry.promise);
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [videoPost("feed", "https://cdninstagram.com/feed.jpg")],
    });

    expect(wrapper.text()).toContain("Nike");
    expect(wrapper.get("button.aspect-square img").attributes("src")).toBe(
      "https://cdninstagram.com/feed.jpg",
    );
    expect(wrapper.text()).not.toContain("stories unavailable");
    expect(ipc.fetchStories).toHaveBeenCalledTimes(1);
    await flushPromises();
    expect(ipc.fetchStories).toHaveBeenCalledTimes(1);

    await button(wrapper, "Stories").trigger("click");
    expect(wrapper.text()).toContain("stories unavailable");
    await button(wrapper, "Retry stories").trigger("click");
    expect(ipc.fetchStories).toHaveBeenCalledTimes(2);
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(1);
    expect(wrapper.text()).not.toContain("stories unavailable");
    expect(wrapper.text()).toContain("Loading stories…");

    retry.resolve([]);
    await flushPromises();
    expect(wrapper.text()).toContain("No active stories.");
  });

  it("keeps an existing stories snapshot visible while retrying", async () => {
    ipc.fetchStories.mockResolvedValueOnce([
      story("s1", "https://cdninstagram.com/existing-story.jpg"),
    ]);
    const wrapper = render();
    await loadProfile(wrapper);
    const store = useExplorerStore();
    const failureToken = store.beginStoriesRequest("nike")!;
    store.failStories("nike", failureToken, "stories unavailable");
    await flushPromises();
    await button(wrapper, "Stories").trigger("click");

    const retry = deferred<ReturnType<typeof story>[]>();
    ipc.fetchStories.mockReturnValueOnce(retry.promise);
    await button(wrapper, "Retry stories").trigger("click");

    expect(wrapper.text()).not.toContain("stories unavailable");
    expect(wrapper.find("img[src='https://cdninstagram.com/existing-story.jpg']").exists()).toBe(true);
    retry.resolve([story("s2", "https://cdninstagram.com/retried-story.jpg")]);
    await flushPromises();
    expect(wrapper.find("img[src='https://cdninstagram.com/existing-story.jpg']").exists()).toBe(false);
    expect(wrapper.find("img[src='https://cdninstagram.com/retried-story.jpg']").exists()).toBe(true);
  });

  it("loads more clips without duplicates and downloads only those shown", async () => {
    ipc.fetchReels
      .mockResolvedValueOnce({
        posts: [
          videoPost("r1", "https://cdninstagram.com/first.jpg"),
          videoPost("r2", "https://cdninstagram.com/second.jpg"),
        ],
        end_cursor: "next",
      })
      .mockResolvedValueOnce({
        posts: [
          videoPost("r2", "https://cdninstagram.com/replacement.jpg"),
          videoPost("r3", "https://cdninstagram.com/third.jpg"),
        ],
        end_cursor: null,
      });
    ipc.enqueueProfileDownload.mockResolvedValue("job-reels");
    const wrapper = render();
    await loadProfile(wrapper);

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();
    await button(wrapper, "Load more").trigger("click");
    await flushPromises();

    expect(wrapper.findAll("button.aspect-square img").map((image) => image.attributes("src"))).toEqual([
      "https://cdninstagram.com/first.jpg",
      "https://cdninstagram.com/second.jpg",
      "https://cdninstagram.com/third.jpg",
    ]);
    expect(ipc.fetchReels).toHaveBeenNthCalledWith(2, "42", "next");

    await button(wrapper, "Download shown (3)").trigger("click");
    await flushPromises();

    expect(ipc.enqueueProfileDownload).toHaveBeenCalledWith("nike", {
      posts: false,
      reels: true,
      stories: false,
      highlights: false,
      avatar: false,
      max_posts: 3,
    });
  });

  it("hides Load more when the clips API repeats the requested cursor", async () => {
    ipc.fetchReels
      .mockResolvedValueOnce({
        posts: [videoPost("r1", "https://cdninstagram.com/first.jpg")],
        end_cursor: "same",
      })
      .mockResolvedValueOnce({
        posts: [videoPost("r2", "https://cdninstagram.com/second.jpg")],
        end_cursor: "same",
      });
    const wrapper = render();
    await loadProfile(wrapper);

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();
    await button(wrapper, "Load more").trigger("click");
    await flushPromises();

    expect(wrapper.findAll("button").some((item) => item.text() === "Load more")).toBe(false);
  });

  it("shows loaded stories only on Stories and never leaks them into empty Reels", async () => {
    ipc.fetchStories.mockResolvedValue([
      story("s1", "https://cdninstagram.com/story.jpg"),
    ]);
    ipc.fetchReels.mockResolvedValue({ posts: [], end_cursor: null });
    const wrapper = render();
    await loadProfile(wrapper);

    await button(wrapper, "Stories").trigger("click");
    expect(wrapper.text()).toContain("Download all stories");
    expect(wrapper.find("img[src='https://cdninstagram.com/story.jpg']").exists()).toBe(true);

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();

    expect(wrapper.text()).not.toContain("Download all stories");
    expect(wrapper.find("img[src='https://cdninstagram.com/story.jpg']").exists()).toBe(false);
    expect(wrapper.text()).toContain("No reels yet.");
  });

  it("does not request stories for a private profile", async () => {
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      profile: { ...preview.profile, is_private: true },
    });

    expect(ipc.fetchStories).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("Private profile — only the avatar is accessible");
  });

  it("rejects a stale stories response after another profile resolves", async () => {
    const nikeStories = deferred<ReturnType<typeof story>[]>();
    ipc.fetchStories
      .mockReturnValueOnce(nikeStories.promise)
      .mockResolvedValueOnce([story("adidas", "https://cdninstagram.com/adidas-story.jpg")]);
    const wrapper = render();
    await loadProfile(wrapper);
    await loadProfile(wrapper, adidasPreview);
    await button(wrapper, "Stories").trigger("click");

    expect(wrapper.find("img[src='https://cdninstagram.com/adidas-story.jpg']").exists()).toBe(true);
    nikeStories.resolve([story("nike", "https://cdninstagram.com/nike-story.jpg")]);
    await flushPromises();

    expect(wrapper.find("img[src='https://cdninstagram.com/nike-story.jpg']").exists()).toBe(false);
    expect(wrapper.find("img[src='https://cdninstagram.com/adidas-story.jpg']").exists()).toBe(true);
  });

  it("uses the stories request generation when the same profile is loaded again", async () => {
    const firstNikeStories = deferred<ReturnType<typeof story>[]>();
    ipc.fetchStories
      .mockReturnValueOnce(firstNikeStories.promise)
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([story("fresh", "https://cdninstagram.com/fresh-story.jpg")]);
    const wrapper = render();
    await loadProfile(wrapper);
    await loadProfile(wrapper, adidasPreview);
    await loadProfile(wrapper);
    await button(wrapper, "Stories").trigger("click");

    expect(wrapper.find("img[src='https://cdninstagram.com/fresh-story.jpg']").exists()).toBe(true);
    firstNikeStories.resolve([story("stale", "https://cdninstagram.com/stale-story.jpg")]);
    await flushPromises();

    expect(wrapper.find("img[src='https://cdninstagram.com/stale-story.jpg']").exists()).toBe(false);
    expect(wrapper.find("img[src='https://cdninstagram.com/fresh-story.jpg']").exists()).toBe(true);
  });

  it("retains the selected profile, Reels tab, and page after remount", async () => {
    const pinia = createPinia();
    ipc.fetchReels.mockResolvedValue({
      posts: [videoPost("r1", "https://cdninstagram.com/reel.jpg")],
      end_cursor: null,
    });
    const first = render(pinia);
    await loadProfile(first);
    await button(first, "Reels").trigger("click");
    await flushPromises();
    first.unmount();

    const second = render(pinia);
    await flushPromises();

    expect(second.text()).toContain("Nike");
    expect(button(second, "Reels").classes()).toContain("bg-surface-3");
    expect(second.findAll("button.aspect-square img")).toHaveLength(1);
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(1);
    expect(ipc.fetchReels).toHaveBeenCalledTimes(1);
  });

  it("auto-loads unresolved stories once when a public profile remounts", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    useExplorerStore().commitProfile(preview);

    const wrapper = render(pinia);
    await flushPromises();

    expect(ipc.fetchStories).toHaveBeenCalledTimes(1);
    expect(ipc.fetchStories).toHaveBeenCalledWith("nike");
    wrapper.unmount();
  });

  it("does not duplicate stories fetching on remount after data or an error resolves", async () => {
    const resolvedPinia = createPinia();
    setActivePinia(resolvedPinia);
    const resolvedStore = useExplorerStore();
    resolvedStore.commitProfile(preview);
    const resolvedToken = resolvedStore.beginStoriesRequest("nike")!;
    resolvedStore.commitStories(
      "nike",
      resolvedToken,
      [story("s1", "https://cdninstagram.com/story.jpg")],
    );
    const resolved = render(resolvedPinia);
    await flushPromises();
    expect(ipc.fetchStories).not.toHaveBeenCalled();
    resolved.unmount();

    const failedPinia = createPinia();
    setActivePinia(failedPinia);
    const failedStore = useExplorerStore();
    failedStore.commitProfile(preview);
    const failedToken = failedStore.beginStoriesRequest("nike")!;
    failedStore.failStories("nike", failedToken, "stories unavailable");
    const failed = render(failedPinia);
    await flushPromises();
    expect(ipc.fetchStories).not.toHaveBeenCalled();
    expect(failed.text()).not.toContain("stories unavailable");
    await button(failed, "Stories").trigger("click");
    expect(failed.text()).toContain("stories unavailable");
  });

  it("preserves one pending Stories request across unmount and remount", async () => {
    const pending = deferred<ReturnType<typeof story>[]>();
    ipc.fetchStories.mockReturnValue(pending.promise);
    const pinia = createPinia();
    const first = render(pinia);
    await loadProfile(first);
    expect(ipc.fetchStories).toHaveBeenCalledTimes(1);

    first.unmount();
    const second = render(pinia);
    await flushPromises();
    await button(second, "Stories").trigger("click");
    expect(second.text()).toContain("Loading stories…");
    expect(ipc.fetchStories).toHaveBeenCalledTimes(1);

    pending.resolve([story("s1", "https://cdninstagram.com/preserved-story.jpg")]);
    await flushPromises();

    expect(second.find("img[src='https://cdninstagram.com/preserved-story.jpg']").exists()).toBe(true);
    expect(useExplorerStore().storiesLoading).toBe(false);
  });

  it("allows retry after the first clips page fails", async () => {
    ipc.fetchReels
      .mockRejectedValueOnce(new Error("clips unavailable"))
      .mockResolvedValueOnce({ posts: [], end_cursor: null });
    const wrapper = render();
    await loadProfile(wrapper);

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("clips unavailable");

    await button(wrapper, "Retry reels").trigger("click");
    await flushPromises();
    expect(ipc.fetchReels).toHaveBeenCalledTimes(2);
  });

  it("does not commit a stale clips response after another profile loads", async () => {
    const pending = deferred<MediaPage>();
    ipc.fetchReels.mockReturnValue(pending.promise);
    const wrapper = render();
    await loadProfile(wrapper);
    await button(wrapper, "Reels").trigger("click");
    expect(ipc.fetchReels).toHaveBeenCalledWith("42", null);

    await loadProfile(wrapper, adidasPreview);
    pending.resolve({
      posts: [videoPost("stale", "https://cdninstagram.com/stale.jpg")],
      end_cursor: null,
    });
    await flushPromises();

    expect(wrapper.text()).toContain("Adidas");
    expect(wrapper.findAll("button.aspect-square img")).toHaveLength(0);
    expect(wrapper.html()).not.toContain("stale.jpg");
  });
});
