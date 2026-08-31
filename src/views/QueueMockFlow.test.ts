/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { DOMWrapper, flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { enqueueFetchedPostDownload, type Post } from "../lib/ipc";
import { installTauriMock, uninstallTauriMock } from "../lib/mock";
import { useJobsStore } from "../stores/jobs";
import QueueView from "./QueueView.vue";

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

let wrapper: VueWrapper | undefined;

beforeEach(() => {
  vi.useFakeTimers();
  window.history.replaceState({}, "", "/queue?mock=1");
  installTauriMock();
});

afterEach(() => {
  wrapper?.unmount();
  wrapper = undefined;
  uninstallTauriMock();
  vi.runOnlyPendingTimers();
  vi.useRealTimers();
  document.body.replaceChildren();
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  delete (window as unknown as { __TAURI_EVENT_PLUGIN_INTERNALS__?: unknown })
    .__TAURI_EVENT_PLUGIN_INTERNALS__;
  window.history.replaceState({}, "", "/");
  vi.restoreAllMocks();
});

describe("Queue mock download journey", () => {
  it("turns an Explore snapshot into actionable exact output details", async () => {
    const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: Invoke } })
      .__TAURI_INTERNALS__;
    const realInvoke = internals.invoke;
    const invokeSpy = vi.fn((cmd: string, args?: Record<string, unknown>) => realInvoke(cmd, args));
    internals.invoke = invokeSpy;

    const pinia = createPinia();
    setActivePinia(pinia);
    const jobs = useJobsStore(pinia);
    await jobs.init();
    wrapper = mount(QueueView, {
      attachTo: document.body,
      global: { plugins: [pinia] },
    });
    const posts: Post[] = [
      { pk: "1", code: "ONE", resources: [{ url: "https://cdninstagram.com/one.jpg", kind: "photo" }] },
      { pk: "2", code: "TWO", resources: [{ url: "https://cdninstagram.com/two.mp4", kind: "video" }] },
      {
        pk: "3",
        code: "THREE",
        resources: [
          { url: "https://cdninstagram.com/three.jpg", kind: "photo" },
          { url: "https://cdninstagram.com/three.mp4", kind: "video" },
        ],
      },
      { pk: "4", code: "FOUR", resources: [{ url: "https://cdninstagram.com/four.jpg", kind: "photo" }] },
    ];

    const jobId = await enqueueFetchedPostDownload("nike", "posts", "selected", posts);
    jobs.addPlaceholder(jobId, "@nike posts · selected · 4");
    await flushPromises();
    expect(wrapper.get(`[data-job-id='${jobId}']`).text()).toContain("fetching");

    await vi.advanceTimersByTimeAsync(15);
    await flushPromises();
    expect(wrapper.get(`[data-job-id='${jobId}']`).text()).toContain("downloading");

    await vi.runAllTimersAsync();
    await flushPromises();
    const card = wrapper.get(`[data-job-id='${jobId}']`);
    expect(card.attributes("role")).toBe("button");
    expect(card.text()).toContain("5 files");

    await card.trigger("click");
    await flushPromises();
    const dialog = new DOMWrapper(document.body).get("[role='dialog']");
    expect(dialog.get("[data-output-summary]").text()).toBe(
      "4 items requested · 5 files saved",
    );
    const rows = dialog.findAll("[data-output-row]");
    expect(rows).toHaveLength(5);
    expect(rows.map((row) => row.get("[data-output-basename]").text())).toEqual([
      "ONE_1.jpg",
      "TWO_1.mp4",
      "THREE_1.jpg",
      "THREE_2.mp4",
      "FOUR_1.jpg",
    ]);

    const probes = invokeSpy.mock.calls.filter(([cmd]) => cmd === "request_library_preview_access");
    expect(probes).toHaveLength(1);
    expect(probes[0]?.[1]?.fileId).toBe(10101);
    const previewSources = dialog
      .findAll("[data-output-preview]")
      .map((preview) => preview.attributes("src"));
    expect(previewSources).toHaveLength(5);
    expect(previewSources[0]).toMatch(/^data:image\/svg\+xml,/);
    expect(previewSources[1]).toMatch(/^data:video\/mp4;base64,/);
    expect(previewSources[2]).toMatch(/^data:image\/svg\+xml,/);
    expect(previewSources[3]).toMatch(/^data:video\/mp4;base64,/);
    expect(previewSources[4]).toMatch(/^data:image\/svg\+xml,/);
    expect(previewSources.some((source) => source?.startsWith("library://"))).toBe(false);

    await rows[0].get("[data-action='open-output']").trigger("click");
    await flushPromises();
    await rows[1].get("[data-action='reveal-output']").trigger("click");
    await flushPromises();
    expect(
      invokeSpy.mock.calls
        .filter(([cmd]) => cmd === "open_library_file")
        .map(([cmd, args]) => [cmd, args]),
    ).toEqual([["open_library_file", { fileId: 10101 }]]);
    expect(
      invokeSpy.mock.calls
        .filter(([cmd]) => cmd === "reveal_library_file")
        .map(([cmd, args]) => [cmd, args]),
    ).toEqual([["reveal_library_file", { fileId: 10102 }]]);
  });
});
