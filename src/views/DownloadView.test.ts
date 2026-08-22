/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => ({
  downloadPost: vi.fn(),
  enqueueProfileDownload: vi.fn(),
  fetchProfile: vi.fn(),
  resolveInput: vi.fn(),
}));

vi.mock("../lib/ipc", () => ({
  ...ipc,
  cancelJob: vi.fn(),
  configState: vi.fn(),
  formatBytes: (n: number) => `${n} B`,
  getBalance: vi.fn(),
  onJobProgress: vi.fn(),
  saveSettings: vi.fn(),
}));

import DownloadView from "./DownloadView.vue";

const publicPreview = {
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

function render() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(DownloadView, {
    global: {
      plugins: [pinia],
      stubs: { JobCard: true },
    },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("DownloadView concurrency", () => {
  it("accepts only one lookup while input resolution is pending", async () => {
    const pending = deferred<{ kind: "profile"; username: string }>();
    ipc.resolveInput.mockReturnValue(pending.promise);
    const wrapper = render();
    await wrapper.get("input").setValue("nike");

    await wrapper.get("form").trigger("submit");
    await wrapper.get("form").trigger("submit");

    expect(ipc.resolveInput).toHaveBeenCalledTimes(1);
    pending.resolve({ kind: "profile", username: "nike" });
    ipc.fetchProfile.mockResolvedValue(publicPreview);
    await flushPromises();
  });

  it("enqueues only one profile download while the first call is pending", async () => {
    ipc.resolveInput.mockResolvedValue({ kind: "profile", username: "nike" });
    ipc.fetchProfile.mockResolvedValue(publicPreview);
    const pending = deferred<string>();
    ipc.enqueueProfileDownload.mockReturnValue(pending.promise);
    const wrapper = render();
    await wrapper.get("input").setValue("nike");
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    const download = wrapper.findAll("button").find((button) => button.text() === "Download");
    expect(download).toBeDefined();

    await download!.trigger("click");
    await download!.trigger("click");

    expect(ipc.enqueueProfileDownload).toHaveBeenCalledTimes(1);
    pending.resolve("job-1");
    await flushPromises();
  });

  it("blocks an empty private-profile download", async () => {
    ipc.resolveInput.mockResolvedValue({ kind: "profile", username: "nike" });
    ipc.fetchProfile.mockResolvedValue({
      ...publicPreview,
      profile: { ...publicPreview.profile, is_private: true },
    });
    const wrapper = render();
    await wrapper.get("input").setValue("nike");
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    const avatar = wrapper.get('input[type="checkbox"]');
    await avatar.setValue(false);
    const download = wrapper.findAll("button").find((button) => button.text() === "Download");

    expect(download?.attributes("disabled")).toBeDefined();
    await download!.trigger("click");
    expect(ipc.enqueueProfileDownload).not.toHaveBeenCalled();
  });
});
