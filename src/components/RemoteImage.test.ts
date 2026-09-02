/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => ({
  remoteMediaUrl: vi.fn<(source: string) => string>(),
}));

vi.mock("../lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/ipc")>()),
  remoteMediaUrl: ipc.remoteMediaUrl,
}));

import MediaPreviewPlaceholder from "./MediaPreviewPlaceholder.vue";
import RemoteImage from "./RemoteImage.vue";
import type { RemoteMediaVariant } from "./remoteMedia";
import { useRemoteMediaHealthStore } from "../stores/remoteMediaHealth";

type RemoteImageProps = InstanceType<typeof RemoteImage>["$props"];

const wrappers: Array<{ unmount: () => void }> = [];

function render(props: Partial<RemoteImageProps> = {}) {
  const pinia = createPinia();
  setActivePinia(pinia);
  const health = useRemoteMediaHealthStore(pinia);
  const reportSuccess = vi.spyOn(health, "reportSuccess");
  const reportFailure = vi.spyOn(health, "reportFailure");
  const wrapper = mount(RemoteImage, {
    props: {
      source: "https://cdn.example/preview.jpg",
      alt: "Post preview",
      variant: "thumbnail",
      ...props,
    },
    attrs: { id: "remote-preview" },
    global: { plugins: [pinia] },
  });
  wrappers.push(wrapper);

  return { wrapper, health, reportSuccess, reportFailure };
}

beforeEach(() => {
  ipc.remoteMediaUrl.mockReset();
  ipc.remoteMediaUrl.mockImplementation((source) =>
    source.startsWith("https://") ? `remote-media:${source}` : "",
  );
});

afterEach(() => {
  for (const wrapper of wrappers.splice(0)) wrapper.unmount();
  vi.restoreAllMocks();
});

