/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => ({
  downloadDirect: vi.fn(),
  downloadPost: vi.fn(),
  remoteMediaUrl: vi.fn((url: string) => `remote-media:${url}`),
}));
const clipboard = vi.hoisted(() => ({ writeText: vi.fn() }));

vi.mock("../lib/ipc", () => ({
  ...ipc,
  cancelJob: vi.fn(),
  onJobProgress: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-clipboard-manager", () => clipboard);

import PostModal from "./PostModal.vue";
import RemoteImage from "./RemoteImage.vue";
import RemoteVideo from "./RemoteVideo.vue";
import type { Post, StoryItem } from "../lib/ipc";

const post: Post = {
  pk: "42",
  code: "POSTCODE",
  caption: "The complete caption\nwith every line preserved.",
  resources: [{ url: "https://cdn.example/post.jpg", kind: "photo" }],
};

const story: StoryItem = {
  pk: "story-42",
  kind: "photo",
  media_url: "https://cdn.example/story.jpg",
};
const signedPhotoSource =
  "https://cdn.example/signed-photo.jpg?token=photo%2Bsignature&expires=999999#preview";
const signedVideoSource =
  "https://cdn.example/signed-video.mp4?token=video%2Bsignature&expires=999999#preview";

const wrappers: Array<{ unmount: () => void }> = [];

function render(props: Partial<InstanceType<typeof PostModal>["$props"]> = {}) {
  setActivePinia(createPinia());
  const wrapper = mount(PostModal, {
    props: {
      username: "nike",
      post,
      postCategory: "posts",
      ...props,
    },
    global: { plugins: [createPinia()], stubs: { Teleport: true } },
  });
  wrappers.push(wrapper);
  return wrapper;
}

function action(wrapper: ReturnType<typeof render>, name: string) {
  return wrapper.get(`[data-action="${name}"]`);
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

beforeEach(() => {
  vi.useFakeTimers();
  clipboard.writeText.mockResolvedValue(undefined);
});

afterEach(() => {
  for (const wrapper of wrappers.splice(0)) wrapper.unmount();
  vi.useRealTimers();
  vi.restoreAllMocks();
  document.body.replaceChildren();
});

describe("PostModal copy actions", () => {
  it("keeps the modal card scrollable within the viewport with Download inside", () => {
    const wrapper = render();
    const card = wrapper.get(".card");

    expect(card.classes()).toContain("max-h-[calc(100vh-2rem)]");
    expect(card.classes()).toContain("overflow-y-auto");
    expect(card.attributes("data-testid")).toBe("post-modal-card");
    expect(card.findAll("button").some((button) => button.text() === "Download")).toBe(true);
  });

  it("renders a photo post with the original source through RemoteImage", () => {
    const wrapper = render();
    const preview = wrapper.getComponent(RemoteImage);

    expect(preview.props("source")).toBe(post.resources[0].url);
    expect(preview.props("alt")).toBe("@nike · POSTCODE preview");
    expect(preview.props("variant")).toBe("modal");
  });

  it("renders a photo story with the original source through RemoteImage", () => {
    const wrapper = render({ post: null, story });
    const preview = wrapper.getComponent(RemoteImage);

    expect(preview.props("source")).toBe(story.media_url);
    expect(preview.props("alt")).toBe("Story · @nike preview");
    expect(preview.props("variant")).toBe("modal");
  });

  it("renders a reel video resource with controls through RemoteVideo", () => {
    const videoPost: Post = {
      ...post,
      code: "REELCODE",
      owner_username: "runner",
      resources: [
        { url: "https://cdn.example/reel-cover.jpg", kind: "photo" },
        { url: "https://cdn.example/reel-original.mp4", kind: "video" },
      ],
    };
    const wrapper = render({ post: videoPost, postCategory: "reels" });
    const preview = wrapper.getComponent(RemoteVideo);

    expect(preview.props("source")).toBe(videoPost.resources[1].url);
    expect(preview.props("label")).toBe("@runner · REELCODE video preview");
    expect(preview.props("controls")).toBe(true);
  });

  it("passes signed photo and video sources to remote components unchanged", () => {
    const photoWrapper = render({
      post: { ...post, resources: [{ url: signedPhotoSource, kind: "photo" }] },
    });
    const videoWrapper = render({
      post: {
        ...post,
        resources: [{ url: signedVideoSource, kind: "video" }],
      },
    });

    expect(photoWrapper.getComponent(RemoteImage).props("source")).toBe(signedPhotoSource);
    expect(videoWrapper.getComponent(RemoteVideo).props("source")).toBe(signedVideoSource);
  });

  it("prefers a thumbnail for photos and a video resource over any image", () => {
    const thumbnailSource = "https://cdn.example/thumbnail.jpg?size=modal#cover";
    const photoResource = "https://cdn.example/photo-resource.jpg";
    const videoResource = "https://cdn.example/video-resource.mp4?quality=original#media";
    const photoWrapper = render({
      post: {
        ...post,
        thumbnail_url: thumbnailSource,
        resources: [{ url: photoResource, kind: "photo" }],
      },
    });
    const videoWrapper = render({
      post: {
        ...post,
        thumbnail_url: thumbnailSource,
        resources: [
          { url: photoResource, kind: "photo" },
          { url: videoResource, kind: "video" },
        ],
      },
    });

    expect(photoWrapper.getComponent(RemoteImage).props("source")).toBe(thumbnailSource);
    expect(videoWrapper.findComponent(RemoteImage).exists()).toBe(false);
    expect(videoWrapper.getComponent(RemoteVideo).props("source")).toBe(videoResource);
  });

  it("switches from a photo preview to the exact new video source after a prop change", async () => {
    const wrapper = render({
      post: { ...post, resources: [{ url: signedPhotoSource, kind: "photo" }] },
    });
    expect(wrapper.getComponent(RemoteImage).props("source")).toBe(signedPhotoSource);

    await wrapper.setProps({
      post: { ...post, resources: [{ url: signedVideoSource, kind: "video" }] },
    });
    await flushPromises();

    expect(wrapper.findComponent(RemoteImage).exists()).toBe(false);
    expect(wrapper.getComponent(RemoteVideo).props("source")).toBe(signedVideoSource);
  });

  it("renders a video story with controls through RemoteVideo", () => {
    const videoStory: StoryItem = {
      ...story,
      kind: "video",
      media_url: "https://cdn.example/story-original.mp4",
    };
    const wrapper = render({ post: null, story: videoStory });
    const preview = wrapper.getComponent(RemoteVideo);

    expect(preview.props("source")).toBe(videoStory.media_url);
    expect(preview.props("label")).toBe("Story · @nike video preview");
    expect(preview.props("controls")).toBe(true);
  });

  it("keeps post and story downloads unchanged", async () => {
    const postWrapper = render();
    await postWrapper.findAll("button").find((button) => button.text() === "Download")!.trigger("click");
    await flushPromises();
    expect(ipc.downloadPost).toHaveBeenCalledWith(post.code);

    const storyWrapper = render({ post: null, story });
    await storyWrapper.findAll("button").find((button) => button.text() === "Download")!.trigger("click");
    await flushPromises();
    expect(ipc.downloadDirect).toHaveBeenCalledWith("nike", "stories", [
      { url: story.media_url, pk: story.pk, taken_at: story.taken_at },
    ]);
  });

  it("copies the complete untruncated caption and announces success", async () => {
    const wrapper = render();

    await action(wrapper, "copy-description").trigger("click");
    await flushPromises();

    expect(clipboard.writeText).toHaveBeenCalledWith(post.caption);
    expect(action(wrapper, "copy-description").text()).toContain("Copied");
    expect(wrapper.get("[aria-live='polite']").text()).toContain("description copied");
  });

  it("copies a canonical post URL rather than a media URL", async () => {
    const wrapper = render();

    await action(wrapper, "copy-link").trigger("click");
    await flushPromises();

    expect(clipboard.writeText).toHaveBeenCalledWith("https://www.instagram.com/p/POSTCODE/");
  });

  it("copies a canonical reel URL", async () => {
    const wrapper = render({ postCategory: "reels" });

    await action(wrapper, "copy-link").trigger("click");
    await flushPromises();

    expect(clipboard.writeText).toHaveBeenCalledWith("https://www.instagram.com/reel/POSTCODE/");
  });

  it("disables description copy when the caption is absent", () => {
    const wrapper = render({ post: { ...post, caption: "" } });

    expect(action(wrapper, "copy-description").attributes("disabled")).toBeDefined();
    expect(action(wrapper, "copy-description").attributes("aria-disabled")).toBe("true");
  });

  it("does not show copy actions for stories", () => {
    const wrapper = render({ post: null, story });

    expect(wrapper.find("[data-action='copy-description']").exists()).toBe(false);
    expect(wrapper.find("[data-action='copy-link']").exists()).toBe(false);
  });

  it("renders a concise inline error without success when clipboard access fails", async () => {
    clipboard.writeText.mockRejectedValueOnce(new Error("Clipboard blocked"));
    const wrapper = render();

    await action(wrapper, "copy-link").trigger("click");
    await flushPromises();

    expect(wrapper.get("[data-copy-error]").attributes("role")).toBe("alert");
    expect(wrapper.get("[data-copy-error]").text()).toContain("Could not copy");
    expect(action(wrapper, "copy-link").text()).not.toContain("Copied");
    expect(wrapper.get("[aria-live='polite']").text()).toBe("");
  });

  it("clears transient copy feedback after its timer and when the preview item changes", async () => {
    const wrapper = render();
    await action(wrapper, "copy-link").trigger("click");
    await flushPromises();
    expect(action(wrapper, "copy-link").text()).toContain("Copied");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await flushPromises();
    expect(wrapper.emitted("close")).toHaveLength(1);
    expect(action(wrapper, "copy-link").text()).not.toContain("Copied");

    await action(wrapper, "copy-link").trigger("click");
    await flushPromises();
    await wrapper.setProps({ post: { ...post, code: "NEXT" } });
    expect(action(wrapper, "copy-link").text()).not.toContain("Copied");

    await action(wrapper, "copy-link").trigger("click");
    await flushPromises();
    vi.advanceTimersByTime(2000);
    await flushPromises();
    expect(action(wrapper, "copy-link").text()).not.toContain("Copied");
  });

  it("ignores a delayed clipboard completion after the modal closes", async () => {
    const pending = deferred<void>();
    clipboard.writeText.mockReturnValueOnce(pending.promise);
    const wrapper = render();

    await action(wrapper, "copy-link").trigger("click");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    pending.resolve();
    await flushPromises();

    expect(wrapper.emitted("close")).toHaveLength(1);
    expect(action(wrapper, "copy-link").text()).not.toContain("Copied");
    expect(wrapper.get("[aria-live='polite']").text()).toBe("");
  });

  it("ignores a delayed clipboard failure after the preview item changes", async () => {
    const pending = deferred<void>();
    clipboard.writeText.mockReturnValueOnce(pending.promise);
    const wrapper = render();

    await action(wrapper, "copy-description").trigger("click");
    await wrapper.setProps({ post: { ...post, code: "NEXT" } });
    pending.reject(new Error("Clipboard blocked"));
    await flushPromises();

    expect(wrapper.find("[data-copy-error]").exists()).toBe(false);
    expect(action(wrapper, "copy-description").text()).not.toContain("Copied");
  });

  it("does not create delayed feedback or a timer after unmount", async () => {
    const pending = deferred<void>();
    clipboard.writeText.mockReturnValueOnce(pending.promise);
    const startTimer = vi.spyOn(window, "setTimeout");
    const wrapper = render();

    await action(wrapper, "copy-link").trigger("click");
    wrapper.unmount();
    pending.resolve();
    await flushPromises();

    expect(startTimer).not.toHaveBeenCalled();
  });
});

describe("PostModal caption mentions", () => {
  const captionWithMention =
    "First line\nFollow @Nike_Official or email mail@example.com <em>literally</em>.";

  it("renders valid mentions as accessible profile links while keeping other caption text safe", () => {
    const wrapper = render({ post: { ...post, caption: captionWithMention } });
    const captionElement = wrapper.get("[data-caption]");
    const mention = captionElement
      .findAll("a[href]")
      .find((link) => link.element.textContent === "@Nike_Official");

    expect(captionElement.element.tagName).toBe("P");
    expect(captionElement.classes()).toEqual(
      expect.arrayContaining(["whitespace-pre-wrap", "break-words"]),
    );
    expect(captionElement.classes()).not.toContain("line-clamp-4");
    expect(mention).toBeDefined();
    expect(mention!.element.tagName).toBe("A");
    expect(mention!.attributes("href")).toBe("/explore?profile=Nike_Official");
    expect(mention!.attributes("aria-label")).toBeUndefined();
    expect(mention!.classes().join(" ")).toContain("focus-visible:outline-2");
    expect(mention!.classes()).toContain("underline");
    expect(mention!.classes()).toContain("decoration-current");
    expect(mention!.classes()).not.toContain("decoration-transparent");
    expect(captionElement.findAll("a")).toHaveLength(1);
    expect(captionElement.element.textContent).toBe(captionWithMention);
    expect(captionElement.find("em").exists()).toBe(false);
  });

  it("keeps text-only captions clamped", () => {
    const wrapper = render({ post: { ...post, caption: "A plain caption without mentions." } });

    expect(wrapper.get("[data-caption]").classes()).toContain("line-clamp-4");
  });

  it("prevents native navigation and emits the username for an unmodified primary click", () => {
    const wrapper = render({ post: { ...post, caption: captionWithMention } });
    const mention = wrapper.get('[data-caption-mention="Nike_Official"]');
    const bubbledClick = vi.fn();
    wrapper.get("[data-testid='post-modal-card']").element.addEventListener("click", bubbledClick);
    const click = new MouseEvent("click", {
      button: 0,
      bubbles: true,
      cancelable: true,
    });

    mention.element.dispatchEvent(click);

    expect(click.defaultPrevented).toBe(true);
    expect(bubbledClick).not.toHaveBeenCalled();
    expect(wrapper.emitted("open-profile")).toEqual([["Nike_Official"]]);
  });

  it.each([
    ["a macOS meta-click", { button: 0, metaKey: true }],
    ["a ctrl-click", { button: 0, ctrlKey: true }],
    ["a shift-click", { button: 0, shiftKey: true }],
    ["an alt-click", { button: 0, altKey: true }],
    ["a non-primary click", { button: 1 }],
  ])("keeps native navigation and does not emit for %s", (_label, init) => {
    const wrapper = render({ post: { ...post, caption: captionWithMention } });
    const mention = wrapper.get('[data-caption-mention="Nike_Official"]');
    const defaultPreventedAtCard: boolean[] = [];
    const bubbledClick = vi.fn((event: Event) => {
      defaultPreventedAtCard.push(event.defaultPrevented);
      event.preventDefault();
    });
    wrapper.get("[data-testid='post-modal-card']").element.addEventListener("click", bubbledClick);
    const click = new MouseEvent("click", {
      ...init,
      bubbles: true,
      cancelable: true,
    });

    mention.element.dispatchEvent(click);

    expect(wrapper.emitted("open-profile")).toBeUndefined();
    expect(bubbledClick).toHaveBeenCalledOnce();
    expect(defaultPreventedAtCard).toEqual([false]);
  });

  it("renders independent links for two mentions and emits the clicked username", () => {
    const wrapper = render({
      post: { ...post, caption: "Follow @nike and @adidas." },
    });
    const mentions = wrapper.findAll("[data-caption-mention]");

    expect(mentions).toHaveLength(2);
    expect(mentions.map((mention) => mention.attributes("href"))).toEqual([
      "/explore?profile=nike",
      "/explore?profile=adidas",
    ]);

    const click = new MouseEvent("click", {
      button: 0,
      bubbles: true,
      cancelable: true,
    });
    mentions[1].element.dispatchEvent(click);

    expect(click.defaultPrevented).toBe(true);
    expect(wrapper.emitted("open-profile")).toEqual([["adidas"]]);
  });

  it("copies the exact caption including its newline and mention", async () => {
    const wrapper = render({ post: { ...post, caption: captionWithMention } });

    await action(wrapper, "copy-description").trigger("click");
    await flushPromises();

    expect(clipboard.writeText).toHaveBeenCalledWith(captionWithMention);
  });
});
