/** @vitest-environment happy-dom */

import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => ({
  requestLibraryPreviewAccess: vi.fn(),
  libraryMediaUrl: vi.fn(),
  openLibraryFile: vi.fn(),
  revealLibraryFile: vi.fn(),
}));

vi.mock("../lib/ipc", () => ({
  ...ipc,
  formatBytes: (bytes: number) => `${bytes} B`,
}));

import DownloadJobDetails from "./DownloadJobDetails.vue";
import type { JobView } from "../stores/jobs";

const wrappers: Array<{ unmount: () => void }> = [];

function job(overrides: Partial<JobView> = {}): JobView {
  return {
    id: "job-1",
    label: "@nike posts · selected · 4",
    state: "done",
    currentFile: 0,
    totalFiles: 0,
    bytesDone: 0,
    fileName: "",
    resultCount: 5,
    resultDir: "/private/archive",
    catalogWarnings: 0,
    resourceFailures: 0,
    requestedItems: 4,
    outputs: [
      { file_id: 101, basename: "first.jpg", kind: "photo", byte_size: 1024, ordinal: 0 },
      { file_id: 102, basename: "second.mp4", kind: "video", byte_size: 2048, ordinal: 1 },
      { basename: "third.jpg", kind: "photo", byte_size: 3072, ordinal: 2 },
      { file_id: 104, basename: "fourth.jpg", kind: "photo", byte_size: 4096, ordinal: 3 },
      { file_id: 105, basename: "fifth.jpg", kind: "photo", byte_size: 5120, ordinal: 4 },
    ],
    ...overrides,
  };
}

function render(value = job()) {
  const wrapper = mount(DownloadJobDetails, {
    props: { job: value },
    attachTo: document.body,
    global: { stubs: { Teleport: true } },
  });
  wrappers.push(wrapper);
  return wrapper;
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
  ipc.requestLibraryPreviewAccess.mockReset().mockResolvedValue(true);
  ipc.libraryMediaUrl.mockReset().mockImplementation((id: number) => `library://localhost/media/${id}`);
  ipc.openLibraryFile.mockReset().mockResolvedValue(undefined);
  ipc.revealLibraryFile.mockReset().mockResolvedValue(undefined);
});

afterEach(() => {
  for (const wrapper of wrappers.splice(0)) wrapper.unmount();
  document.body.replaceChildren();
  vi.restoreAllMocks();
});

