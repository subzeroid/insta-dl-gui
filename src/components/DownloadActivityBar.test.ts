/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { JobProgress } from "../lib/ipc";

const ipc = vi.hoisted(() => ({
  listener: undefined as ((progress: JobProgress) => void) | undefined,
  onJobProgress: vi.fn(),
}));

vi.mock("../lib/ipc", () => ({
  cancelJob: vi.fn(),
  formatBytes: (bytes: number) => `${bytes} bytes`,
  onJobProgress: ipc.onJobProgress,
}));

import DownloadActivityBar from "./DownloadActivityBar.vue";
import { useJobsStore } from "../stores/jobs";

function render() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(DownloadActivityBar, {
    global: {
      plugins: [pinia],
      stubs: {
        RouterLink: {
          props: ["to"],
          template: '<a :href="to"><slot /></a>',
        },
      },
    },
  });
}

beforeEach(() => {
  ipc.listener = undefined;
  ipc.onJobProgress.mockReset();
  ipc.onJobProgress.mockImplementation(async (listener: (progress: JobProgress) => void) => {
    ipc.listener = listener;
    return () => {};
  });
});

describe("DownloadActivityBar", () => {
  it("keeps Queue reachable while downloads are idle", () => {
    const wrapper = render();

    expect(wrapper.attributes("href")).toBe("/queue");
    expect(wrapper.text()).toContain("Downloads");
    expect(wrapper.text()).toContain("No active downloads");
    expect(wrapper.find("[data-testid='download-progress']").exists()).toBe(false);
  });

  it("summarizes the active job with file and byte progress", async () => {
    const wrapper = render();
    const jobs = useJobsStore();
    await jobs.init();

    ipc.listener?.({
      job_id: "stories",
      state: "downloading",
      label: "@instagram stories",
      current_file: 5,
      total_files: 12,
      bytes_done: 1_800_000,
      file_name: "story.mp4",
    });
    await flushPromises();

    expect(wrapper.text()).toContain("@instagram stories");
    expect(wrapper.text()).toContain("1 active");
    expect(wrapper.text()).toContain("file 5/12");
    expect(wrapper.text()).toContain("1800000 bytes");
    expect(wrapper.find("[data-testid='download-progress']").exists()).toBe(true);
  });

  it("shows the number of simultaneous active jobs", async () => {
    const wrapper = render();
    const jobs = useJobsStore();
    jobs.addPlaceholder("job-1", "First");
    jobs.addPlaceholder("job-2", "Second");
    await flushPromises();

    expect(wrapper.text()).toContain("2 active");
  });
});
