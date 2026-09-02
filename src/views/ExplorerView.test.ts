/** @vitest-environment happy-dom */

import { createPinia, setActivePinia, type Pinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createMemoryHistory,
  createRouter,
  type LocationQueryRaw,
  type Router,
} from "vue-router";

const ipc = vi.hoisted(() => ({
  downloadDirect: vi.fn(),
  downloadPost: vi.fn(),
  checkDownloadStatuses: vi.fn(),
  enqueueFetchedPostDownload: vi.fn(),
  enqueueProfileDownload: vi.fn(),
  fetchProfile: vi.fn(),
  fetchReels: vi.fn(),
  fetchStories: vi.fn(),
  remoteMediaUrl: vi.fn((url: string) => `remote-media:${url}`),
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
import DownloadScopeGroup from "../components/DownloadScopeGroup.vue";
import RemoteImage from "../components/RemoteImage.vue";
import type { MediaPage, ProfilePreview } from "../lib/ipc";
import { useExplorerStore } from "../stores/explorer";
import { useJobsStore } from "../stores/jobs";

const wrappers: Array<{ unmount: () => void }> = [];
let router: Router;

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

function photoPost(pk: string, thumbnail_url: string) {
  return {
    pk,
    code: pk.toUpperCase(),
    caption: pk,
    resources: [{ url: `${thumbnail_url}.jpg`, kind: "photo" as const }],
    thumbnail_url,
  };
}

function carouselPost(pk: string, thumbnail_url: string) {
  return {
    pk,
    code: pk.toUpperCase(),
    caption: pk,
    resources: [
      { url: `${thumbnail_url}-one.jpg`, kind: "photo" as const },
      { url: `${thumbnail_url}-two.mp4`, kind: "video" as const },
    ],
    thumbnail_url,
  };
}

function unknownPost(pk: string, thumbnail_url: string) {
  return { pk, code: pk.toUpperCase(), caption: pk, resources: [], thumbnail_url };
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
  const wrapper = mount(ExplorerView, {
    global: {
      plugins: [pinia, router],
      stubs: {
        JobCard: true,
        PostModal: true,
        RouterLink: {
          props: ["to"],
          template: '<a :data-to="JSON.stringify(to)"><slot /></a>',
        },
      },
    },
  });
  wrappers.push(wrapper);
  return wrapper;
}

async function replaceExplorerRoute(query: LocationQueryRaw): Promise<void> {
  await router.replace({ path: "/explore", query });
  await flushPromises();
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
  const download = wrapper
    .find("[role='group'][aria-label='Download']")
    .findAll("button")
    .find((item) => item.text() === label);
  if (download) return download;
  const found = wrapper.findAll("button").find((item) => item.text() === label);
  if (!found) throw new Error(`Button not found: ${label}`);
  return found;
}

function downloadButtons(wrapper: ReturnType<typeof render>) {
  return wrapper.get("[role='group'][aria-label='Download']").findAll("button");
}

function selection(wrapper: ReturnType<typeof render>, label: string) {
  return wrapper.get(`input[type="checkbox"][aria-label="${label}"]`);
}

function finishJob(jobId: string, state: "done" | "failed" | "cancelled" = "done") {
  const job = useJobsStore().jobs.get(jobId);
  if (!job) throw new Error(`Job not found: ${jobId}`);
  job.state = state;
}

beforeEach(async () => {
  vi.resetAllMocks();
  ipc.checkDownloadStatuses.mockResolvedValue([]);
  ipc.remoteMediaUrl.mockImplementation((url: string) => `remote-media:${url}`);
  ipc.fetchStories.mockResolvedValue([]);
  router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/explore", component: { template: "<div />" } }],
  });
  await router.replace("/explore");
  await router.isReady();
});

afterEach(() => {
  for (const wrapper of wrappers.splice(0)) wrapper.unmount();
  document.body.replaceChildren();
  vi.useRealTimers();
  window.history.replaceState({}, "", "/");
});

describe("ExplorerView async wiring", () => {
  it("renders resilient remote images for autocomplete, profile, grid, and story previews", async () => {
    vi.useFakeTimers();
    ipc.searchUsers.mockResolvedValueOnce([
      {
        pk: "search-user",
        username: "nike-search",
        is_verified: false,
        is_private: false,
        avatar_url: "https://cdninstagram.com/search-avatar.jpg",
      },
    ]);
    ipc.fetchStories.mockResolvedValueOnce([{
      ...story("story", "https://cdninstagram.com/story.jpg"),
      thumb_url: "https://cdninstagram.com/story-thumb.jpg",
    }]);
    const post = photoPost("photo", "https://cdninstagram.com/post-thumb");
    const wrapper = render();

    await wrapper.get("input").setValue("nike");
    vi.advanceTimersByTime(250);
    await flushPromises();
    const autocompleteAvatar = wrapper.findAllComponents(RemoteImage).find(
      (image) => image.props("variant") === "compact-avatar",
    );
    expect(autocompleteAvatar?.props()).toMatchObject({
      source: "https://cdninstagram.com/search-avatar.jpg",
      alt: "",
      variant: "compact-avatar",
    });
    expect(autocompleteAvatar?.classes()).toEqual(
      expect.arrayContaining(["h-6", "w-6", "shrink-0", "rounded-full"]),
    );
    expect(wrapper.findAll("img").every(
      (image) => image.element.closest("[data-remote-image]") !== null,
    )).toBe(true);

    vi.useRealTimers();
    await loadProfile(wrapper, { ...preview, recent_posts: [post] });
    const profileAvatar = wrapper.findAllComponents(RemoteImage).find(
      (image) => image.props("variant") === "avatar",
    );
    expect(profileAvatar?.props()).toMatchObject({
      source: preview.profile.avatar_url,
      alt: "@nike profile picture",
      variant: "avatar",
    });
    expect(profileAvatar?.classes()).toEqual(
      expect.arrayContaining(["h-16", "w-16", "shrink-0", "rounded-full"]),
    );
    const thumbnail = wrapper.get('[data-media-id="photo"]').getComponent(RemoteImage);
    expect(thumbnail.props()).toMatchObject({
      source: "https://cdninstagram.com/post-thumb",
      alt: "",
      variant: "thumbnail",
      loading: "lazy",
    });
    expect(thumbnail.classes()).toEqual(
      expect.arrayContaining(["h-full", "w-full", "rounded-lg"]),
    );

    await button(wrapper, "Stories").trigger("click");
    await flushPromises();
    const storyPreview = wrapper.get('[data-story-id="story"]').getComponent(RemoteImage);
    expect(storyPreview.props()).toMatchObject({
      source: "https://cdninstagram.com/story-thumb.jpg",
      alt: "",
      variant: "story",
    });
    expect(storyPreview.classes()).toEqual(
      expect.arrayContaining(["h-20", "w-20", "rounded-full"]),
    );
    expect(wrapper.findAll("img").every(
      (image) => image.element.closest("[data-remote-image]") !== null,
    )).toBe(true);
  });

  it("keeps remote image placeholders mounted when preview sources are missing", async () => {
    vi.useFakeTimers();
    ipc.searchUsers.mockResolvedValueOnce([{
      pk: "no-avatar",
      username: "no-avatar",
      is_verified: false,
      is_private: false,
    }]);
    ipc.fetchStories.mockResolvedValueOnce([story("blank-story", "")]);
    const wrapper = render();

    await wrapper.get("input").setValue("no-avatar");
    vi.advanceTimersByTime(250);
    await flushPromises();
    const autocompleteAvatar = wrapper.findAllComponents(RemoteImage).find(
      (image) => image.props("variant") === "compact-avatar",
    );
    expect(autocompleteAvatar).toBeDefined();
    expect(autocompleteAvatar?.props("source")).toBeNull();

    vi.useRealTimers();
    await loadProfile(wrapper, {
      ...preview,
      profile: { ...preview.profile, avatar_url: undefined },
      recent_posts: [unknownPost("blank-post", "")],
    });
    expect(wrapper.findAllComponents(RemoteImage).some(
      (image) => image.props("variant") === "avatar" && image.props("source") === null,
    )).toBe(true);
    expect(wrapper.get('[data-media-id="blank-post"]').getComponent(RemoteImage).props()).toMatchObject({
      source: "",
      variant: "thumbnail",
    });

    await button(wrapper, "Stories").trigger("click");
    await flushPromises();
    expect(wrapper.get('[data-story-id="blank-story"]').getComponent(RemoteImage).props()).toMatchObject({
      source: "",
      variant: "story",
    });
  });

  it("keeps autocomplete keyboard navigation selecting the highlighted profile", async () => {
    vi.useFakeTimers();
    ipc.searchUsers.mockResolvedValueOnce([
      { pk: "1", username: "nike", is_verified: true, is_private: false },
      { pk: "2", username: "adidas", is_verified: false, is_private: false },
    ]);
    ipc.fetchProfile.mockResolvedValueOnce(adidasPreview);
    const wrapper = render();
    const input = wrapper.get("input");

    await input.setValue("sportswear");
    vi.advanceTimersByTime(250);
    await flushPromises();
    await input.trigger("keydown", { key: "ArrowDown" });
    await input.trigger("keydown", { key: "Enter" });
    await flushPromises();

    expect(ipc.fetchProfile).toHaveBeenCalledWith("adidas", null);
    expect(input.element.value).toBe("@adidas");
    expect(wrapper.text()).toContain("Adidas");
  });

  it("links public profile counts to Followers and Following pages", async () => {
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      profile: {
        ...preview.profile,
        follower_count: 291_000_000,
        following_count: 234,
      },
    });

    expect(wrapper.get('[data-relationship="followers"]').attributes("data-to")).toContain(
      "/explore/nike/followers",
    );
    expect(wrapper.get('[data-relationship="followers"]').text()).toContain("291M followers");
    expect(wrapper.get('[data-relationship="following"]').attributes("data-to")).toContain(
      "/explore/nike/following",
    );
    expect(wrapper.get('[data-relationship="following"]').text()).toContain("234 following");
  });

  it("loads a profile requested by a relationship-result deep link before a demo default", async () => {
    await replaceExplorerRoute({ profile: "adidas" });
    window.history.replaceState(
      {},
      "",
      "/explore?demo=remote-media-failure",
    );
    ipc.fetchProfile.mockResolvedValueOnce(adidasPreview);
    const wrapper = render();
    await flushPromises();

    expect(ipc.fetchProfile).toHaveBeenCalledWith("adidas", null);
    expect(wrapper.get("input").element.value).toBe("@adidas");
    expect(wrapper.text()).toContain("Adidas");
  });

  it("opens a mentioned profile and restores profiles through Back and Forward", async () => {
    const nikeWithPost: ProfilePreview = {
      ...preview,
      recent_posts: [photoPost("post", "https://cdninstagram.com/post")],
    };
    await replaceExplorerRoute({ profile: "nike" });
    ipc.fetchProfile
      .mockResolvedValueOnce(nikeWithPost)
      .mockResolvedValueOnce(adidasPreview)
      .mockResolvedValueOnce(nikeWithPost)
      .mockResolvedValueOnce(adidasPreview);
    const wrapper = render();
    await flushPromises();
    const replace = vi.spyOn(router, "replace");

    expect(wrapper.text()).toContain("Nike");
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(1);
    await wrapper.get('[data-media-id="post"] [data-action="preview"]').trigger("click");
    wrapper.getComponent({ name: "PostModal" }).vm.$emit("open-profile", "adidas");
    await flushPromises();

    expect(wrapper.findComponent({ name: "PostModal" }).exists()).toBe(false);
    expect(router.currentRoute.value.query.profile).toBe("adidas");
    expect(ipc.fetchProfile).toHaveBeenLastCalledWith("adidas", null);
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(2);
    expect(wrapper.text()).toContain("Adidas");
    expect(replace).not.toHaveBeenCalled();

    router.back();
    await flushPromises();
    expect(router.currentRoute.value.query.profile).toBe("nike");
    expect(wrapper.text()).toContain("Nike");
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(3);

    router.forward();
    await flushPromises();
    expect(router.currentRoute.value.query.profile).toBe("adidas");
    expect(wrapper.text()).toContain("Adidas");
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(4);
  });

  it("records a noncanonical current profile before opening a mentioned profile", async () => {
    const nikeWithPost: ProfilePreview = {
      ...preview,
      recent_posts: [photoPost("post", "https://cdninstagram.com/post")],
    };
    const wrapper = render();
    await loadProfile(wrapper, nikeWithPost);
    const replace = vi.spyOn(router, "replace");
    const push = vi.spyOn(router, "push");
    ipc.fetchProfile
      .mockResolvedValueOnce(adidasPreview)
      .mockResolvedValueOnce(nikeWithPost);
    await wrapper.get('[data-media-id="post"] [data-action="preview"]').trigger("click");

    wrapper.getComponent({ name: "PostModal" }).vm.$emit("open-profile", "adidas");
    await flushPromises();

    expect(replace).toHaveBeenCalledTimes(1);
    expect(replace).toHaveBeenCalledWith({
      path: "/explore",
      query: { profile: "nike" },
    });
    expect(push).toHaveBeenCalledTimes(1);
    expect(push).toHaveBeenCalledWith({
      path: "/explore",
      query: { profile: "adidas" },
    });
    expect(router.currentRoute.value.query.profile).toBe("adidas");
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(2);
    expect(wrapper.text()).toContain("Adidas");

    router.back();
    await flushPromises();
    expect(router.currentRoute.value.query.profile).toBe("nike");
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(3);
    expect(wrapper.text()).toContain("Nike");
  });

  it("shows a missing mentioned profile error and Back restores the source profile", async () => {
    const nikeWithPost: ProfilePreview = {
      ...preview,
      recent_posts: [photoPost("post", "https://cdninstagram.com/post")],
    };
    await replaceExplorerRoute({ profile: "nike" });
    ipc.fetchProfile
      .mockResolvedValueOnce(nikeWithPost)
      .mockRejectedValueOnce(new Error("Profile not found"))
      .mockResolvedValueOnce(nikeWithPost);
    const wrapper = render();
    await flushPromises();
    await wrapper.get('[data-media-id="post"] [data-action="preview"]').trigger("click");

    wrapper.getComponent({ name: "PostModal" }).vm.$emit("open-profile", "missing_user");
    await flushPromises();

    expect(router.currentRoute.value.query.profile).toBe("missing_user");
    expect(wrapper.text()).toContain("Profile not found");

    router.back();
    await flushPromises();
    expect(router.currentRoute.value.query.profile).toBe("nike");
    expect(wrapper.text()).toContain("Nike");
    expect(wrapper.text()).not.toContain("Profile not found");
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(3);
  });

  it("keeps Back on the source profile when the mentioned profile resolves late", async () => {
    const nikeWithPost: ProfilePreview = {
      ...preview,
      recent_posts: [photoPost("post", "https://cdninstagram.com/post")],
    };
    const pendingAdidas = deferred<ProfilePreview>();
    await replaceExplorerRoute({ profile: "nike" });
    ipc.fetchProfile
      .mockResolvedValueOnce(nikeWithPost)
      .mockReturnValueOnce(pendingAdidas.promise)
      .mockResolvedValueOnce(nikeWithPost);
    const wrapper = render();
    await flushPromises();
    await wrapper.get('[data-media-id="post"] [data-action="preview"]').trigger("click");

    wrapper.getComponent({ name: "PostModal" }).vm.$emit("open-profile", "adidas");
    await flushPromises();
    expect(router.currentRoute.value.query.profile).toBe("adidas");
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(2);

    router.back();
    await flushPromises();
    expect(router.currentRoute.value.query.profile).toBe("nike");
    expect(wrapper.text()).toContain("Nike");
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(3);

    pendingAdidas.resolve(adidasPreview);
    await flushPromises();
    expect(router.currentRoute.value.query.profile).toBe("nike");
    expect(wrapper.text()).toContain("Nike");
    expect(wrapper.text()).not.toContain("Adidas");
    expect(wrapper.text()).not.toContain("Profile not found");
  });

  it("rejects a stale profile response from an older route query", async () => {
    const stale = deferred<ProfilePreview>();
    const pumaPreview: ProfilePreview = {
      ...preview,
      profile: {
        ...preview.profile,
        pk: "126",
        username: "puma",
        full_name: "Puma",
      },
    };
    await replaceExplorerRoute({ profile: "nike" });
    ipc.fetchProfile
      .mockResolvedValueOnce(preview)
      .mockReturnValueOnce(stale.promise)
      .mockResolvedValueOnce(pumaPreview);
    const wrapper = render();
    await flushPromises();

    await router.push({ path: "/explore", query: { profile: "adidas" } });
    await flushPromises();
    await router.push({ path: "/explore", query: { profile: "puma" } });
    await flushPromises();
    expect(wrapper.text()).toContain("Puma");

    stale.resolve(adidasPreview);
    await flushPromises();
    expect(wrapper.text()).toContain("Puma");
    expect(wrapper.text()).not.toContain("Adidas");
  });

  it("does not surface a stale profile rejection after a newer route succeeds", async () => {
    const stale = deferred<ProfilePreview>();
    const pumaPreview: ProfilePreview = {
      ...preview,
      profile: {
        ...preview.profile,
        pk: "126",
        username: "puma",
        full_name: "Puma",
      },
    };
    await replaceExplorerRoute({ profile: "nike" });
    ipc.fetchProfile
      .mockResolvedValueOnce(preview)
      .mockReturnValueOnce(stale.promise)
      .mockResolvedValueOnce(pumaPreview);
    const wrapper = render();
    await flushPromises();

    await router.push({ path: "/explore", query: { profile: "adidas" } });
    await flushPromises();
    await router.push({ path: "/explore", query: { profile: "puma" } });
    await flushPromises();
    expect(wrapper.text()).toContain("Puma");

    stale.reject(new Error("Stale Adidas failure"));
    await flushPromises();
    expect(router.currentRoute.value.query.profile).toBe("puma");
    expect(wrapper.text()).toContain("Puma");
    expect(wrapper.text()).not.toContain("Stale Adidas failure");
  });

  it("closes the modal without navigation or refetch for the current profile", async () => {
    const nikeWithPost: ProfilePreview = {
      ...preview,
      recent_posts: [photoPost("post", "https://cdninstagram.com/post")],
    };
    await replaceExplorerRoute({ profile: "nike" });
    ipc.fetchProfile.mockResolvedValueOnce(nikeWithPost);
    const wrapper = render();
    await flushPromises();
    const push = vi.spyOn(router, "push");
    const replace = vi.spyOn(router, "replace");
    await wrapper.get('[data-media-id="post"] [data-action="preview"]').trigger("click");

    wrapper.getComponent({ name: "PostModal" }).vm.$emit("open-profile", "NIKE");
    await flushPromises();

    expect(wrapper.findComponent({ name: "PostModal" }).exists()).toBe(false);
    expect(router.currentRoute.value.query.profile).toBe("nike");
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(1);
    expect(push).not.toHaveBeenCalled();
    expect(replace).not.toHaveBeenCalled();
  });

  it("auto-loads the neutral profile for the remote-media failure demo", async () => {
    window.history.replaceState({}, "", "/explore?mock=1&demo=remote-media-failure");
    ipc.fetchProfile.mockResolvedValueOnce({
      ...preview,
      profile: {
        ...preview.profile,
        username: "preview_demo",
        full_name: "Preview Demo",
      },
    });
    const wrapper = render();
    await flushPromises();

    expect(ipc.fetchProfile).toHaveBeenCalledWith("preview_demo", null);
    expect(wrapper.get("input").element.value).toBe("@preview_demo");
  });

  it("keeps the healthy Explore demo defaulting to natgeo", async () => {
    window.history.replaceState({}, "", "/explore?mock=1&demo=explore");
    ipc.fetchProfile.mockResolvedValueOnce(preview);
    const wrapper = render();
    await flushPromises();

    expect(ipc.fetchProfile).toHaveBeenCalledWith("natgeo", null);
    expect(wrapper.get("input").element.value).toBe("@natgeo");
  });

  it("filters loaded Posts for the grid and Shown download while preserving hidden selected posts", async () => {
    const photo = photoPost("photo", "https://cdninstagram.com/photo");
    const video = videoPost("video", "https://cdninstagram.com/video");
    const carousel = carouselPost("carousel", "https://cdninstagram.com/carousel");
    const unknown = unknownPost("unknown", "https://cdninstagram.com/unknown");
    ipc.enqueueFetchedPostDownload
      .mockResolvedValueOnce("job-shown-videos")
      .mockResolvedValueOnce("job-selected-filtered");
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [photo, video, carousel, unknown],
    });

    expect(wrapper.findAll("[data-media-id]").map((item) => item.attributes("data-media-id"))).toEqual([
      "photo",
      "video",
      "carousel",
      "unknown",
    ]);
    expect(button(wrapper, "Shown 3").exists()).toBe(true);

    await wrapper.get('[data-post-filter="videos"]').trigger("click");
    expect(wrapper.get('[data-post-filter="videos"]').attributes("aria-pressed")).toBe("true");
    expect(wrapper.get('[data-post-filter="videos"]').attributes("aria-current")).toBe("true");
    expect(wrapper.get('[data-post-filter="all"]').attributes("aria-pressed")).toBe("false");
    expect(wrapper.findAll("[data-media-id]").map((item) => item.attributes("data-media-id"))).toEqual([
      "video",
    ]);
    expect(button(wrapper, "Shown 1").exists()).toBe(true);
    await button(wrapper, "Shown 1").trigger("click");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenNthCalledWith(
      1,
      "nike",
      "posts",
      "shown",
      [video],
    );
    finishJob("job-shown-videos");

    await wrapper.get('[data-post-filter="photos"]').trigger("click");
    await selection(wrapper, "Select post PHOTO").setValue(true);
    await wrapper.get('[data-post-filter="videos"]').trigger("click");
    await selection(wrapper, "Select post VIDEO").setValue(true);
    expect(button(wrapper, "Selected 2").exists()).toBe(true);
    await button(wrapper, "Selected 2").trigger("click");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenNthCalledWith(
      2,
      "nike",
      "posts",
      "selected",
      [photo, video],
    );

    await wrapper.get('[data-post-filter="carousels"]').trigger("click");
    expect(wrapper.findAll("[data-media-id]").map((item) => item.attributes("data-media-id"))).toEqual([
      "carousel",
    ]);
    expect(button(wrapper, "Shown 1").exists()).toBe(true);
    await wrapper.get('[data-post-filter="photos"]').trigger("click");
    expect(wrapper.findAll("[data-media-id]").map((item) => item.attributes("data-media-id"))).toEqual([
      "photo",
    ]);
    expect(button(wrapper, "Shown 1").exists()).toBe(true);
    await wrapper.get('[data-post-filter="all"]').trigger("click");
    expect(wrapper.findAll("[data-media-id]").map((item) => item.attributes("data-media-id"))).toEqual([
      "photo",
      "video",
      "carousel",
      "unknown",
    ]);
    expect(button(wrapper, "Shown 3").exists()).toBe(true);
    finishJob("job-selected-filtered");
    await flushPromises();
  });

  it("keeps resource-less Posts visible but excludes them from exact downloads and selection", async () => {
    const photo = photoPost("photo", "https://cdninstagram.com/photo");
    const video = videoPost("video", "https://cdninstagram.com/video");
    const carousel = carouselPost("carousel", "https://cdninstagram.com/carousel");
    const unknown = unknownPost("unknown", "https://cdninstagram.com/unknown");
    ipc.enqueueFetchedPostDownload
      .mockResolvedValueOnce("job-valid-shown")
      .mockResolvedValueOnce("job-valid-selected");
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [photo, video, carousel, unknown],
    });

    expect(wrapper.findAll("[data-media-id]")).toHaveLength(4);
    expect(button(wrapper, "Shown 3").exists()).toBe(true);
    const unavailableTile = wrapper.get('[data-media-id="unknown"]');
    const unavailableInput = selection(wrapper, "Select post UNKNOWN");
    expect(unavailableTile.get("[data-download-unavailable]").text()).toBe("Unavailable");
    expect(unavailableInput.attributes("disabled")).toBeDefined();
    expect(unavailableInput.attributes("aria-describedby")).toBeTruthy();
    await unavailableInput.trigger("change");
    expect(useExplorerStore().isSelected("posts", "unknown")).toBe(false);

    await button(wrapper, "Shown 3").trigger("click");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenNthCalledWith(
      1,
      "nike",
      "posts",
      "shown",
      [photo, video, carousel],
    );
    finishJob("job-valid-shown");
    await flushPromises();

    const store = useExplorerStore();
    store.selected.posts.push("unknown");
    await selection(wrapper, "Select post PHOTO").setValue(true);
    expect(button(wrapper, "Selected 1").exists()).toBe(true);
    await button(wrapper, "Selected 1").trigger("click");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenNthCalledWith(
      2,
      "nike",
      "posts",
      "selected",
      [photo],
    );
  });

  it("applies the downloadable exact-snapshot predicate to Reels", async () => {
    const reel = videoPost("reel", "https://cdninstagram.com/reel");
    const unavailable = unknownPost("unknown-reel", "https://cdninstagram.com/unknown-reel");
    ipc.fetchReels.mockResolvedValue({ posts: [reel, unavailable], end_cursor: null });
    ipc.enqueueFetchedPostDownload.mockResolvedValue("job-reels-shown");
    const wrapper = render();
    await loadProfile(wrapper);

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();

    expect(wrapper.findAll("[data-media-id]")).toHaveLength(2);
    expect(button(wrapper, "Shown 1").exists()).toBe(true);
    expect(selection(wrapper, "Select reel UNKNOWN-REEL").attributes("disabled")).toBeDefined();
    useExplorerStore().selected.reels.push("unknown-reel");
    expect(button(wrapper, "Selected 0").attributes("disabled")).toBeDefined();

    await button(wrapper, "Shown 1").trigger("click");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledWith(
      "nike",
      "reels",
      "shown",
      [reel],
    );
  });

  it("keeps loading more available for an empty Posts filter and hides the filter outside Posts", async () => {
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [videoPost("video", "https://cdninstagram.com/video")],
      end_cursor: "next-page",
    });

    await wrapper.get('[data-post-filter="photos"]').trigger("click");
    expect(wrapper.text()).toContain("No photos in 1 loaded post.");
    expect(button(wrapper, "Load more").exists()).toBe(true);

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();
    expect(wrapper.find("[data-post-filter]").exists()).toBe(false);
    await button(wrapper, "Stories").trigger("click");
    expect(wrapper.find("[data-post-filter]").exists()).toBe(false);
  });

  it("shows the current Posts page and total pages calculated from the profile count", async () => {
    const firstPage = Array.from({ length: 12 }, (_, index) =>
      photoPost(`p${index}`, `https://cdninstagram.com/p${index}`),
    );
    const secondPage = Array.from({ length: 12 }, (_, index) =>
      photoPost(`p${index + 12}`, `https://cdninstagram.com/p${index + 12}`),
    );
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      profile: { ...preview.profile, media_count: 25 },
      recent_posts: firstPage,
      end_cursor: "posts-page-2",
    });

    expect(wrapper.get("[data-pagination-status]").text()).toBe("Page 1 of 3");
    ipc.fetchProfile.mockResolvedValueOnce({
      ...preview,
      profile: { ...preview.profile, media_count: 25 },
      recent_posts: secondPage,
      end_cursor: "posts-page-3",
    });
    await button(wrapper, "Load more").trigger("click");
    await flushPromises();

    expect(wrapper.get("[data-pagination-status]").text()).toBe("Page 2 of 3");
  });

  it("shows only the current Reels page because the clips API has no reels total", async () => {
    ipc.fetchReels
      .mockResolvedValueOnce({
        posts: [videoPost("r1", "https://cdninstagram.com/r1")],
        end_cursor: "reels-page-2",
      })
      .mockResolvedValueOnce({
        posts: [videoPost("r2", "https://cdninstagram.com/r2")],
        end_cursor: null,
      });
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      profile: { ...preview.profile, media_count: 25 },
    });

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-pagination-status]").text()).toBe("Page 1");

    await button(wrapper, "Load more").trigger("click");
    await flushPromises();
    expect(wrapper.get("[data-pagination-status]").text()).toBe("Page 2");
  });

  it("uses a filter-specific empty state for every empty Posts media filter", async () => {
    const cases = [
      {
        filter: "photos",
        label: "photos",
        posts: [videoPost("video", "https://cdninstagram.com/video")],
      },
      {
        filter: "videos",
        label: "videos",
        posts: [photoPost("photo", "https://cdninstagram.com/photo")],
      },
      {
        filter: "carousels",
        label: "carousels",
        posts: [photoPost("photo", "https://cdninstagram.com/photo")],
      },
    ] as const;

    for (const entry of cases) {
      const wrapper = render();
      await loadProfile(wrapper, {
        ...preview,
        recent_posts: [...entry.posts],
        end_cursor: `${entry.filter}-next-page`,
      });

      await wrapper.get(`[data-post-filter="${entry.filter}"]`).trigger("click");
      expect(wrapper.findAll("[data-media-id]")).toHaveLength(0);
      expect(wrapper.text()).toContain(`No ${entry.label} in 1 loaded post.`);
      expect(wrapper.text()).not.toContain("No posts yet.");
      expect(button(wrapper, "Shown 0").attributes("disabled")).toBeDefined();
      expect(button(wrapper, "Load more").exists()).toBe(true);
      wrapper.unmount();
    }
  });

  it("persists the active Posts filter and filtered grid across an Explorer remount", async () => {
    const pinia = createPinia();
    const photo = photoPost("photo", "https://cdninstagram.com/photo");
    const video = videoPost("video", "https://cdninstagram.com/video");
    const first = render(pinia);
    await loadProfile(first, { ...preview, recent_posts: [photo, video] });

    await first.get('[data-post-filter="videos"]').trigger("click");
    first.unmount();

    const second = render(pinia);
    expect(second.get('[data-post-filter="videos"]').attributes("aria-pressed")).toBe("true");
    expect(second.get('[data-post-filter="videos"]').attributes("aria-current")).toBe("true");
    expect(second.findAll("[data-media-id]").map((item) => item.attributes("data-media-id"))).toEqual([
      "video",
    ]);
  });

  it("shows media badges and descriptive preview labels for Posts and Reels", async () => {
    const photo = photoPost("photo", "https://cdninstagram.com/photo");
    const video = videoPost("video", "https://cdninstagram.com/video");
    const carousel = carouselPost("carousel", "https://cdninstagram.com/carousel");
    const unknown = unknownPost("unknown", "https://cdninstagram.com/unknown");
    const reel = carouselPost("reel", "https://cdninstagram.com/reel");
    ipc.fetchReels.mockResolvedValue({ posts: [reel], end_cursor: null });
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: [photo, video, carousel, unknown] });

    expect(wrapper.get('[data-media-id="photo"] [role="img"]').text()).toBe("PHOTO");
    expect(wrapper.get('[data-media-id="video"] [role="img"]').text()).toBe("VIDEO");
    expect(wrapper.get('[data-media-id="carousel"] [role="img"]').text()).toBe("CAROUSEL · 2");
    expect(wrapper.get('[data-media-id="unknown"] [role="img"]').text()).toBe("POST");
    expect(wrapper.get('[data-media-id="carousel"] [data-action="preview"]').attributes("aria-label")).toBe(
      "Preview carousel with 2 resources CAROUSEL",
    );
    expect(wrapper.get('[data-media-id="unknown"] [data-action="preview"]').attributes("aria-label")).toBe(
      "Preview post UNKNOWN",
    );

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();
    expect(wrapper.get('[data-media-id="reel"] [role="img"]').text()).toBe("CAROUSEL · 2");
    expect(wrapper.get('[data-media-id="reel"] [data-action="preview"]').attributes("aria-label")).toBe(
      "Preview carousel with 2 resources REEL",
    );
  });

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

  it("keeps stable download scopes in Posts, Reels, and Stories", async () => {
    const post = videoPost("p1", "https://cdninstagram.com/post.jpg");
    ipc.fetchReels.mockResolvedValue({
      posts: [videoPost("r1", "https://cdninstagram.com/reel.jpg")],
      end_cursor: null,
    });
    ipc.fetchStories.mockResolvedValue([
      story("s1", "https://cdninstagram.com/story.jpg"),
    ]);
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: [post] });

    expect(downloadButtons(wrapper).map((item) => item.text())).toEqual([
      "All",
      "Shown 1",
      "Selected 0",
    ]);
    expect(button(wrapper, "All").attributes("title")).toMatch(/complete Posts archive.*API requests/i);
    const scopeHelp = wrapper.get('[data-action="scope-help"]');
    expect(scopeHelp.attributes("aria-expanded")).toBe("false");
    expect(wrapper.text()).not.toContain("complete category archive");
    await scopeHelp.trigger("click");
    expect(scopeHelp.attributes("aria-expanded")).toBe("true");
    expect(wrapper.text()).toContain("complete category archive");
    expect(button(wrapper, "Selected 0").attributes("disabled")).toBeDefined();

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();
    expect(downloadButtons(wrapper).map((item) => item.text())).toEqual([
      "All",
      "Shown 1",
      "Selected 0",
    ]);
    expect(button(wrapper, "Selected 0").attributes("disabled")).toBeDefined();

    await button(wrapper, "Stories").trigger("click");
    expect(downloadButtons(wrapper).map((item) => item.text())).toEqual([
      "All",
      "Shown 1",
      "Selected 0",
    ]);
    expect(button(wrapper, "Selected 0").attributes("disabled")).toBeDefined();
    expect(button(wrapper, "All").attributes("title")).toMatch(
      /refreshes.*all current Stories.*additional API requests/i,
    );
    expect(wrapper.text()).not.toContain("Download all posts");
    expect(wrapper.text()).not.toContain("Download shown (");
    expect(wrapper.text()).not.toContain("Download all stories");
  });

  it("keeps tabs and downloads on the first row with a clean Posts filter row below", async () => {
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [photoPost("photo", "https://cdninstagram.com/photo")],
    });

    const toolbar = wrapper.get("[data-explorer-toolbar]");
    expect(toolbar.find('[data-explore-tabs]').exists()).toBe(true);
    expect(toolbar.find('[aria-label="Download"]').exists()).toBe(true);
    expect(toolbar.find('[aria-label="Posts filter"]').exists()).toBe(false);

    const filterRow = wrapper.get("[data-post-filter-row]");
    const filters = filterRow.findAll("button");
    expect(filterRow.find('[aria-label="Posts filter"]').exists()).toBe(true);
    expect(filters).toHaveLength(4);
    expect(filters.every((filter) => filter.classes().includes("border-l"))).toBe(true);
    expect(filters[0].classes()).toContain("shadow-[inset_0_-2px_0_var(--color-accent)]");
    expect(filters[0].classes()).not.toContain("ring-1");
  });

  it("suppresses duplicate group actions, disables the group, and retries after failure", async () => {
    const pending = deferred<string>();
    ipc.enqueueProfileDownload.mockReturnValueOnce(pending.promise).mockResolvedValueOnce("job-2");
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });
    const download = button(wrapper, "All");

    await download.trigger("click");
    await download.trigger("click");
    expect(ipc.enqueueProfileDownload).toHaveBeenCalledTimes(1);
    expect(downloadButtons(wrapper).every((item) => item.attributes("disabled") !== undefined)).toBe(true);

    pending.reject(new Error("network"));
    await flushPromises();
    expect(wrapper.text()).toContain("Error: network");
    expect(button(wrapper, "All").attributes("disabled")).toBeUndefined();
    expect(button(wrapper, "Shown 1").attributes("disabled")).toBeUndefined();
    await button(wrapper, "All").trigger("click");
    await flushPromises();
    expect(ipc.enqueueProfileDownload).toHaveBeenCalledTimes(2);
  });

  it("keeps an accepted snapshot busy until its queued job becomes terminal", async () => {
    ipc.enqueueFetchedPostDownload
      .mockResolvedValueOnce("job-shown")
      .mockResolvedValueOnce("job-shown-retry");
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });

    await button(wrapper, "Shown 1").trigger("click");
    await flushPromises();

    expect(useJobsStore().jobs.get("job-shown")?.conflictKeys).toEqual([
      "folder:nike:posts",
    ]);
    expect(downloadButtons(wrapper).every((item) => item.attributes("disabled") !== undefined)).toBe(true);
    await button(wrapper, "Shown 1").trigger("click");
    wrapper.getComponent(DownloadScopeGroup).vm.$emit("download-shown");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledTimes(1);

    finishJob("job-shown");
    await flushPromises();
    expect(button(wrapper, "Shown 1").attributes("disabled")).toBeUndefined();
    await button(wrapper, "Shown 1").trigger("click");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledTimes(2);
  });

  it("treats Posts and Reels snapshots as the same physical-folder conflict", async () => {
    ipc.enqueueFetchedPostDownload.mockResolvedValue("job-post-folder");
    ipc.fetchReels.mockResolvedValue({
      posts: [videoPost("r1", "https://cdninstagram.com/reel.jpg")],
      end_cursor: null,
    });
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });
    await button(wrapper, "Shown 1").trigger("click");
    await flushPromises();

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();
    expect(downloadButtons(wrapper).every((item) => item.attributes("disabled") !== undefined)).toBe(true);
    wrapper.getComponent(DownloadScopeGroup).vm.$emit("download-all");
    wrapper.getComponent(DownloadScopeGroup).vm.$emit("download-shown");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledTimes(1);
    expect(ipc.enqueueProfileDownload).not.toHaveBeenCalled();

    finishJob("job-post-folder");
    await flushPromises();
    expect(button(wrapper, "All").attributes("disabled")).toBeUndefined();
  });

  it("uses the profile conflict from All to block other tabs", async () => {
    ipc.enqueueProfileDownload.mockResolvedValue("job-all-posts");
    ipc.fetchStories.mockResolvedValue([
      story("s1", "https://cdninstagram.com/story.jpg"),
    ]);
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });
    await button(wrapper, "All").trigger("click");
    await flushPromises();

    expect(useJobsStore().jobs.get("job-all-posts")?.conflictKeys).toEqual([
      "profile:nike",
      "folder:nike:posts",
    ]);
    await button(wrapper, "Stories").trigger("click");
    expect(downloadButtons(wrapper).every((item) => item.attributes("disabled") !== undefined)).toBe(true);
    wrapper.getComponent(DownloadScopeGroup).vm.$emit("download-all");
    wrapper.getComponent(DownloadScopeGroup).vm.$emit("download-shown");
    await flushPromises();
    expect(ipc.enqueueProfileDownload).toHaveBeenCalledTimes(1);
    expect(ipc.downloadDirect).not.toHaveBeenCalled();

    finishJob("job-all-posts");
    await flushPromises();
    expect(button(wrapper, "All").attributes("disabled")).toBeUndefined();
  });

  it("retains accepted job conflicts across an Explore remount", async () => {
    const pinia = createPinia();
    ipc.enqueueFetchedPostDownload.mockResolvedValue("job-remount");
    const first = render(pinia);
    await loadProfile(first, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });
    await button(first, "Shown 1").trigger("click");
    await flushPromises();
    first.unmount();

    const second = render(pinia);
    await flushPromises();
    expect(downloadButtons(second).every((item) => item.attributes("disabled") !== undefined)).toBe(true);
    finishJob("job-remount");
    await flushPromises();
    expect(button(second, "Shown 1").attributes("disabled")).toBeUndefined();
  });

  it("retains a pending enqueue reservation across remount and transfers it to the accepted job", async () => {
    const pinia = createPinia();
    const pending = deferred<string>();
    ipc.enqueueFetchedPostDownload.mockReturnValue(pending.promise);
    const first = render(pinia);
    await loadProfile(first, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });
    await button(first, "Shown 1").trigger("click");
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledTimes(1);
    first.unmount();

    const second = render(pinia);
    await flushPromises();
    expect(downloadButtons(second).every((item) => item.attributes("disabled") !== undefined)).toBe(true);
    second.getComponent(DownloadScopeGroup).vm.$emit("download-shown");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledTimes(1);

    pending.resolve("job-pending-remount");
    await flushPromises();
    expect(useJobsStore().jobs.get("job-pending-remount")?.conflictKeys).toEqual([
      "folder:nike:posts",
    ]);
    expect(downloadButtons(second).every((item) => item.attributes("disabled") !== undefined)).toBe(true);

    finishJob("job-pending-remount");
    await flushPromises();
    expect(button(second, "Shown 1").attributes("disabled")).toBeUndefined();
  });

  it("releases a remounted pending reservation after enqueue failure and allows retry", async () => {
    const pinia = createPinia();
    const pending = deferred<string>();
    ipc.enqueueFetchedPostDownload
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValueOnce("job-after-error");
    const first = render(pinia);
    await loadProfile(first, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });
    await button(first, "Shown 1").trigger("click");
    first.unmount();

    const second = render(pinia);
    await flushPromises();
    expect(button(second, "Shown 1").attributes("disabled")).toBeDefined();
    pending.reject(new Error("enqueue failed"));
    await flushPromises();

    expect(button(second, "Shown 1").attributes("disabled")).toBeUndefined();
    await button(second, "Shown 1").trigger("click");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledTimes(2);
  });

  it("maps All to an unlimited archive request for the active tab", async () => {
    ipc.enqueueProfileDownload
      .mockResolvedValueOnce("job-all-posts")
      .mockResolvedValueOnce("job-all-reels")
      .mockResolvedValueOnce("job-all-stories");
    ipc.fetchReels.mockResolvedValue({
      posts: [videoPost("r1", "https://cdninstagram.com/reel.jpg")],
      end_cursor: "next",
    });
    ipc.fetchStories.mockResolvedValue([
      story("s1", "https://cdninstagram.com/story.jpg"),
    ]);
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });

    await button(wrapper, "All").trigger("click");
    await flushPromises();
    expect(ipc.enqueueProfileDownload).toHaveBeenNthCalledWith(1, "nike", {
      posts: true,
      reels: false,
      stories: false,
      highlights: false,
      avatar: false,
      max_posts: null,
    });
    finishJob("job-all-posts");
    await flushPromises();

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();
    await button(wrapper, "All").trigger("click");
    await flushPromises();
    expect(ipc.enqueueProfileDownload).toHaveBeenNthCalledWith(2, "nike", {
      posts: false,
      reels: true,
      stories: false,
      highlights: false,
      avatar: false,
      max_posts: null,
    });
    finishJob("job-all-reels");
    await flushPromises();

    await button(wrapper, "Stories").trigger("click");
    await button(wrapper, "All").trigger("click");
    await flushPromises();
    expect(ipc.enqueueProfileDownload).toHaveBeenNthCalledWith(3, "nike", {
      posts: false,
      reels: false,
      stories: true,
      highlights: false,
      avatar: false,
      max_posts: null,
    });
    expect(ipc.downloadDirect).not.toHaveBeenCalled();
  });

  it("submits exact shown Posts and Stories snapshots without archive refetches", async () => {
    const first = videoPost("p1", "https://cdninstagram.com/first.jpg");
    const second = videoPost("p2", "https://cdninstagram.com/second.jpg");
    const firstStory = { ...story("s1", "https://cdninstagram.com/story-one.jpg"), taken_at: 101 };
    const secondStory = { ...story("s2", "https://cdninstagram.com/story-two.jpg"), taken_at: 202 };
    ipc.enqueueFetchedPostDownload.mockResolvedValue("job-posts");
    ipc.downloadDirect.mockResolvedValue("job-stories");
    ipc.fetchStories.mockResolvedValue([firstStory, secondStory]);
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: [first, second] });

    await button(wrapper, "Shown 2").trigger("click");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledWith(
      "nike",
      "posts",
      "shown",
      [first, second],
    );
    expect(ipc.enqueueProfileDownload).not.toHaveBeenCalled();

    await button(wrapper, "Stories").trigger("click");
    await button(wrapper, "Shown 2").trigger("click");
    await flushPromises();
    expect(ipc.downloadDirect).toHaveBeenCalledWith("nike", "stories", [
      { url: firstStory.media_url, pk: "s1", taken_at: 101 },
      { url: secondStory.media_url, pk: "s2", taken_at: 202 },
    ]);
    expect(ipc.enqueueProfileDownload).not.toHaveBeenCalled();
  });

  it("keeps empty Shown and Selected actions disabled and side-effect free", async () => {
    const wrapper = render();
    await loadProfile(wrapper);

    expect(button(wrapper, "Shown 0").attributes("disabled")).toBeDefined();
    expect(button(wrapper, "Selected 0").attributes("disabled")).toBeDefined();
    await button(wrapper, "Shown 0").trigger("click");
    await button(wrapper, "Selected 0").trigger("click");
    await flushPromises();

    expect(ipc.enqueueFetchedPostDownload).not.toHaveBeenCalled();
    expect(ipc.enqueueProfileDownload).not.toHaveBeenCalled();
    expect(ipc.downloadDirect).not.toHaveBeenCalled();
  });

  it("accepts one exact 500-item Shown snapshot", async () => {
    const posts = Array.from({ length: 500 }, (_, index) =>
      videoPost(`p${index}`, `https://cdninstagram.com/post-${index}.jpg`),
    );
    ipc.enqueueFetchedPostDownload.mockResolvedValue("job-500-shown");
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: posts });

    expect(button(wrapper, "Shown 500").attributes("disabled")).toBeUndefined();
    await button(wrapper, "Shown 500").trigger("click");
    await flushPromises();

    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledTimes(1);
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledWith(
      "nike",
      "posts",
      "shown",
      posts,
    );
  });

  it("accepts one exact 500-item Selected snapshot", async () => {
    const posts = Array.from({ length: 500 }, (_, index) =>
      videoPost(`p${index}`, `https://cdninstagram.com/post-${index}.jpg`),
    );
    ipc.enqueueFetchedPostDownload.mockResolvedValue("job-500-selected");
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: posts });
    const store = useExplorerStore();
    for (const post of posts) store.toggleSelected("posts", post.pk);
    await flushPromises();

    expect(button(wrapper, "Selected 500").attributes("disabled")).toBeUndefined();
    await button(wrapper, "Selected 500").trigger("click");
    await flushPromises();

    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledTimes(1);
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledWith(
      "nike",
      "posts",
      "selected",
      posts,
    );
  }, 10_000);

  it("blocks a 501-item Shown snapshot before IPC while keeping All and small Selected available", async () => {
    const posts = Array.from({ length: 501 }, (_, index) =>
      videoPost(`p${index}`, `https://cdninstagram.com/post-${index}.jpg`),
    );
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: posts });
    useExplorerStore().toggleSelected("posts", posts[0]!.pk);
    await flushPromises();

    expect(button(wrapper, "All").attributes("disabled")).toBeUndefined();
    expect(button(wrapper, "Shown 501").attributes("disabled")).toBeDefined();
    expect(button(wrapper, "Selected 1").attributes("disabled")).toBeUndefined();
    expect(wrapper.text()).toContain("Shown has 501 items, above the 500-item exact snapshot limit.");
    wrapper.getComponent(DownloadScopeGroup).vm.$emit("download-shown");
    await flushPromises();

    expect(ipc.enqueueFetchedPostDownload).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain(
      "Shown snapshots are limited to 500 items. Use All for a complete archive.",
    );
  });

  it("blocks a 501-item Selected snapshot before IPC without truncating or chunking", async () => {
    const posts = Array.from({ length: 501 }, (_, index) =>
      videoPost(`p${index}`, `https://cdninstagram.com/post-${index}.jpg`),
    );
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: posts });
    const store = useExplorerStore();
    for (const post of posts) store.toggleSelected("posts", post.pk);
    await flushPromises();

    expect(button(wrapper, "All").attributes("disabled")).toBeUndefined();
    expect(button(wrapper, "Selected 501").attributes("disabled")).toBeDefined();
    expect(wrapper.text()).toContain("Selected has 501 items, above the 500-item exact snapshot limit.");
    wrapper.getComponent(DownloadScopeGroup).vm.$emit("download-selected");
    await flushPromises();

    expect(ipc.enqueueFetchedPostDownload).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain(
      "Selected snapshots are limited to 500 items. Use All for a complete archive.",
    );
  }, 10_000);

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
    expect(wrapper.findAll("[data-media-id] img").map((image) => image.attributes("src"))).toEqual([
      "remote-media:https://cdninstagram.com/reel-one.jpg",
      "remote-media:https://cdninstagram.com/reel-two.jpg",
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
    expect(ipc.fetchStories).toHaveBeenCalledWith("42");
    expect(wrapper.text()).toContain("Nike");
    expect(wrapper.text()).not.toContain("Loading profile");
    expect(wrapper.get("[data-media-id] img").attributes("src")).toBe(
      "remote-media:https://cdninstagram.com/feed.jpg",
    );
    await wrapper.get("[data-media-id] [data-action='preview']").trigger("click");
    expect(wrapper.find("post-modal-stub").exists()).toBe(true);

    await button(wrapper, "Stories").trigger("click");
    expect(wrapper.text()).toContain("Loading stories…");
    expect(wrapper.text()).not.toContain("Load stories · costs 2 requests");

    pending.resolve([story("s1", "https://cdninstagram.com/story.jpg")]);
    await flushPromises();
    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/story.jpg']").exists()).toBe(true);
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
    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/nike-story.jpg']").exists()).toBe(true);
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
    expect(wrapper.get("[data-media-id] img").attributes("src")).toBe(
      "remote-media:https://cdninstagram.com/feed.jpg",
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
    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/existing-story.jpg']").exists()).toBe(true);
    retry.resolve([story("s2", "https://cdninstagram.com/retried-story.jpg")]);
    await flushPromises();
    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/existing-story.jpg']").exists()).toBe(false);
    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/retried-story.jpg']").exists()).toBe(true);
  });

  it("loads more clips without duplicates and downloads the exact shown snapshot", async () => {
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
    ipc.enqueueFetchedPostDownload.mockResolvedValue("job-reels");
    const wrapper = render();
    await loadProfile(wrapper);

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();
    await button(wrapper, "Load more").trigger("click");
    await flushPromises();

    expect(wrapper.findAll("[data-media-id] img").map((image) => image.attributes("src"))).toEqual([
      "remote-media:https://cdninstagram.com/first.jpg",
      "remote-media:https://cdninstagram.com/second.jpg",
      "remote-media:https://cdninstagram.com/third.jpg",
    ]);
    expect(ipc.fetchReels).toHaveBeenNthCalledWith(2, "42", "next");

    await button(wrapper, "Shown 3").trigger("click");
    await flushPromises();

    expect(ipc.enqueueFetchedPostDownload).toHaveBeenCalledWith(
      "nike",
      "reels",
      "shown",
      [
        videoPost("r1", "https://cdninstagram.com/first.jpg"),
        videoPost("r2", "https://cdninstagram.com/second.jpg"),
        videoPost("r3", "https://cdninstagram.com/third.jpg"),
      ],
    );
    expect(ipc.enqueueProfileDownload).not.toHaveBeenCalled();
  });

  it("downloads exact selected Posts and Reels objects, including carousel resources", async () => {
    const carousel = {
      pk: "p1",
      code: "CAROUSEL",
      caption: "all resources stay together",
      resources: [
        { url: "https://cdninstagram.com/one.jpg", kind: "photo" as const },
        { url: "https://cdninstagram.com/two.mp4", kind: "video" as const },
      ],
      thumbnail_url: "https://cdninstagram.com/one.jpg",
    };
    const otherPost = videoPost("p2", "https://cdninstagram.com/other.jpg");
    const reel = videoPost("r1", "https://cdninstagram.com/reel.jpg");
    ipc.enqueueFetchedPostDownload
      .mockResolvedValueOnce("job-selected-posts")
      .mockResolvedValueOnce("job-selected-reels");
    ipc.fetchReels.mockResolvedValue({ posts: [reel], end_cursor: null });
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: [carousel, otherPost] });

    await selection(wrapper, "Select post CAROUSEL").setValue(true);
    expect(button(wrapper, "Selected 1").attributes("disabled")).toBeUndefined();
    await button(wrapper, "Selected 1").trigger("click");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenNthCalledWith(
      1,
      "nike",
      "posts",
      "selected",
      [carousel],
    );
    expect(carousel.resources).toHaveLength(2);
    finishJob("job-selected-posts");
    await flushPromises();

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();
    await selection(wrapper, "Select reel R1").setValue(true);
    await button(wrapper, "Selected 1").trigger("click");
    await flushPromises();
    expect(ipc.enqueueFetchedPostDownload).toHaveBeenNthCalledWith(
      2,
      "nike",
      "reels",
      "selected",
      [reel],
    );
  });

  it("downloads exact selected Story direct items", async () => {
    const selectedStory = {
      ...story("s1", "https://cdninstagram.com/selected-story.jpg"),
      taken_at: 123,
    };
    ipc.fetchStories.mockResolvedValue([
      selectedStory,
      story("s2", "https://cdninstagram.com/other-story.jpg"),
    ]);
    ipc.downloadDirect.mockResolvedValue("job-story");
    const wrapper = render();
    await loadProfile(wrapper);
    await button(wrapper, "Stories").trigger("click");

    await selection(wrapper, "Select story s1").setValue(true);
    await button(wrapper, "Selected 1").trigger("click");
    await flushPromises();

    expect(ipc.downloadDirect).toHaveBeenCalledWith("nike", "stories", [
      { url: selectedStory.media_url, pk: "s1", taken_at: 123 },
    ]);
  });

  it("clears only submitted IDs after success and preserves choices added while pending", async () => {
    const pending = deferred<string>();
    ipc.enqueueFetchedPostDownload.mockReturnValue(pending.promise);
    const first = videoPost("p1", "https://cdninstagram.com/first.jpg");
    const second = videoPost("p2", "https://cdninstagram.com/second.jpg");
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: [first, second] });

    await selection(wrapper, "Select post P1").setValue(true);
    await button(wrapper, "Selected 1").trigger("click");
    expect(selection(wrapper, "Select post P2").attributes("disabled")).toBeUndefined();
    await selection(wrapper, "Select post P2").setValue(true);
    expect(button(wrapper, "Selected 2").attributes("disabled")).toBeDefined();

    pending.resolve("job-posts");
    await flushPromises();

    expect((selection(wrapper, "Select post P1").element as HTMLInputElement).checked).toBe(false);
    expect((selection(wrapper, "Select post P2").element as HTMLInputElement).checked).toBe(true);
    expect(button(wrapper, "Selected 1").exists()).toBe(true);
  });

  it("preserves a submitted ID that is deselected and reselected while pending", async () => {
    const pending = deferred<string>();
    ipc.enqueueFetchedPostDownload.mockReturnValue(pending.promise);
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });

    const checkbox = selection(wrapper, "Select post P1");
    await checkbox.setValue(true);
    await button(wrapper, "Selected 1").trigger("click");
    await checkbox.setValue(false);
    await checkbox.setValue(true);

    pending.resolve("job-reselected");
    await flushPromises();

    expect((selection(wrapper, "Select post P1").element as HTMLInputElement).checked).toBe(true);
    expect(button(wrapper, "Selected 1").exists()).toBe(true);
  });

  it("clears an accepted submitted selection after the Explore view remounts", async () => {
    const pinia = createPinia();
    const pending = deferred<string>();
    ipc.enqueueFetchedPostDownload.mockReturnValue(pending.promise);
    const first = render(pinia);
    await loadProfile(first, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });

    await selection(first, "Select post P1").setValue(true);
    await button(first, "Selected 1").trigger("click");
    first.unmount();

    const second = render(pinia);
    expect((selection(second, "Select post P1").element as HTMLInputElement).checked).toBe(true);
    pending.resolve("job-remounted-selection");
    await flushPromises();

    expect((selection(second, "Select post P1").element as HTMLInputElement).checked).toBe(false);
    expect(button(second, "Selected 0").attributes("disabled")).toBeDefined();
  });

  it("retains submitted selection and reports an enqueue failure", async () => {
    ipc.enqueueFetchedPostDownload.mockRejectedValue(new Error("enqueue failed"));
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });

    await selection(wrapper, "Select post P1").setValue(true);
    await button(wrapper, "Selected 1").trigger("click");
    await flushPromises();

    expect((selection(wrapper, "Select post P1").element as HTMLInputElement).checked).toBe(true);
    expect(wrapper.text()).toContain("Error: enqueue failed");
  });

  it("does not let an old profile enqueue clear a replacement profile selection", async () => {
    const pending = deferred<string>();
    ipc.enqueueFetchedPostDownload.mockReturnValue(pending.promise);
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/nike.jpg")],
    });
    await selection(wrapper, "Select post P1").setValue(true);
    await button(wrapper, "Selected 1").trigger("click");

    await loadProfile(wrapper, {
      ...adidasPreview,
      recent_posts: [videoPost("a1", "https://cdninstagram.com/adidas.jpg")],
    });
    await selection(wrapper, "Select post A1").setValue(true);
    pending.resolve("job-nike");
    await flushPromises();

    expect(wrapper.text()).toContain("Adidas");
    expect((selection(wrapper, "Select post A1").element as HTMLInputElement).checked).toBe(true);
    expect(button(wrapper, "Selected 1").exists()).toBe(true);
  });

  it("does not let a same-username ABA enqueue clear a fresh selection", async () => {
    const pending = deferred<string>();
    ipc.enqueueFetchedPostDownload.mockReturnValue(pending.promise);
    const profileWithPost = {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    };
    const wrapper = render();
    await loadProfile(wrapper, profileWithPost);
    await selection(wrapper, "Select post P1").setValue(true);
    await button(wrapper, "Selected 1").trigger("click");

    await loadProfile(wrapper, { ...profileWithPost, profile: { ...profileWithPost.profile } });
    await selection(wrapper, "Select post P1").setValue(true);
    pending.resolve("job-old-nike");
    await flushPromises();

    expect((selection(wrapper, "Select post P1").element as HTMLInputElement).checked).toBe(true);
    expect(button(wrapper, "Selected 1").exists()).toBe(true);
  });

  it("preserves independent per-tab selections through tab changes and remount", async () => {
    const pinia = createPinia();
    ipc.fetchReels.mockResolvedValue({
      posts: [videoPost("r1", "https://cdninstagram.com/reel.jpg")],
      end_cursor: null,
    });
    ipc.fetchStories.mockResolvedValue([
      story("s1", "https://cdninstagram.com/story.jpg"),
    ]);
    const first = render(pinia);
    await loadProfile(first, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });
    await selection(first, "Select post P1").setValue(true);
    await button(first, "Reels").trigger("click");
    await flushPromises();
    await selection(first, "Select reel R1").setValue(true);
    await button(first, "Stories").trigger("click");
    await selection(first, "Select story s1").setValue(true);
    first.unmount();

    const second = render(pinia);
    await flushPromises();
    expect(button(second, "Selected 1").exists()).toBe(true);
    expect((selection(second, "Select story s1").element as HTMLInputElement).checked).toBe(true);

    await button(second, "Posts").trigger("click");
    expect(button(second, "Selected 1").exists()).toBe(true);
    expect((selection(second, "Select post P1").element as HTMLInputElement).checked).toBe(true);

    await button(second, "Reels").trigger("click");
    expect(button(second, "Selected 1").exists()).toBe(true);
    expect((selection(second, "Select reel R1").element as HTMLInputElement).checked).toBe(true);
  });

  it("keeps selection controls as preview-button siblings without nested interactive HTML", async () => {
    ipc.fetchStories.mockResolvedValue([
      story("s1", "https://cdninstagram.com/story.jpg"),
    ]);
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [videoPost("p1", "https://cdninstagram.com/post.jpg")],
    });

    const mediaTile = wrapper.get("[data-media-id='p1']");
    const mediaPreview = mediaTile.get("button[data-action='preview']");
    const mediaCheckbox = selection(wrapper, "Select post P1");
    expect(mediaTile.element.tagName).toBe("DIV");
    expect(mediaPreview.element.parentElement).toBe(mediaTile.element);
    expect(mediaCheckbox.element.closest("label")?.parentElement).toBe(mediaTile.element);
    expect(mediaTile.findAll("button")).toHaveLength(1);
    expect(mediaTile.findAll("input")).toHaveLength(1);
    expect(mediaTile.find("button input").exists()).toBe(false);
    await mediaCheckbox.trigger("click");
    expect(wrapper.find("post-modal-stub").exists()).toBe(false);
    if (!useExplorerStore().isSelected("posts", "p1")) {
      await mediaCheckbox.trigger("change");
    }
    expect(mediaTile.classes()).toEqual(expect.arrayContaining(["ring-2", "ring-accent"]));
    await mediaPreview.trigger("click");
    expect(wrapper.find("post-modal-stub").exists()).toBe(true);
    wrapper.getComponent({ name: "PostModal" }).vm.$emit("close");
    await flushPromises();

    await button(wrapper, "Stories").trigger("click");
    const storyTile = wrapper.get("[data-story-id='s1']");
    const storyPreview = storyTile.get("button[data-action='preview']");
    const storyCheckbox = selection(wrapper, "Select story s1");
    expect(storyTile.element.tagName).toBe("DIV");
    expect(storyPreview.element.parentElement).toBe(storyTile.element);
    expect(storyCheckbox.element.closest("label")?.parentElement).toBe(storyTile.element);
    expect(storyTile.findAll("button")).toHaveLength(1);
    expect(storyTile.findAll("input")).toHaveLength(1);
    expect(storyTile.find("button input").exists()).toBe(false);
    expect(wrapper.find("button input").exists()).toBe(false);
    expect(storyTile.classes()).toEqual(
      expect.arrayContaining(["border-2", "border-[var(--color-accent)]"]),
    );
    await storyCheckbox.trigger("click");
    expect(wrapper.find("post-modal-stub").exists()).toBe(false);
    if (!useExplorerStore().isSelected("stories", "s1")) {
      await storyCheckbox.trigger("change");
    }
    expect(storyTile.classes()).toEqual(expect.arrayContaining(["ring-2", "ring-accent"]));
    await storyPreview.trigger("click");
    expect(wrapper.find("post-modal-stub").exists()).toBe(true);
  });

  it("keeps the Stories scope group busy during the paid automatic request", async () => {
    const pending = deferred<ReturnType<typeof story>[]>();
    ipc.fetchStories.mockReturnValue(pending.promise);
    ipc.enqueueProfileDownload.mockResolvedValue("job-stories");
    const wrapper = render();
    await loadProfile(wrapper);
    await button(wrapper, "Stories").trigger("click");

    expect(downloadButtons(wrapper).every((item) => item.attributes("disabled") !== undefined)).toBe(true);
    await button(wrapper, "All").trigger("click");
    expect(ipc.fetchStories).toHaveBeenCalledTimes(1);
    expect(ipc.enqueueProfileDownload).not.toHaveBeenCalled();

    pending.resolve([story("s1", "https://cdninstagram.com/story.jpg")]);
    await flushPromises();
    expect(button(wrapper, "All").attributes("disabled")).toBeUndefined();
    await button(wrapper, "All").trigger("click");
    await flushPromises();
    expect(ipc.enqueueProfileDownload).toHaveBeenCalledWith("nike", {
      posts: false,
      reels: false,
      stories: true,
      highlights: false,
      avatar: false,
      max_posts: null,
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
    expect(downloadButtons(wrapper).map((item) => item.text())).toEqual([
      "All",
      "Shown 1",
      "Selected 0",
    ]);
    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/story.jpg']").exists()).toBe(true);

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();

    expect(wrapper.text()).not.toContain("Download all stories");
    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/story.jpg']").exists()).toBe(false);
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

    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/adidas-story.jpg']").exists()).toBe(true);
    nikeStories.resolve([story("nike", "https://cdninstagram.com/nike-story.jpg")]);
    await flushPromises();

    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/nike-story.jpg']").exists()).toBe(false);
    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/adidas-story.jpg']").exists()).toBe(true);
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

    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/fresh-story.jpg']").exists()).toBe(true);
    firstNikeStories.resolve([story("stale", "https://cdninstagram.com/stale-story.jpg")]);
    await flushPromises();

    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/stale-story.jpg']").exists()).toBe(false);
    expect(wrapper.find("img[src='remote-media:https://cdninstagram.com/fresh-story.jpg']").exists()).toBe(true);
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
    expect(second.findAll("[data-media-id] img")).toHaveLength(1);
    expect(ipc.fetchProfile).toHaveBeenCalledTimes(1);
    expect(ipc.fetchReels).toHaveBeenCalledTimes(1);
  });

  it("resumes unresolved Reels when a retained profile route remounts", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const store = useExplorerStore();
    store.commitProfile(preview);
    store.activeTab = "reels";
    await replaceExplorerRoute({ profile: "nike" });
    ipc.fetchReels.mockResolvedValue({
      posts: [videoPost("r1", "https://cdninstagram.com/recovered-reel.jpg")],
      end_cursor: null,
    });

    const wrapper = render(pinia);
    await flushPromises();

    expect(ipc.fetchProfile).not.toHaveBeenCalled();
    expect(ipc.fetchReels).toHaveBeenCalledWith("42", null);
    expect(wrapper.get('[data-media-id="r1"] img').attributes("src")).toBe(
      "remote-media:https://cdninstagram.com/recovered-reel.jpg",
    );
  });

  it("auto-loads unresolved stories once when a public profile remounts", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    useExplorerStore().commitProfile(preview);

    const wrapper = render(pinia);
    await flushPromises();

    expect(ipc.fetchStories).toHaveBeenCalledTimes(1);
    expect(ipc.fetchStories).toHaveBeenCalledWith("42");
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

    expect(second.find("img[src='remote-media:https://cdninstagram.com/preserved-story.jpg']").exists()).toBe(true);
    expect(useExplorerStore().storiesLoading).toBe(false);
  });

  it("refreshes story status when a pending request commits after its owner unmounts", async () => {
    const pending = deferred<ReturnType<typeof story>[]>();
    ipc.fetchStories.mockReturnValue(pending.promise);
    ipc.checkDownloadStatuses.mockResolvedValue([
      {
        namespace: "story",
        pk: "2101",
        state: "downloaded",
        available_resources: 1,
        expected_resources: 1,
      },
    ]);
    const pinia = createPinia();
    const first = render(pinia);
    await loadProfile(first);
    await button(first, "Stories").trigger("click");
    first.unmount();

    const second = render(pinia);
    await flushPromises();
    expect(button(second, "Stories").classes()).toContain("bg-surface-3");
    expect(second.text()).toContain("Loading stories…");
    expect(ipc.fetchStories).toHaveBeenCalledTimes(1);
    expect(ipc.checkDownloadStatuses).not.toHaveBeenCalled();

    pending.resolve([story("2101", "https://cdninstagram.com/preserved-status-story.jpg")]);
    await flushPromises();

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(1);
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledWith([
      { namespace: "story", pk: "2101", resources: ["photo"] },
    ]);
    expect(second.get("[data-story-id='2101'] [data-download-status]").text()).toBe(
      "Downloaded",
    );
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
    expect(wrapper.findAll("[data-media-id] img")).toHaveLength(0);
    expect(wrapper.html()).not.toContain("stale.jpg");
  });
});

