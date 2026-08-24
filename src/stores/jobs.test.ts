import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { JobProgress } from "../lib/ipc";

const ipc = vi.hoisted(() => ({
  listener: undefined as ((progress: JobProgress) => void) | undefined,
  onJobProgress: vi.fn(),
}));

vi.mock("../lib/ipc", () => ({
  cancelJob: vi.fn(),
  onJobProgress: ipc.onJobProgress,
}));

import { useJobsStore } from "./jobs";

beforeEach(() => {
  setActivePinia(createPinia());
  ipc.listener = undefined;
  ipc.onJobProgress.mockReset();
  ipc.onJobProgress.mockImplementation(async (listener: (progress: JobProgress) => void) => {
    ipc.listener = listener;
    return () => {};
  });
});

describe("jobs warning state", () => {
  it("treats warning fields omitted by legacy done events as zero", async () => {
    const store = useJobsStore();
    await store.init();

    ipc.listener?.({
      job_id: "legacy",
      state: "done",
      label: "Legacy",
      count: 2,
      dir: "/archive",
    });

    expect(store.jobs.get("legacy")).toMatchObject({
      resultCount: 2,
      catalogWarnings: 0,
      resourceFailures: 0,
    });
  });

  it("retains backend warning counts without retaining raw failure details", async () => {
    const store = useJobsStore();
    await store.init();

    ipc.listener?.({
      job_id: "partial",
      state: "done",
      label: "Partial",
      count: 3,
      dir: "/archive",
      catalog_warnings: 1,
      resource_failures: 2,
    });
    ipc.listener?.({
      job_id: "partial",
      state: "done",
      label: "Partial",
      count: 3,
      dir: "/archive",
    });

    const job = store.jobs.get("partial");
    expect(job).toMatchObject({
      resultCount: 3,
      catalogWarnings: 1,
      resourceFailures: 2,
    });
    expect(JSON.stringify(job)).not.toContain("token");
    expect(JSON.stringify(job)).not.toContain("https://");
  });
});
