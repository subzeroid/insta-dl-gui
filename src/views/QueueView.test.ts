/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => ({
  requestLibraryPreviewAccess: vi.fn(),
  libraryMediaUrl: vi.fn(),
}));

vi.mock("../lib/ipc", () => ({
  cancelJob: vi.fn(),
  onJobProgress: vi.fn(),
  formatBytes: (bytes: number) => `${bytes} B`,
  requestLibraryPreviewAccess: ipc.requestLibraryPreviewAccess,
  libraryMediaUrl: ipc.libraryMediaUrl,
  openLibraryFile: vi.fn(),
  revealLibraryFile: vi.fn(),
}));

import QueueView from "./QueueView.vue";
import { useJobsStore, type JobView } from "../stores/jobs";

function doneJob(id: string, actionable = true): JobView {
  return {
    id,
    label: `Job ${id}`,
    state: "done",
    currentFile: 0,
    totalFiles: 0,
    bytesDone: 0,
    fileName: "",
    resultCount: 1,
    catalogWarnings: 0,
    resourceFailures: 0,
    outputs: actionable
      ? [{ file_id: 10, basename: "one.jpg", kind: "photo", byte_size: 10, ordinal: 0 }]
      : undefined,
  };
}

function render() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const store = useJobsStore(pinia);
  const wrapper = mount(QueueView, {
    attachTo: document.body,
    global: { plugins: [pinia], stubs: { Teleport: true } },
  });
  return { wrapper, store };
}

beforeEach(() => {
  ipc.requestLibraryPreviewAccess.mockReset().mockResolvedValue(true);
  ipc.libraryMediaUrl.mockReset().mockImplementation((id: number) => `library://localhost/media/${id}`);
});

afterEach(() => {
  document.body.replaceChildren();
  vi.restoreAllMocks();
});

describe("QueueView completed download details", () => {
  it.each(["click", "Enter", " "])("opens exact job details via %s and restores focus on close", async (activation) => {
    const { wrapper, store } = render();
    store.jobs.set("actionable", doneJob("actionable"));
    await flushPromises();
    const card = wrapper.get("[data-job-id='actionable']");
    (card.element as HTMLElement).focus();

    if (activation === "click") await card.trigger("click");
    else await card.trigger("keydown", { key: activation });
    await flushPromises();
    expect(wrapper.get("[role='dialog']").text()).toContain("Job actionable");

    await wrapper.get("button[aria-label='Close download details']").trigger("click");
    await flushPromises();
    expect(wrapper.find("[role='dialog']").exists()).toBe(false);
    expect(document.activeElement).toBe(card.element);
  });

  it("does nothing for a legacy done card without outputs", async () => {
    const { wrapper, store } = render();
    store.jobs.set("legacy", doneJob("legacy", false));
    await flushPromises();
    const card = wrapper.get("[data-job-id='legacy']");
    await card.trigger("click");
    await card.trigger("keydown", { key: "Enter" });
    expect(wrapper.find("[role='dialog']").exists()).toBe(false);
  });

  it("closes details before Clear finished removes the originating card", async () => {
    const { wrapper, store } = render();
    store.jobs.set("actionable", doneJob("actionable"));
    await flushPromises();
    await wrapper.get("[data-job-id='actionable']").trigger("click");
    await flushPromises();
    expect(wrapper.find("[role='dialog']").exists()).toBe(true);

    await wrapper.get("button[data-action='clear-finished']").trigger("click");
    await flushPromises();
    expect(wrapper.find("[role='dialog']").exists()).toBe(false);
    expect(store.jobs.size).toBe(0);
  });

  it("closes safely if the selected job disappears", async () => {
    const { wrapper, store } = render();
    store.jobs.set("actionable", doneJob("actionable"));
    await flushPromises();
    await wrapper.get("[data-job-id='actionable']").trigger("click");
    await flushPromises();

    store.jobs.delete("actionable");
    await flushPromises();
    expect(wrapper.find("[role='dialog']").exists()).toBe(false);
  });
});
