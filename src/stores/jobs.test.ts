import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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

afterEach(() => {
  vi.useRealTimers();
});

describe("jobs timing metadata", () => {
  it("captures the placeholder start and the first terminal event without rewriting either", async () => {
    vi.useFakeTimers();
    const startedAt = Date.parse("2026-09-01T09:10:11.000Z");
    const finishedAt = Date.parse("2026-09-01T09:12:13.000Z");
    const store = useJobsStore();

    vi.setSystemTime(startedAt);
    store.addPlaceholder("timed", "Timed download");
    expect(store.jobs.get("timed")?.startedAt).toBe(startedAt);
    expect(store.jobs.get("timed")?.finishedAt).toBeUndefined();

    await store.init();
    vi.setSystemTime(Date.parse("2026-09-01T09:11:12.000Z"));
    ipc.listener?.({
      job_id: "timed",
      state: "downloading",
      label: "Timed download",
      current_file: 1,
      total_files: 2,
    });
    expect(store.jobs.get("timed")?.startedAt).toBe(startedAt);
    expect(store.jobs.get("timed")?.finishedAt).toBeUndefined();

    vi.setSystemTime(finishedAt);
    ipc.listener?.({
      job_id: "timed",
      state: "done",
      label: "Timed download",
      count: 2,
    });
    expect(store.jobs.get("timed")?.finishedAt).toBe(finishedAt);

    vi.setSystemTime(Date.parse("2026-09-01T09:15:16.000Z"));
    ipc.listener?.({
      job_id: "timed",
      state: "done",
      label: "Timed download",
      count: 2,
    });
    expect(store.jobs.get("timed")).toMatchObject({ startedAt, finishedAt });
  });

  it.each(["done", "failed", "cancelled"] as const)(
    "timestamps an early %s event as both the observed start and finish",
    async (state) => {
      vi.useFakeTimers();
      const observedAt = Date.parse("2026-09-01T10:20:30.000Z");
      vi.setSystemTime(observedAt);
      const store = useJobsStore();
      await store.init();

      const progress: JobProgress = {
        job_id: `early-${state}`,
        state,
        label: "Early terminal event",
        ...(state === "done" ? { count: 1 } : {}),
        ...(state === "failed" ? { error: "network" } : {}),
      };
      ipc.listener?.(progress);

      expect(store.jobs.get(`early-${state}`)).toMatchObject({
        startedAt: observedAt,
        finishedAt: observedAt,
      });
    },
  );
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

describe("jobs completed output metadata", () => {
  it("maps safe output metadata and requested item count from a done event", async () => {
    const store = useJobsStore();
    await store.init();

    ipc.listener?.({
      job_id: "selected-posts",
      state: "done",
      label: "@nike posts · selected · 4",
      count: 5,
      requested_items: 4,
      outputs: [
        { file_id: 41, basename: "one.jpg", kind: "photo", byte_size: 1200, ordinal: 0 },
        { basename: "two.mp4", kind: "video", byte_size: 3400, ordinal: 1 },
      ],
    });

    expect(store.jobs.get("selected-posts")).toMatchObject({
      requestedItems: 4,
      outputs: [
        { file_id: 41, basename: "one.jpg", kind: "photo", byte_size: 1200, ordinal: 0 },
        { basename: "two.mp4", kind: "video", byte_size: 3400, ordinal: 1 },
      ],
    });
    expect(JSON.stringify(store.jobs.get("selected-posts"))).not.toContain("path");
  });

  it("replaces output metadata on repeated done events instead of retaining stale files", async () => {
    const store = useJobsStore();
    await store.init();
    ipc.listener?.({
      job_id: "repeat",
      state: "done",
      label: "First terminal event",
      requested_items: 2,
      outputs: [
        { file_id: 1, basename: "stale.jpg", kind: "photo", byte_size: 1, ordinal: 0 },
      ],
    });

    ipc.listener?.({
      job_id: "repeat",
      state: "done",
      label: "Second terminal event",
      requested_items: 1,
      outputs: [
        { file_id: 2, basename: "fresh.mp4", kind: "video", byte_size: 2, ordinal: 0 },
      ],
    });

    expect(store.jobs.get("repeat")?.requestedItems).toBe(1);
    expect(store.jobs.get("repeat")?.outputs).toEqual([
      { file_id: 2, basename: "fresh.mp4", kind: "video", byte_size: 2, ordinal: 0 },
    ]);
  });

  it("clears output metadata for a legacy replacement and keeps placeholders non-actionable", async () => {
    const store = useJobsStore();
    store.addPlaceholder("placeholder", "Waiting");
    expect(store.jobs.get("placeholder")?.outputs).toBeUndefined();
    expect(store.jobs.get("placeholder")?.requestedItems).toBeUndefined();

    await store.init();
    ipc.listener?.({
      job_id: "legacy-replacement",
      state: "done",
      label: "First",
      requested_items: 1,
      outputs: [
        { file_id: 3, basename: "indexed.jpg", kind: "photo", byte_size: 3, ordinal: 0 },
      ],
    });
    ipc.listener?.({
      job_id: "legacy-replacement",
      state: "done",
      label: "Legacy",
      count: 1,
    });

    expect(store.jobs.get("legacy-replacement")?.outputs).toBeUndefined();
    expect(store.jobs.get("legacy-replacement")?.requestedItems).toBeUndefined();
  });
});

describe("jobs conflict metadata", () => {
  it("reserves pending conflicts atomically and releases them by token", () => {
    const store = useJobsStore();
    const first = Symbol("first enqueue");
    const conflicting = Symbol("conflicting enqueue");
    const unrelated = Symbol("unrelated enqueue");

    expect(store.reserveConflictKeys(first, ["folder:nike:posts", "folder:nike:posts"])).toBe(true);
    expect(store.hasActiveConflict(["folder:nike:posts"])).toBe(true);
    expect(store.reserveConflictKeys(conflicting, ["folder:nike:posts"])).toBe(false);
    expect(store.reserveConflictKeys(unrelated, ["folder:nike:stories"])).toBe(true);

    store.releaseConflictKeys(first);
    expect(store.hasActiveConflict(["folder:nike:posts"])).toBe(false);
    expect(store.hasActiveConflict(["folder:nike:stories"])).toBe(true);
    store.releaseConflictKeys(unrelated);
    expect(store.hasActiveConflict(["folder:nike:stories"])).toBe(false);
  });

  it("transfers a pending reservation into active job metadata without a gap", () => {
    const store = useJobsStore();
    const token = Symbol("accepted enqueue");
    expect(store.reserveConflictKeys(token, ["folder:nike:posts"])).toBe(true);

    store.transferConflictReservation(
      token,
      "accepted",
      "Explore posts",
      ["folder:nike:posts"],
    );

    expect(store.jobs.get("accepted")?.conflictKeys).toEqual(["folder:nike:posts"]);
    expect(store.hasActiveConflict(["folder:nike:posts"])).toBe(true);
    store.jobs.get("accepted")!.state = "failed";
    expect(store.hasActiveConflict(["folder:nike:posts"])).toBe(false);
  });

  it("releases a reservation when transfer finds an early terminal backend event", async () => {
    const store = useJobsStore();
    await store.init();
    const token = Symbol("early terminal enqueue");
    expect(store.reserveConflictKeys(token, ["profile:nike", "folder:nike:posts"])).toBe(true);
    ipc.listener?.({
      job_id: "early-reserved",
      state: "done",
      label: "Backend label",
      count: 1,
    });

    store.transferConflictReservation(
      token,
      "early-reserved",
      "Placeholder label",
      ["profile:nike", "folder:nike:posts"],
    );

    expect(store.jobs.get("early-reserved")).toMatchObject({
      state: "done",
      label: "Placeholder label",
      conflictKeys: ["profile:nike", "folder:nike:posts"],
    });
    expect(store.hasActiveConflict(["profile:nike", "folder:nike:posts"])).toBe(false);
  });

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
      label: "Placeholder label",
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
