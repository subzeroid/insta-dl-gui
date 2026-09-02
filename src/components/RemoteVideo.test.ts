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
import RemoteVideo from "./RemoteVideo.vue";
import { useRemoteMediaHealthStore } from "../stores/remoteMediaHealth";

type RemoteVideoProps = InstanceType<typeof RemoteVideo>["$props"];

const wrappers: Array<{ unmount: () => void }> = [];

function render(props: Partial<RemoteVideoProps> = {}) {
  const pinia = createPinia();
  setActivePinia(pinia);
  const health = useRemoteMediaHealthStore(pinia);
  const reportSuccess = vi.spyOn(health, "reportSuccess");
  const reportFailure = vi.spyOn(health, "reportFailure");
  const wrapper = mount(RemoteVideo, {
    props: {
      source: "https://cdn.example/preview.mp4",
      label: "Post video preview",
      ...props,
    },
    attrs: { id: "remote-video-preview" },
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

describe("RemoteVideo", () => {
  it("keeps a valid native video hidden and non-interactive while metadata loads", () => {
    const { wrapper } = render({
      source: "  https://cdn.example/original.mp4  ",
      label: "Original video",
    });
    const root = wrapper.get("[data-remote-video]");
    const video = wrapper.get("video");
    const placeholder = wrapper.getComponent(MediaPreviewPlaceholder);

    expect(root.attributes("id")).toBe("remote-video-preview");
    expect(root.attributes("data-state")).toBe("loading");
    expect(root.classes()).toEqual(
      expect.arrayContaining([
        "relative",
        "block",
        "overflow-hidden",
        "bg-black",
      ]),
    );
    expect(video.attributes("src")).toBe(
      "remote-media:https://cdn.example/original.mp4",
    );
    expect(video.attributes("controls")).toBeUndefined();
    expect(video.attributes("preload")).toBe("metadata");
    expect(video.attributes("aria-hidden")).toBe("true");
    expect(video.attributes("tabindex")).toBe("-1");
    expect(video.classes()).toEqual(
      expect.arrayContaining([
        "object-contain",
        "transition-opacity",
        "opacity-0",
        "pointer-events-none",
      ]),
    );
    expect(placeholder.props("variant")).toBe("modal");
    expect(placeholder.props("label")).toBe("Original video");
    expect(placeholder.props("unavailable")).toBe(false);
    expect(ipc.remoteMediaUrl).toHaveBeenCalledWith(
      "https://cdn.example/original.mp4",
    );
  });

  it("reveals on loadedmetadata and reports success for the trimmed original source", async () => {
    const { wrapper, reportSuccess, reportFailure } = render({
      source: "  https://cdn.example/original.mp4  ",
      label: "Original video",
    });
    const root = wrapper.get("[data-remote-video]");

    await wrapper.get("video").trigger("loadedmetadata");

    expect(root.attributes("data-state")).toBe("loaded");
    const video = wrapper.get("video");
    expect(video.classes()).toContain("opacity-100");
    expect(video.classes()).not.toContain("pointer-events-none");
    expect(video.attributes("controls")).toBe("");
    expect(video.attributes("aria-label")).toBe("Original video");
    expect(video.attributes("aria-hidden")).toBeUndefined();
    expect(video.attributes("tabindex")).toBeUndefined();
    expect(wrapper.findComponent(MediaPreviewPlaceholder).exists()).toBe(false);
    expect(reportSuccess).toHaveBeenCalledOnce();
    expect(reportSuccess).toHaveBeenCalledWith(
      "https://cdn.example/original.mp4",
    );
    expect(reportFailure).not.toHaveBeenCalled();
  });

  it("keeps controls disabled after metadata loads when controls=false", async () => {
    const { wrapper } = render({ controls: false });

    await wrapper.get("video").trigger("loadedmetadata");

    expect(wrapper.get("video").attributes("controls")).toBeUndefined();
  });

  it("removes a failed native video and displays the shared modal failure placeholder", async () => {
    const { wrapper, reportSuccess, reportFailure } = render({
      source: " https://cdn.example/broken.mp4 ",
      label: "Broken video",
    });

    await wrapper.get("video").trigger("error");

    const root = wrapper.get("[data-remote-video]");
    const placeholder = wrapper.getComponent(MediaPreviewPlaceholder);
    expect(root.attributes("data-state")).toBe("failed");
    expect(wrapper.find("video").exists()).toBe(false);
    expect(placeholder.props("variant")).toBe("modal");
    expect(placeholder.props("unavailable")).toBe(true);
    expect(placeholder.text()).toBe("Preview unavailable");
    expect(reportFailure).toHaveBeenCalledOnce();
    expect(reportFailure).toHaveBeenCalledWith(
      "https://cdn.example/broken.mp4",
    );
    expect(reportSuccess).not.toHaveBeenCalled();
  });

  it("recreates a fresh native video without changing its URL after retryAll", async () => {
    const { wrapper, health, reportFailure } = render();
    const failedVideo = wrapper.get("video").element;
    const originalUrl = wrapper.get("video").attributes("src");

    await wrapper.get("video").trigger("error");
    expect(wrapper.find("video").exists()).toBe(false);

    health.retryAll();
    await nextTick();

    const retriedVideo = wrapper.get("video");
    expect(retriedVideo.element).not.toBe(failedVideo);
    expect(retriedVideo.attributes("src")).toBe(originalUrl);
    expect(retriedVideo.attributes("src")).toBe(
      "remote-media:https://cdn.example/preview.mp4",
    );
    expect(retriedVideo.attributes("src")).not.toContain("?");
    expect(retriedVideo.classes()).toContain("opacity-0");
    expect(wrapper.get("[data-remote-video]").attributes("data-state")).toBe(
      "loading",
    );
    expect(
      wrapper.getComponent(MediaPreviewPlaceholder).props("unavailable"),
    ).toBe(false);
    expect(reportFailure).toHaveBeenCalledOnce();
  });

  it("does not mount or report a network failure for empty and rejected sources", () => {
    const cases: Array<string | null | undefined> = [
      undefined,
      null,
      "   ",
      "http://unsafe.test/video.mp4",
    ];

    for (const source of cases) {
      const { wrapper, reportSuccess, reportFailure } = render({ source });

      expect(wrapper.get("[data-remote-video]").attributes("data-state")).toBe(
        "failed",
      );
      expect(wrapper.find("video").exists()).toBe(false);
      expect(wrapper.getComponent(MediaPreviewPlaceholder).text()).toBe(
        "Preview unavailable",
      );
      expect(reportFailure).not.toHaveBeenCalled();
      expect(reportSuccess).not.toHaveBeenCalled();
    }

    expect(ipc.remoteMediaUrl).toHaveBeenCalledTimes(1);
    expect(ipc.remoteMediaUrl).toHaveBeenCalledWith(
      "http://unsafe.test/video.mp4",
    );
  });

  it("starts a fresh loading request when the original source changes", async () => {
    const { wrapper, reportSuccess, reportFailure } = render();
    const firstVideo = wrapper.get("video").element;
    await wrapper.get("video").trigger("loadedmetadata");

    await wrapper.setProps({ source: " https://cdn.example/next.mp4 " });

    const nextVideo = wrapper.get("video");
    expect(nextVideo.element).not.toBe(firstVideo);
    expect(nextVideo.attributes("src")).toBe(
      "remote-media:https://cdn.example/next.mp4",
    );
    expect(nextVideo.classes()).toContain("opacity-0");
    expect(wrapper.get("[data-remote-video]").attributes("data-state")).toBe(
      "loading",
    );
    expect(wrapper.findComponent(MediaPreviewPlaceholder).exists()).toBe(true);
    expect(reportSuccess).toHaveBeenCalledOnce();
    expect(reportFailure).not.toHaveBeenCalled();
  });
});