describe("DownloadJobDetails", () => {
  it("renders the exact ordered outputs and explains item-to-file expansion", () => {
    const wrapper = render();
    const dialog = wrapper.get("[role='dialog']");
    expect(dialog.attributes("aria-modal")).toBe("true");
    expect(dialog.text()).toContain("@nike posts · selected · 4");
    expect(dialog.text()).toContain("4 items requested · 5 files saved");
    const rows = wrapper.findAll("[data-output-row]");
    expect(rows).toHaveLength(5);
    expect(rows.map((row) => row.get("[data-output-basename]").text())).toEqual([
      "first.jpg",
      "second.mp4",
      "third.jpg",
      "fourth.jpg",
      "fifth.jpg",
    ]);
    expect(rows[0].text()).toContain("Photo");
    expect(rows[0].text()).toContain("File 1");
    expect(rows[0].text()).toContain("1024 B");
    expect(rows[1].text()).toContain("Video");
  });

  it("falls back to a file-only summary when requested item count is unknown", () => {
    const wrapper = render(job({ requestedItems: undefined }));
    expect(wrapper.get("[data-output-summary]").text()).toBe("5 files saved");
  });

  it("uses one indexed access probe before creating photo and video preview URLs", async () => {
    const access = deferred<boolean>();
    ipc.requestLibraryPreviewAccess.mockReturnValueOnce(access.promise);
    const wrapper = render();

    expect(ipc.requestLibraryPreviewAccess).toHaveBeenCalledTimes(1);
    expect(ipc.requestLibraryPreviewAccess).toHaveBeenCalledWith(101);
    expect(ipc.libraryMediaUrl).not.toHaveBeenCalled();
    expect(wrapper.find("img[data-output-preview]").exists()).toBe(false);

    access.resolve(true);
    await flushPromises();

    expect(ipc.requestLibraryPreviewAccess).toHaveBeenCalledTimes(1);
    expect(ipc.libraryMediaUrl.mock.calls.map((call) => call[0])).toEqual([101, 102, 104, 105]);
    expect(wrapper.get("img[data-file-id='101']").attributes("src")).toBe("library://localhost/media/101");
    expect(wrapper.get("video[data-file-id='102']").attributes("src")).toBe("library://localhost/media/102");
    expect(wrapper.get("video[data-file-id='102']").attributes("controls")).toBeDefined();
  });

  it.each([
    ["denied", false],
    ["failed", new Error("permission IPC failed")],
  ])("shows a concise preview %s state without prompting repeatedly", async (_label, outcome) => {
    if (outcome instanceof Error) ipc.requestLibraryPreviewAccess.mockRejectedValueOnce(outcome);
    else ipc.requestLibraryPreviewAccess.mockResolvedValueOnce(outcome);
    const wrapper = render();
    await flushPromises();
    await wrapper.setProps({ job: { ...job(), label: "Same job, updated label" } });
    await flushPromises();

    expect(ipc.requestLibraryPreviewAccess).toHaveBeenCalledTimes(1);
    expect(ipc.libraryMediaUrl).not.toHaveBeenCalled();
    expect(wrapper.get("[data-preview-state]").text()).toContain("Preview unavailable");
  });

  it("lists unindexed files, disables their actions, and never sends basename or path to IPC", async () => {
    const wrapper = render();
    await flushPromises();
    const row = wrapper.findAll("[data-output-row]")[2];

    expect(row.text()).toContain("Not indexed");
    expect(row.text()).toContain("Rescan Library");
    expect(row.get("[data-action='open-output']").attributes("disabled")).toBeDefined();
    expect(row.get("[data-action='reveal-output']").attributes("disabled")).toBeDefined();
    await row.get("[data-action='open-output']").trigger("click");
    await row.get("[data-action='reveal-output']").trigger("click");
    expect(ipc.openLibraryFile).not.toHaveBeenCalled();
    expect(ipc.revealLibraryFile).not.toHaveBeenCalled();
  });

  it("opens and reveals only by numeric file id and reports errors on the affected row", async () => {
    ipc.openLibraryFile.mockRejectedValueOnce(new Error("File moved"));
    ipc.revealLibraryFile.mockRejectedValueOnce("Reveal unavailable");
    const wrapper = render();
    await flushPromises();
    const first = wrapper.findAll("[data-output-row]")[0];

    await first.get("[data-action='open-output']").trigger("click");
    await flushPromises();
    expect(ipc.openLibraryFile).toHaveBeenCalledWith(101);
    expect(wrapper.get("[data-output-row='0'] [data-row-error]").text()).toContain("File moved");

    await first.get("[data-action='reveal-output']").trigger("click");
    await flushPromises();
    expect(ipc.revealLibraryFile).toHaveBeenCalledWith(101);
    expect(wrapper.get("[data-output-row='0'] [data-row-error]").text()).toContain("Reveal unavailable");
    expect(typeof ipc.openLibraryFile.mock.calls[0][0]).toBe("number");
    expect(typeof ipc.revealLibraryFile.mock.calls[0][0]).toBe("number");
  });

  it("renders malicious-looking basenames as inert text", () => {
    const malicious = '<img src=x onerror="window.pwned=true">';
    const wrapper = render(job({
      requestedItems: 1,
      outputs: [{ file_id: 9, basename: malicious, kind: "photo", byte_size: 1, ordinal: 0 }],
    }));

    expect(wrapper.get("[data-output-basename]").text()).toBe(malicious);
    expect(wrapper.find("[data-output-basename] img").exists()).toBe(false);
  });

  it("closes by button, backdrop, and Escape", async () => {
    const button = render();
    await button.get("button[aria-label='Close download details']").trigger("click");
    expect(button.emitted("close")).toHaveLength(1);

    const backdrop = render();
    await backdrop.get("[data-testid='download-details-backdrop']").trigger("click");
    expect(backdrop.emitted("close")).toHaveLength(1);

    const escape = render();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(escape.emitted("close")).toHaveLength(1);
  });

  it("focuses inside, traps Tab both ways, and removes its listener on unmount", async () => {
    const add = vi.spyOn(window, "addEventListener");
    const remove = vi.spyOn(window, "removeEventListener");
    ipc.requestLibraryPreviewAccess.mockReturnValueOnce(new Promise<boolean>(() => {}));
    const wrapper = render(job({ outputs: [{ file_id: 1, basename: "one.jpg", kind: "photo", byte_size: 1, ordinal: 0 }] }));
    await flushPromises();
    const close = wrapper.get("button[aria-label='Close download details']");
    const buttons = wrapper.findAll("[role='dialog'] button:not([disabled])");
    const last = buttons.at(-1)!;
    expect(document.activeElement).toBe(close.element);

    (close.element as HTMLElement).focus();
    await close.trigger("keydown", { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last.element);
    (last.element as HTMLElement).focus();
    await last.trigger("keydown", { key: "Tab" });
    expect(document.activeElement).toBe(close.element);

    const keyHandler = add.mock.calls.find((call) => call[0] === "keydown")?.[1];
    wrapper.unmount();
    expect(remove).toHaveBeenCalledWith("keydown", keyHandler);
  });

  it("invalidates pending access after close, unmount, and job switch", async () => {
    const closePending = deferred<boolean>();
    ipc.requestLibraryPreviewAccess.mockReturnValueOnce(closePending.promise);
    const closing = render();
    await closing.get("button[aria-label='Close download details']").trigger("click");
    closePending.resolve(true);
    await flushPromises();
    expect(ipc.libraryMediaUrl).not.toHaveBeenCalled();

    const unmountPending = deferred<boolean>();
    ipc.requestLibraryPreviewAccess.mockReturnValueOnce(unmountPending.promise);
    const unmounting = render(job({ id: "job-2" }));
    unmounting.unmount();
    unmountPending.resolve(true);
    await flushPromises();
    expect(ipc.libraryMediaUrl).not.toHaveBeenCalled();

    const switchPending = deferred<boolean>();
    ipc.requestLibraryPreviewAccess.mockReturnValueOnce(switchPending.promise);
    const switching = render(job({ id: "job-3" }));
    await switching.setProps({ job: job({ id: "job-4", outputs: [] }) });
    switchPending.resolve(true);
    await flushPromises();
    expect(ipc.libraryMediaUrl).not.toHaveBeenCalled();
  });
});
