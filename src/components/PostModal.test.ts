/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => ({
  downloadDirect: vi.fn(),
  downloadPost: vi.fn(),
}));

vi.mock("../lib/ipc", () => ({
  ...ipc,
  cancelJob: vi.fn(),
  onJobProgress: vi.fn(),
}));

import PostModal from "./PostModal.vue";
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

const wrappers: Array<{ unmount: () => void }> = [];
let writeText: ReturnType<typeof vi.fn>;

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
  writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
});

afterEach(() => {
  for (const wrapper of wrappers.splice(0)) wrapper.unmount();
  vi.useRealTimers();
  vi.restoreAllMocks();
  document.body.replaceChildren();
});

describe("PostModal copy actions", () => {
  it("copies the complete untruncated caption and announces success", async () => {
    const wrapper = render();

    await action(wrapper, "copy-description").trigger("click");
    await flushPromises();

    expect(writeText).toHaveBeenCalledWith(post.caption);
    expect(action(wrapper, "copy-description").text()).toContain("Copied");
    expect(wrapper.get("[aria-live='polite']").text()).toContain("description copied");
  });

  it("copies a canonical post URL rather than a media URL", async () => {
    const wrapper = render();

    await action(wrapper, "copy-link").trigger("click");
    await flushPromises();

    expect(writeText).toHaveBeenCalledWith("https://www.instagram.com/p/POSTCODE/");
  });

  it("copies a canonical reel URL", async () => {
    const wrapper = render({ postCategory: "reels" });

    await action(wrapper, "copy-link").trigger("click");
    await flushPromises();

    expect(writeText).toHaveBeenCalledWith("https://www.instagram.com/reel/POSTCODE/");
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
    writeText.mockRejectedValueOnce(new Error("Clipboard blocked"));
    const wrapper = render();

    await action(wrapper, "copy-link").trigger("click");
    await flushPromises();

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
    writeText.mockReturnValueOnce(pending.promise);
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
    writeText.mockReturnValueOnce(pending.promise);
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
    writeText.mockReturnValueOnce(pending.promise);
    const startTimer = vi.spyOn(window, "setTimeout");
    const wrapper = render();

    await action(wrapper, "copy-link").trigger("click");
    wrapper.unmount();
    pending.resolve();
    await flushPromises();

    expect(startTimer).not.toHaveBeenCalled();
  });
});
