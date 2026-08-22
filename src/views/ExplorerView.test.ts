/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => ({
  downloadDirect: vi.fn(),
  downloadPost: vi.fn(),
  enqueueProfileDownload: vi.fn(),
  fetchProfile: vi.fn(),
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function render() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(ExplorerView, {
    global: {
      plugins: [pinia],
      stubs: { JobCard: true, PostModal: true },
    },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("ExplorerView async wiring", () => {
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
});
