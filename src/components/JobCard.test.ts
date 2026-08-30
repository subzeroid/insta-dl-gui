/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/ipc", () => ({
  cancelJob: vi.fn(),
  formatBytes: (bytes: number) => `${bytes} B`,
  onJobProgress: vi.fn(),
}));

import JobCard from "./JobCard.vue";
import type { JobView } from "../stores/jobs";

function doneJob(overrides: Partial<JobView> = {}): JobView {
  return {
    id: "job-1",
    label: "Archive",
    state: "done",
    currentFile: 0,
    totalFiles: 0,
    bytesDone: 0,
    fileName: "",
    resultCount: 3,
    resultDir: "/archive",
    catalogWarnings: 0,
    resourceFailures: 0,
    ...overrides,
  };
}

function render(job: JobView) {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(JobCard, {
    props: { job },
    global: { plugins: [pinia] },
  });
}

beforeEach(() => {
  setActivePinia(createPinia());
});

describe("JobCard done warnings", () => {
  it("keeps a legacy successful completion green", () => {
    const wrapper = render(doneJob());

    expect(wrapper.text()).toContain("✓ 3 files");
    expect(wrapper.find(".text-ok").exists()).toBe(true);
    expect(wrapper.text()).not.toContain("resource failure");
    expect(wrapper.text()).not.toContain("indexing failed");
  });

  it("renders resource failures as a non-green partial completion", () => {
    const wrapper = render(doneJob({ resourceFailures: 2 }));

    expect(wrapper.text()).toContain("saved 3 files / 2 resource failures");
    expect(wrapper.find(".text-warn").exists()).toBe(true);
    expect(wrapper.find(".text-ok").exists()).toBe(false);
  });

  it("explains that catalog warnings need a Library rescan", () => {
    const wrapper = render(doneJob({ resultCount: 1, catalogWarnings: 2 }));

    expect(wrapper.text()).toContain("saved 1 file with warnings");
    expect(wrapper.text()).toContain(
      "Files are saved, but Library indexing failed for 2 items. Rescan the Library.",
    );
    expect(wrapper.find(".text-warn").exists()).toBe(true);
  });
});

describe("JobCard completed download inspection", () => {
  const outputs = [
    { file_id: 7, basename: "saved.jpg", kind: "photo" as const, byte_size: 12, ordinal: 0 },
  ];

  it.each(["click", "Enter", " "])("opens output details with %s activation", async (activation) => {
    const wrapper = render(doneJob({ outputs }));
    const card = wrapper.get("[data-job-id='job-1']");

    if (activation === "click") await card.trigger("click");
    else await card.trigger("keydown", { key: activation });

    expect(wrapper.emitted("inspect")).toHaveLength(1);
    expect(wrapper.emitted("inspect")?.[0]?.[0]).toMatchObject({ id: "job-1", outputs });
    expect(wrapper.emitted("inspect")?.[0]?.[1]).toBe(card.element);
  });

  it("exposes button semantics and a visible focus affordance only for actionable jobs", () => {
    const actionable = render(doneJob({ outputs }));
    const card = actionable.get("[data-job-id='job-1']");
    expect(card.attributes("role")).toBe("button");
    expect(card.attributes("tabindex")).toBe("0");
    expect(card.attributes("aria-label")).toContain("Inspect downloaded files");
    expect(card.classes().join(" ")).toContain("focus-visible:ring-2");

    for (const state of ["done", "fetching", "downloading", "failed", "cancelled"] as const) {
      const job = doneJob({ state, outputs: state === "done" ? undefined : outputs });
      const inert = render(job).get("[data-job-id='job-1']");
      expect(inert.attributes("role")).toBeUndefined();
      expect(inert.attributes("tabindex")).toBeUndefined();
    }
  });

  it("isolates Cancel from card activation", async () => {
    const wrapper = render(doneJob({ state: "downloading", outputs }));
    const cancel = wrapper.get("button");
    await cancel.trigger("click");

    expect(wrapper.emitted("inspect")).toBeUndefined();
  });
});