describe("RemoteImage", () => {
  it("keeps the native image hidden until load and reports success for the trimmed source", async () => {
    const { wrapper, reportSuccess, reportFailure } = render({
      source: "  https://cdn.example/original.jpg  ",
      alt: "  Original alt text  ",
    });
    const root = wrapper.get("[data-remote-image]");
    const image = wrapper.get("img");

    expect(root.attributes("id")).toBe("remote-preview");
    expect(root.attributes("data-state")).toBe("loading");
    expect(root.attributes("data-variant")).toBe("thumbnail");
    expect(image.attributes("src")).toBe(
      "remote-media:https://cdn.example/original.jpg",
    );
    expect(image.attributes("alt")).toBe("  Original alt text  ");
    expect(image.attributes("loading")).toBe("eager");
    expect(image.attributes("referrerpolicy")).toBe("no-referrer");
    expect(image.classes()).toContain("opacity-0");
    expect(wrapper.findComponent(MediaPreviewPlaceholder).exists()).toBe(true);
    expect(ipc.remoteMediaUrl).toHaveBeenCalledWith(
      "https://cdn.example/original.jpg",
    );

    await image.trigger("load");

    expect(root.attributes("data-state")).toBe("loaded");
    expect(wrapper.get("img").classes()).toContain("opacity-100");
    expect(wrapper.findComponent(MediaPreviewPlaceholder).exists()).toBe(false);
    expect(reportSuccess).toHaveBeenCalledOnce();
    expect(reportSuccess).toHaveBeenCalledWith(
      "https://cdn.example/original.jpg",
    );
    expect(reportFailure).not.toHaveBeenCalled();
  });

  it("removes a failed native image and leaves an accessible borderless placeholder", async () => {
    const { wrapper, reportSuccess, reportFailure } = render({
      source: " https://cdn.example/broken.jpg ",
      alt: "Portrait of Ada",
      variant: "avatar",
    });

    await wrapper.get("img").trigger("error");

    const root = wrapper.get("[data-remote-image]");
    const placeholder = wrapper.getComponent(MediaPreviewPlaceholder);
    expect(root.attributes("data-state")).toBe("failed");
    expect(wrapper.find("img").exists()).toBe(false);
    expect(placeholder.attributes("role")).toBe("img");
    expect(placeholder.attributes("aria-label")).toBe("Portrait of Ada");
    expect(placeholder.classes()).toContain("bg-surface-2");
    expect(placeholder.classes()).toContain("text-slate-500");
    expect(placeholder.classes().some((name) => name.includes("border"))).toBe(false);
    expect(reportFailure).toHaveBeenCalledOnce();
    expect(reportFailure).toHaveBeenCalledWith(
      "https://cdn.example/broken.jpg",
    );
    expect(reportSuccess).not.toHaveBeenCalled();
  });

  it("does not mount or report a request for empty and rejected sources", () => {
    const cases: Array<string | null | undefined> = [undefined, null, "   ", "http://unsafe.test/x.jpg"];

    for (const source of cases) {
      const { wrapper, reportSuccess, reportFailure } = render({ source });

      expect(wrapper.get("[data-remote-image]").attributes("data-state")).toBe("failed");
      expect(wrapper.find("img").exists()).toBe(false);
      expect(wrapper.findComponent(MediaPreviewPlaceholder).exists()).toBe(true);
      expect(reportFailure).not.toHaveBeenCalled();
      expect(reportSuccess).not.toHaveBeenCalled();
    }

    expect(ipc.remoteMediaUrl).toHaveBeenCalledTimes(1);
    expect(ipc.remoteMediaUrl).toHaveBeenCalledWith("http://unsafe.test/x.jpg");
  });

  it("does not treat component unmount as a network failure", () => {
    const { wrapper, reportFailure } = render();

    wrapper.unmount();

    expect(reportFailure).not.toHaveBeenCalled();
  });

  it("hides a compact decorative placeholder from assistive technology", () => {
    const { wrapper } = render({ source: null, alt: "", variant: "compact-avatar" });
    const placeholder = wrapper.getComponent(MediaPreviewPlaceholder);

    expect(placeholder.attributes("aria-hidden")).toBe("true");
    expect(placeholder.attributes("role")).toBeUndefined();
    expect(placeholder.attributes("aria-label")).toBeUndefined();
  });

  it.each<[RemoteMediaVariant, string | null, string]>([
    ["compact-avatar", "rounded-full", "object-cover"],
    ["avatar", "rounded-full", "object-cover"],
    ["story", "rounded-full", "object-cover"],
    ["thumbnail", "rounded-lg", "object-cover"],
    ["modal", null, "object-contain"],
  ])("owns %s geometry and image fit at the root", (variant, rounding, fit) => {
    const { wrapper } = render({ variant });
    const rootClasses = wrapper.get("[data-remote-image]").classes();

    expect(rootClasses).toEqual(
      expect.arrayContaining(["relative", "block", "overflow-hidden", "bg-surface-2"]),
    );
    if (rounding) {
      expect(rootClasses).toContain(rounding);
    } else {
      expect(rootClasses.some((name) => name.startsWith("rounded"))).toBe(false);
    }
    expect(wrapper.get("img").classes()).toContain(fit);
  });

  it("recreates a failed native request in the loading state after a health retry", async () => {
    const { wrapper, health, reportFailure } = render();
    const failedImage = wrapper.get("img").element;
    await wrapper.get("img").trigger("error");
    expect(wrapper.find("img").exists()).toBe(false);

    health.retryAll();
    await nextTick();

    const retriedImage = wrapper.get("img");
    expect(retriedImage.element).not.toBe(failedImage);
    expect(retriedImage.classes()).toContain("opacity-0");
    expect(wrapper.get("[data-remote-image]").attributes("data-state")).toBe("loading");
    expect(wrapper.findComponent(MediaPreviewPlaceholder).exists()).toBe(true);
    expect(reportFailure).toHaveBeenCalledOnce();
  });
});

describe("MediaPreviewPlaceholder", () => {
  it("uses a decorative user outline for avatar-like variants and an image glyph otherwise", () => {
    const avatar = mount(MediaPreviewPlaceholder, {
      props: { variant: "story", label: "Story preview", unavailable: true },
    });
    const media = mount(MediaPreviewPlaceholder, {
      props: { variant: "thumbnail", label: "Post preview", unavailable: true },
    });
    wrappers.push(avatar, media);

    expect(avatar.get("svg").attributes("data-glyph")).toBe("user-outline");
    expect(media.get("svg").attributes("data-glyph")).toBe("image-outline");
    expect(avatar.get("svg").attributes("aria-hidden")).toBe("true");
    expect(media.get("svg").attributes("aria-hidden")).toBe("true");
  });

  it("shows failure text only for an unavailable modal preview", () => {
    const failedModal = mount(MediaPreviewPlaceholder, {
      props: { variant: "modal", label: "Post preview", unavailable: true },
    });
    const loadingModal = mount(MediaPreviewPlaceholder, {
      props: { variant: "modal", label: "Post preview", unavailable: false },
    });
    const failedThumbnail = mount(MediaPreviewPlaceholder, {
      props: { variant: "thumbnail", label: "Post preview", unavailable: true },
    });
    wrappers.push(failedModal, loadingModal, failedThumbnail);

    expect(failedModal.text()).toBe("Preview unavailable");
    expect(loadingModal.text()).toBe("");
    expect(failedThumbnail.text()).toBe("");
  });
});