describe("ExplorerView downloaded media status", () => {
  function status(
    namespace: "post" | "story",
    pk: string,
    state: "downloaded" | "partial" | "not_downloaded",
    available: number,
    expected: number,
  ) {
    return {
      namespace,
      pk,
      state,
      available_resources: available,
      expected_resources: expected,
    };
  }

  function keepStoriesPending() {
    const pending = deferred<ReturnType<typeof story>[]>();
    ipc.fetchStories.mockReturnValue(pending.promise);
    return pending;
  }

  it("requests exact post resource identities after the initial profile page commits", async () => {
    keepStoriesPending();
    const posts = [
      photoPost("101", "https://cdninstagram.com/photo"),
      videoPost("102", "https://cdninstagram.com/video"),
      carouselPost("103", "https://cdninstagram.com/carousel"),
    ];
    const wrapper = render();

    await loadProfile(wrapper, { ...preview, recent_posts: posts });

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(1);
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledWith([
      { namespace: "post", pk: "101", resources: ["photo"] },
      { namespace: "post", pk: "102", resources: ["video"] },
      { namespace: "post", pk: "103", resources: ["photo", "video"] },
    ]);
  });

  it("requests reels through the shared post namespace exactly once on first entry", async () => {
    keepStoriesPending();
    ipc.fetchReels.mockResolvedValue({
      posts: [videoPost("201", "https://cdninstagram.com/reel")],
      end_cursor: null,
    });
    const wrapper = render();
    await loadProfile(wrapper);
    ipc.checkDownloadStatuses.mockClear();

    await button(wrapper, "Reels").trigger("click");
    await flushPromises();

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(1);
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledWith([
      { namespace: "post", pk: "201", resources: ["video"] },
    ]);
  });

  it("requests exact story resource identities after stories commit", async () => {
    ipc.fetchStories.mockResolvedValue([
      story("301", "https://cdninstagram.com/story.jpg"),
      { ...story("302", "https://cdninstagram.com/story.mp4"), kind: "video" as const },
    ]);
    const wrapper = render();

    await loadProfile(wrapper);

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(1);
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledWith([
      { namespace: "story", pk: "301", resources: ["photo"] },
      { namespace: "story", pk: "302", resources: ["video"] },
    ]);
  });

  it("renders accessible sibling overlays for downloaded and partial media only", async () => {
    keepStoriesPending();
    ipc.checkDownloadStatuses.mockResolvedValue([
      status("post", "401", "downloaded", 1, 1),
      status("post", "402", "partial", 1, 2),
      status("post", "403", "not_downloaded", 0, 1),
    ]);
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [
        photoPost("401", "https://cdninstagram.com/one"),
        carouselPost("402", "https://cdninstagram.com/two"),
        photoPost("403", "https://cdninstagram.com/three"),
      ],
    });

    const downloaded = wrapper.get("[data-media-id='401'] [data-download-status]");
    const partial = wrapper.get("[data-media-id='402'] [data-download-status]");
    expect(downloaded.text()).toBe("Downloaded");
    expect(downloaded.classes()).toEqual(expect.arrayContaining(["pointer-events-none", "text-emerald-300"]));
    expect(partial.text()).toBe("Partial 1/2");
    expect(partial.classes()).toEqual(expect.arrayContaining(["pointer-events-none", "text-amber-300"]));
    expect(downloaded.element.parentElement).toBe(wrapper.get("[data-media-id='401']").element);
    expect(downloaded.element.closest("button")).toBeNull();
    expect(wrapper.find("[data-media-id='403'] [data-download-status]").exists()).toBe(false);
    expect(selection(wrapper, "Select post 401").attributes("disabled")).toBeUndefined();
    expect(selection(wrapper, "Select post 402").attributes("disabled")).toBeUndefined();
  });

  it("renders story download status as a non-interactive tile sibling", async () => {
    ipc.fetchStories.mockResolvedValue([story("501", "https://cdninstagram.com/story.jpg")]);
    ipc.checkDownloadStatuses.mockResolvedValue([
      status("story", "501", "downloaded", 1, 1),
    ]);
    const wrapper = render();
    await loadProfile(wrapper);
    await button(wrapper, "Stories").trigger("click");
    await flushPromises();

    const tile = wrapper.get("[data-story-id='501']");
    const badge = tile.get("[data-download-status]");
    expect(badge.text()).toBe("Downloaded");
    expect(badge.element.parentElement).toBe(tile.element);
    expect(badge.element.closest("button")).toBeNull();
    expect(selection(wrapper, "Select story 501").attributes("disabled")).toBeUndefined();
  });

  it("clears old badges synchronously when a replacement profile starts loading", async () => {
    keepStoriesPending();
    ipc.checkDownloadStatuses.mockResolvedValue([
      status("post", "601", "downloaded", 1, 1),
    ]);
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [photoPost("601", "https://cdninstagram.com/nike")],
    });
    expect(wrapper.text()).toContain("Downloaded");

    ipc.resolveInput.mockResolvedValueOnce({ kind: "profile", username: "adidas" });
    ipc.fetchProfile.mockReturnValueOnce(deferred<ProfilePreview>().promise);
    await wrapper.get("input").setValue("adidas");
    await wrapper.get("form").trigger("submit");

    expect(wrapper.find("[data-download-status]").exists()).toBe(false);
  });

  it("ignores an earlier profile status response after a later profile commits", async () => {
    keepStoriesPending();
    const oldStatus = deferred<ReturnType<typeof status>[]>();
    ipc.checkDownloadStatuses
      .mockReturnValueOnce(oldStatus.promise)
      .mockResolvedValueOnce([status("post", "701", "not_downloaded", 0, 1)]);
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [photoPost("701", "https://cdninstagram.com/nike")],
    });
    await loadProfile(wrapper, {
      ...adidasPreview,
      recent_posts: [photoPost("701", "https://cdninstagram.com/adidas")],
    });

    oldStatus.resolve([status("post", "701", "downloaded", 1, 1)]);
    await flushPromises();

    expect(wrapper.text()).toContain("Adidas");
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(2);
    expect(wrapper.find("[data-download-status]").exists()).toBe(false);
  });

  it("queries only newly committed pagination identities and merges their statuses", async () => {
    keepStoriesPending();
    ipc.checkDownloadStatuses
      .mockResolvedValueOnce([status("post", "801", "downloaded", 1, 1)])
      .mockResolvedValueOnce([status("post", "802", "partial", 1, 2)]);
    const first = photoPost("801", "https://cdninstagram.com/first");
    const second = carouselPost("802", "https://cdninstagram.com/second");
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: [first], end_cursor: "next" });
    ipc.fetchProfile.mockResolvedValueOnce({
      ...preview,
      recent_posts: [first, second],
      end_cursor: null,
    });

    await button(wrapper, "Load more").trigger("click");
    await flushPromises();

    expect(ipc.checkDownloadStatuses).toHaveBeenNthCalledWith(2, [
      { namespace: "post", pk: "802", resources: ["photo", "video"] },
    ]);
    expect(wrapper.get("[data-media-id='801'] [data-download-status]").text()).toBe("Downloaded");
    expect(wrapper.get("[data-media-id='802'] [data-download-status]").text()).toBe("Partial 1/2");
  });

  it("ignores an older same-tab refresh and runs one full follow-up", async () => {
    keepStoriesPending();
    const initial = deferred<ReturnType<typeof status>[]>();
    ipc.checkDownloadStatuses
      .mockReturnValueOnce(initial.promise)
      .mockResolvedValueOnce([
        status("post", "901", "not_downloaded", 0, 1),
        status("post", "902", "downloaded", 1, 1),
      ]);
    const first = photoPost("901", "https://cdninstagram.com/first");
    const second = photoPost("902", "https://cdninstagram.com/second");
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: [first], end_cursor: "next" });
    ipc.fetchProfile.mockResolvedValueOnce({
      ...preview,
      recent_posts: [second],
      end_cursor: null,
    });
    await button(wrapper, "Load more").trigger("click");
    await flushPromises();
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(1);

    initial.resolve([status("post", "901", "downloaded", 1, 1)]);
    await flushPromises();

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(2);
    expect(ipc.checkDownloadStatuses).toHaveBeenLastCalledWith([
      { namespace: "post", pk: "901", resources: ["photo"] },
      { namespace: "post", pk: "902", resources: ["photo"] },
    ]);
    expect(wrapper.find("[data-media-id='901'] [data-download-status]").exists()).toBe(false);
    expect(wrapper.get("[data-media-id='902'] [data-download-status]").text()).toBe("Downloaded");
  });

  it("keeps Posts and Reels status responses isolated", async () => {
    keepStoriesPending();
    const postsStatus = deferred<ReturnType<typeof status>[]>();
    ipc.checkDownloadStatuses
      .mockReturnValueOnce(postsStatus.promise)
      .mockResolvedValueOnce([status("post", "1001", "not_downloaded", 0, 1)]);
    ipc.fetchReels.mockResolvedValue({
      posts: [videoPost("1001", "https://cdninstagram.com/reel")],
      end_cursor: null,
    });
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [photoPost("1001", "https://cdninstagram.com/post")],
    });
    await button(wrapper, "Reels").trigger("click");
    await flushPromises();

    postsStatus.resolve([status("post", "1001", "downloaded", 1, 1)]);
    await flushPromises();

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(2);
    expect(button(wrapper, "Reels").classes()).toContain("bg-surface-3");
    expect(wrapper.find("[data-download-status]").exists()).toBe(false);
  });

  it("ignores status responses that settle after unmount", async () => {
    keepStoriesPending();
    const stale = deferred<ReturnType<typeof status>[]>();
    ipc.checkDownloadStatuses
      .mockReturnValueOnce(stale.promise)
      .mockResolvedValueOnce([status("post", "1101", "not_downloaded", 0, 1)]);
    const pinia = createPinia();
    const first = render(pinia);
    await loadProfile(first, {
      ...preview,
      recent_posts: [photoPost("1101", "https://cdninstagram.com/post")],
    });
    first.unmount();
    const second = render(pinia);
    await flushPromises();

    stale.resolve([status("post", "1101", "downloaded", 1, 1)]);
    await flushPromises();

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(2);
    expect(second.find("[data-download-status]").exists()).toBe(false);
  });

  it("preserves an honest badge and the main Explore error when lookup rejects", async () => {
    keepStoriesPending();
    ipc.checkDownloadStatuses
      .mockResolvedValueOnce([status("post", "1201", "downloaded", 1, 1)])
      .mockRejectedValueOnce(new Error("catalog unavailable"));
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [photoPost("1201", "https://cdninstagram.com/post")],
    });
    ipc.downloadDirect.mockRejectedValueOnce(new Error("existing Explore failure"));
    await button(wrapper, "↓ Avatar").trigger("click");
    await flushPromises();
    const priorError = wrapper.get("p.text-err").text();
    expect(priorError).toBe("Error: existing Explore failure");

    useJobsStore().addPlaceholder("lookup-failure", "download");
    finishJob("lookup-failure", "failed");
    await flushPromises();

    expect(wrapper.get("[data-download-status]").text()).toBe("Downloaded");
    expect(wrapper.text()).not.toContain("catalog unavailable");
    expect(wrapper.get("p.text-err").text()).toBe(priorError);
  });

  it("removes a previous badge after an honest not-downloaded refresh", async () => {
    keepStoriesPending();
    ipc.checkDownloadStatuses
      .mockResolvedValueOnce([status("post", "1301", "downloaded", 1, 1)])
      .mockResolvedValueOnce([status("post", "1301", "not_downloaded", 0, 1)]);
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [photoPost("1301", "https://cdninstagram.com/post")],
    });
    expect(wrapper.text()).toContain("Downloaded");
    useJobsStore().addPlaceholder("honest-removal", "download");
    finishJob("honest-removal");
    await flushPromises();

    expect(wrapper.find("[data-download-status]").exists()).toBe(false);
  });

  it("refreshes the full retained active tab after remount", async () => {
    keepStoriesPending();
    const pinia = createPinia();
    const posts = [
      photoPost("1401", "https://cdninstagram.com/one"),
      videoPost("1402", "https://cdninstagram.com/two"),
    ];
    const first = render(pinia);
    await loadProfile(first, { ...preview, recent_posts: posts });
    first.unmount();
    ipc.checkDownloadStatuses.mockClear();
    ipc.checkDownloadStatuses.mockResolvedValue([
      status("post", "1401", "downloaded", 1, 1),
      status("post", "1402", "downloaded", 1, 1),
    ]);

    const second = render(pinia);
    await flushPromises();

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(1);
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledWith([
      { namespace: "post", pk: "1401", resources: ["photo"] },
      { namespace: "post", pk: "1402", resources: ["video"] },
    ]);
    expect(second.findAll("[data-download-status]")).toHaveLength(2);
  });

  it("refreshes committed media on explicit tab re-entry", async () => {
    keepStoriesPending();
    ipc.fetchReels.mockResolvedValue({
      posts: [videoPost("1501", "https://cdninstagram.com/reel")],
      end_cursor: null,
    });
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [photoPost("1502", "https://cdninstagram.com/post")],
    });
    await button(wrapper, "Reels").trigger("click");
    await flushPromises();
    ipc.checkDownloadStatuses.mockClear();

    await button(wrapper, "Posts").trigger("click");
    await flushPromises();

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(1);
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledWith([
      { namespace: "post", pk: "1502", resources: ["photo"] },
    ]);
  });

  it("preserves the whole prior map when the second chunk of a 501-item full refresh rejects", async () => {
    keepStoriesPending();
    const posts = Array.from({ length: 501 }, (_, index) =>
      photoPost(String(2000 + index), `https://cdninstagram.com/${index}`),
    );
    const requests = posts.map((post) => ({
      namespace: "post" as const,
      pk: post.pk,
      resources: ["photo" as const],
    }));
    ipc.checkDownloadStatuses
      .mockResolvedValueOnce([status("post", posts[0]!.pk, "downloaded", 1, 1)])
      .mockResolvedValueOnce([status("post", posts[500]!.pk, "partial", 1, 2)]);
    const wrapper = render();
    await loadProfile(wrapper, { ...preview, recent_posts: posts });

    expect(wrapper.get(`[data-media-id='${posts[0]!.pk}'] [data-download-status]`).text()).toBe(
      "Downloaded",
    );
    expect(wrapper.get(`[data-media-id='${posts[500]!.pk}'] [data-download-status]`).text()).toBe(
      "Partial 1/2",
    );
    expect(wrapper.findAll("[data-download-status]")).toHaveLength(2);

    ipc.checkDownloadStatuses.mockReset();
    ipc.checkDownloadStatuses
      .mockImplementationOnce(async (items: Array<{ pk: string }>) =>
        items.map((item, index) =>
          status("post", item.pk, index === 0 ? "not_downloaded" : "downloaded", index === 0 ? 0 : 1, 1),
        ),
      )
      .mockRejectedValueOnce(new Error("second chunk failed"));
    useJobsStore().addPlaceholder("full-501-refresh", "download");
    finishJob("full-501-refresh");
    await flushPromises();

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(2);
    expect(ipc.checkDownloadStatuses).toHaveBeenNthCalledWith(1, requests.slice(0, 500));
    expect(ipc.checkDownloadStatuses).toHaveBeenNthCalledWith(2, requests.slice(500));
    expect(wrapper.findAll("[data-download-status]")).toHaveLength(2);
    expect(wrapper.get(`[data-media-id='${posts[0]!.pk}'] [data-download-status]`).text()).toBe(
      "Downloaded",
    );
    expect(wrapper.get(`[data-media-id='${posts[500]!.pk}'] [data-download-status]`).text()).toBe(
      "Partial 1/2",
    );
  });

  it("does not add a refresh for a terminal job already present when retained media mounts", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    useExplorerStore().commitProfile({
      ...preview,
      recent_posts: [photoPost("2501", "https://cdninstagram.com/retained")],
    });
    useJobsStore().addPlaceholder("already-done", "old download");
    finishJob("already-done");
    render(pinia);
    await flushPromises();

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(1);
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledWith([
      { namespace: "post", pk: "2501", resources: ["photo"] },
    ]);

    useJobsStore().addPlaceholder("seed-check-progress", "unrelated progress");
    const progress = useJobsStore().jobs.get("seed-check-progress")!;
    progress.state = "downloading";
    progress.currentFile = 1;
    await flushPromises();

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(1);
  });

  it("does not refresh loaded media for progress-only job changes", async () => {
    keepStoriesPending();
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [photoPost("2502", "https://cdninstagram.com/loaded")],
    });
    ipc.checkDownloadStatuses.mockClear();

    useJobsStore().addPlaceholder("progress", "new download");
    const progress = useJobsStore().jobs.get("progress")!;
    progress.state = "downloading";
    progress.currentFile = 2;
    await flushPromises();

    expect(ipc.checkDownloadStatuses).not.toHaveBeenCalled();
    expect(wrapper.find("[data-media-id='2502']").exists()).toBe(true);
  });

  it("refreshes for done, failed, and cancelled transitions and coalesces a tick", async () => {
    keepStoriesPending();
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [photoPost("3001", "https://cdninstagram.com/post")],
    });
    ipc.checkDownloadStatuses.mockClear();

    useJobsStore().addPlaceholder("done", "done");
    finishJob("done", "done");
    await flushPromises();
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(1);

    useJobsStore().addPlaceholder("failed", "failed");
    finishJob("failed", "failed");
    await flushPromises();
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(2);

    useJobsStore().addPlaceholder("cancelled-a", "cancelled-a");
    useJobsStore().addPlaceholder("cancelled-b", "cancelled-b");
    finishJob("cancelled-a", "cancelled");
    finishJob("cancelled-b", "cancelled");
    await flushPromises();
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(3);
  });

  it("queues one full follow-up when a terminal arrives during an in-flight refresh", async () => {
    keepStoriesPending();
    const initial = deferred<ReturnType<typeof status>[]>();
    ipc.checkDownloadStatuses
      .mockReturnValueOnce(initial.promise)
      .mockResolvedValueOnce([status("post", "4001", "downloaded", 1, 1)]);
    const wrapper = render();
    await loadProfile(wrapper, {
      ...preview,
      recent_posts: [photoPost("4001", "https://cdninstagram.com/post")],
    });

    useJobsStore().addPlaceholder("terminal-during-refresh", "download");
    finishJob("terminal-during-refresh");
    await flushPromises();
    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(1);

    initial.resolve([status("post", "4001", "not_downloaded", 0, 1)]);
    await flushPromises();

    expect(ipc.checkDownloadStatuses).toHaveBeenCalledTimes(2);
    expect(wrapper.get("[data-download-status]").text()).toBe("Downloaded");
  });
});
