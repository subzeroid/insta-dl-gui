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

describe("jobs conflict metadata", () => {
  it("reports active conflicts and releases them only on terminal progress", async () => {
    const store = useJobsStore();
    await store.init();
    store.addPlaceholder("job-1", "Explore posts", [
      "profile:nike",
      "folder:nike:posts",
      "folder:nike:posts",
    ]);

    expect(store.jobs.get("job-1")?.conflictKeys).toEqual([
      "profile:nike",
      "folder:nike:posts",
    ]);
    expect(store.hasActiveConflict(["folder:nike:posts"])).toBe(true);
    expect(store.hasActiveConflict(["folder:nike:stories"])).toBe(false);

    ipc.listener?.({
      job_id: "job-1",
      state: "done",
      label: "Explore posts",
      count: 1,
    });

    expect(store.hasActiveConflict(["profile:nike", "folder:nike:posts"])).toBe(false);
    expect(store.jobs.get("job-1")?.conflictKeys).toEqual([
      "profile:nike",
      "folder:nike:posts",
    ]);
  });

  it("attaches conflicts after an early terminal event without regressing job state", async () => {
    const store = useJobsStore();
    await store.init();
    ipc.listener?.({
      job_id: "early",
      state: "done",
      label: "Backend label",
      count: 2,
      dir: "/archive",
    });

    store.addPlaceholder("early", "Placeholder label", ["folder:nike:posts"]);

    expect(store.jobs.get("early")).toMatchObject({
      state: "done",
      label: "Backend label",
      resultCount: 2,
      conflictKeys: ["folder:nike:posts"],
    });
    expect(store.hasActiveConflict(["folder:nike:posts"])).toBe(false);
  });

  it("matches either key in a dual-key active job and preserves metadata through updates", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const store = useJobsStore(pinia);
    await store.init();
    store.addPlaceholder("dual", "All posts", ["profile:nike", "folder:nike:posts"]);
    ipc.listener?.({
      job_id: "dual",
      state: "downloading",
      label: "All posts",
      current_file: 1,
      total_files: 3,
    });

    expect(store.hasActiveConflict(["profile:nike"])).toBe(true);
    expect(store.hasActiveConflict(["folder:nike:posts"])).toBe(true);
    expect(store.hasActiveConflict(["folder:adidas:posts"])).toBe(false);
    expect(useJobsStore(pinia).jobs.get("dual")?.conflictKeys).toEqual([
      "profile:nike",
      "folder:nike:posts",
    ]);
  });
});
