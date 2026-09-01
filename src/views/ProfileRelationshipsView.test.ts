/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => ({
  fetchProfileSummary: vi.fn(),
  fetchRelationships: vi.fn(),
  searchRelationships: vi.fn(),
  remoteMediaUrl: vi.fn((url: string) => `remote-media:${url}`),
}));

vi.mock("../lib/ipc", () => ipc);

import ProfileRelationshipsView from "./ProfileRelationshipsView.vue";
import { useExplorerStore } from "../stores/explorer";

const wrappers: Array<{ unmount: () => void }> = [];

function user(pk: string, username: string) {
  return {
    pk,
    username,
    full_name: username.toUpperCase(),
    is_private: false,
    is_verified: username === "verified",
    avatar_url: `https://cdninstagram.com/${username}.jpg`,
  };
}

function render(kind: "followers" | "following" = "followers", isPrivate = false) {
  const pinia = createPinia();
  setActivePinia(pinia);
  useExplorerStore(pinia).commitProfile({
    profile: {
      pk: "42",
      username: "nike",
      media_count: 10,
      follower_count: 5,
      following_count: 4,
      is_private: isPrivate,
      is_verified: true,
    },
    recent_posts: [],
    end_cursor: null,
  });
  const wrapper = mount(ProfileRelationshipsView, {
    props: { username: "nike", kind },
    global: {
      plugins: [pinia],
      stubs: {
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

beforeEach(() => {
  vi.resetAllMocks();
  ipc.remoteMediaUrl.mockImplementation((url: string) => `remote-media:${url}`);
});

afterEach(() => {
  for (const wrapper of wrappers.splice(0)) wrapper.unmount();
  document.body.replaceChildren();
  vi.useRealTimers();
});

describe("ProfileRelationshipsView", () => {
  it("loads, merges, and counts cursor pages", async () => {
    ipc.fetchRelationships
      .mockResolvedValueOnce({
        users: [user("1", "one"), user("2", "two")],
        next_cursor: "page-2",
      })
      .mockResolvedValueOnce({
        users: [user("2", "replacement"), user("3", "three")],
        next_cursor: null,
      });
    const wrapper = render();
    await flushPromises();

    expect(ipc.fetchRelationships).toHaveBeenCalledWith("42", "followers", null);
    expect(wrapper.get("[data-page-status]").text()).toBe("Page 1 of 3");
    expect(wrapper.findAll("[data-related-user]")).toHaveLength(2);

    await wrapper.get("[data-action='load-more']").trigger("click");
    await flushPromises();
    expect(ipc.fetchRelationships).toHaveBeenLastCalledWith("42", "followers", "page-2");
    expect(wrapper.get("[data-page-status]").text()).toBe("Page 2 of 3");
    expect(wrapper.findAll("[data-related-user]")).toHaveLength(3);
    expect(wrapper.find("[data-action='load-more']").exists()).toBe(false);
  });

  it("formats very large estimated page counts", async () => {
    ipc.fetchRelationships.mockResolvedValue({
      users: Array.from({ length: 12 }, (_, index) => user(String(index), `user-${index}`)),
      next_cursor: "page-2",
    });
    const wrapper = render();
    useExplorerStore().profilePreview!.profile.follower_count = 713_000_000;
    await flushPromises();

    expect(wrapper.get("[data-page-status]").text()).toBe("Page 1 of 59,416,667");
  });

  it("debounces server-side search and restores loaded pages when cleared", async () => {
    vi.useFakeTimers();
    ipc.fetchRelationships.mockResolvedValue({
      users: [user("1", "loaded")],
      next_cursor: null,
    });
    ipc.searchRelationships.mockResolvedValue([user("9", "meta")]);
    const wrapper = render("following");
    await flushPromises();

    const input = wrapper.get<HTMLInputElement>("[data-relationship-search]");
    await input.setValue("meta");
    await vi.advanceTimersByTimeAsync(349);
    expect(ipc.searchRelationships).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    await flushPromises();

    expect(ipc.searchRelationships).toHaveBeenCalledWith("42", "following", "meta");
    expect(wrapper.get("[data-search-status]").text()).toBe("1 result");
    expect(wrapper.get("[data-related-user]").text()).toContain("meta");

    await input.setValue("");
    await flushPromises();
    expect(wrapper.get("[data-related-user]").text()).toContain("loaded");
    expect(ipc.fetchRelationships).toHaveBeenCalledTimes(1);
  });

  it("ignores stale search responses", async () => {
    vi.useFakeTimers();
    ipc.fetchRelationships.mockResolvedValue({ users: [], next_cursor: null });
    let resolveOld!: (value: ReturnType<typeof user>[]) => void;
    const old = new Promise<ReturnType<typeof user>[]>((resolve) => { resolveOld = resolve; });
    ipc.searchRelationships
      .mockReturnValueOnce(old)
      .mockResolvedValueOnce([user("2", "new-result")]);
    const wrapper = render();
    await flushPromises();
    const input = wrapper.get<HTMLInputElement>("[data-relationship-search]");

    await input.setValue("old");
    await vi.advanceTimersByTimeAsync(350);
    await input.setValue("new");
    await vi.advanceTimersByTimeAsync(350);
    await flushPromises();
    resolveOld([user("1", "stale-result")]);
    await flushPromises();

    expect(wrapper.text()).toContain("new-result");
    expect(wrapper.text()).not.toContain("stale-result");
  });

  it("links each result to that account in Explorer", async () => {
    ipc.fetchRelationships.mockResolvedValue({
      users: [user("1", "runner")],
      next_cursor: null,
    });
    const wrapper = render();
    await flushPromises();

    expect(wrapper.get("[data-related-user]").attributes("data-to")).toContain(
      '"profile":"runner"',
    );
  });

  it("reloads cleanly when the relationship route changes", async () => {
    ipc.fetchRelationships
      .mockResolvedValueOnce({ users: [user("1", "follower")], next_cursor: null })
      .mockResolvedValueOnce({ users: [user("2", "followed")], next_cursor: null });
    const wrapper = render("followers");
    await flushPromises();
    expect(wrapper.get("[data-related-user]").text()).toContain("follower");

    await wrapper.setProps({ kind: "following" });
    await flushPromises();

    expect(ipc.fetchRelationships).toHaveBeenLastCalledWith("42", "following", null);
    expect(wrapper.get("[data-related-user]").text()).toContain("followed");
    expect(wrapper.get("[data-related-user]").text()).not.toContain("follower");
  });

  it("shows an initial error and retries the same first page", async () => {
    ipc.fetchRelationships
      .mockRejectedValueOnce(new Error("relationship unavailable"))
      .mockResolvedValueOnce({ users: [user("1", "recovered")], next_cursor: null });
    const wrapper = render();
    await flushPromises();
    expect(wrapper.get("[data-list-error]").text()).toContain("relationship unavailable");

    await wrapper.get("[data-action='retry-list']").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("recovered");
    expect(ipc.fetchRelationships).toHaveBeenCalledTimes(2);
  });

  it("retries a failed load-more cursor without discarding loaded users", async () => {
    ipc.fetchRelationships
      .mockResolvedValueOnce({ users: [user("1", "first-page")], next_cursor: "page-2" })
      .mockRejectedValueOnce(new Error("next page unavailable"))
      .mockResolvedValueOnce({ users: [user("2", "second-page")], next_cursor: null });
    const wrapper = render();
    await flushPromises();

    await wrapper.get("[data-action='load-more']").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("first-page");
    expect(wrapper.get("[data-list-error]").text()).toContain("next page unavailable");

    await wrapper.get("[data-action='retry-list']").trigger("click");
    await flushPromises();
    expect(ipc.fetchRelationships).toHaveBeenLastCalledWith("42", "followers", "page-2");
    expect(wrapper.text()).toContain("first-page");
    expect(wrapper.text()).toContain("second-page");
    expect(wrapper.get("[data-page-status]").text()).toBe("Page 2 of 5");
  });

  it("does not request unavailable relationship data for a private profile", async () => {
    const wrapper = render("followers", true);
    await flushPromises();

    expect(ipc.fetchRelationships).not.toHaveBeenCalled();
    expect(wrapper.get("[data-private-profile]").text()).toContain(
      "Followers and following lists are unavailable for private profiles.",
    );
    expect(wrapper.get<HTMLInputElement>("[data-relationship-search]").element.disabled).toBe(true);
  });
});
