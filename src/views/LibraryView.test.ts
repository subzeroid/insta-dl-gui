/** @vitest-environment happy-dom */

import { createPinia, setActivePinia, type Pinia } from "pinia";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  LibraryCard,
  LibraryItemDetail,
  LibraryPage,
  LibraryRoot,
  LibraryScanProgress,
} from "../lib/ipc";

const ipc = vi.hoisted(() => ({
  ensureConfiguredLibraryRoot: vi.fn(),
  listLibraryRoots: vi.fn(),
  startLibraryScan: vi.fn(),
  cancelLibraryScan: vi.fn(),
  queryLibrary: vi.fn(),
  getLibraryItem: vi.fn(),
  openLibraryFile: vi.fn(),
  revealLibraryFile: vi.fn(),
  onLibraryScanProgress: vi.fn(),
  libraryMediaUrl: vi.fn(),
  listener: undefined as ((progress: LibraryScanProgress) => void) | undefined,
}));

vi.mock("../lib/ipc", () => ({
  ensureConfiguredLibraryRoot: ipc.ensureConfiguredLibraryRoot,
  listLibraryRoots: ipc.listLibraryRoots,
  startLibraryScan: ipc.startLibraryScan,
  cancelLibraryScan: ipc.cancelLibraryScan,
  queryLibrary: ipc.queryLibrary,
  getLibraryItem: ipc.getLibraryItem,
  openLibraryFile: ipc.openLibraryFile,
  revealLibraryFile: ipc.revealLibraryFile,
  onLibraryScanProgress: ipc.onLibraryScanProgress,
  libraryMediaUrl: ipc.libraryMediaUrl,
}));

import LibraryView from "./LibraryView.vue";
import { useLibraryStore } from "../stores/library";

const unscannedRoot: LibraryRoot = {
  id: 7,
  path: "/mock/archive",
  label: "Archive",
  created_at: 1_700_000_000,
  last_scan_started_at: null,
  last_scan_completed_at: null,
};

const scannedRoot: LibraryRoot = {
  ...unscannedRoot,
  last_scan_started_at: 1_700_000_100,
  last_scan_completed_at: 1_700_000_120,
};

function card(id: number, overrides: Partial<LibraryCard> = {}): LibraryCard {
  return {
    id,
    kind: "post",
    shortcode: `CODE${id}`,
    owner_username: `owner${id}`,
    taken_at: 1_700_000_000 + id,
    caption: `Caption ${id}`,
    imported_at: 1_700_000_100 + id,
    updated_at: 1_700_000_200 + id,
    preview_file_id: 1_000 + id,
    preview_file_kind: "photo",
    resource_count: 1,
    availability: "available",
    ...overrides,
  };
}

function page(items: LibraryCard[], next_cursor: string | null = null): LibraryPage {
  return { items, next_cursor };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function detail(id = 1): LibraryItemDetail {
  return {
    id,
    kind: "post",
    remote_pk: `remote-${id}`,
    shortcode: `CODE${id}`,
    owner_pk: `owner-pk-${id}`,
    owner_username: "library_owner",
    taken_at: 1_700_000_000,
    caption: "A caption kept in the local archive",
    like_count: 42,
    comment_count: 3,
    imported_at: 1_700_000_100,
    updated_at: 1_700_000_200,
    files: [
      {
        id: 501,
        ordinal: 0,
        kind: "photo",
        byte_size: 1_500_000,
        mtime: 1_700_000_000,
        exists_on_disk: true,
        last_seen_at: 1_700_000_200,
      },
      {
        id: 502,
        ordinal: 1,
        kind: "video",
        byte_size: 2_500_000,
        mtime: 1_700_000_001,
        exists_on_disk: false,
        last_seen_at: 1_700_000_200,
      },
    ],
    source_ids: [],
  };
}

class TestIntersectionObserver implements IntersectionObserver {
  static instances: TestIntersectionObserver[] = [];

  readonly root: Element | Document | null;
  readonly rootMargin: string;
  readonly thresholds: readonly number[];
  private readonly callback: IntersectionObserverCallback;
  private readonly targets = new Set<Element>();

  constructor(callback: IntersectionObserverCallback, options: IntersectionObserverInit = {}) {
    this.callback = callback;
    this.root = options.root ?? null;
    this.rootMargin = options.rootMargin ?? "0px";
    this.thresholds = Array.isArray(options.threshold)
      ? options.threshold
      : [options.threshold ?? 0];
    TestIntersectionObserver.instances.push(this);
  }

  disconnect() {
    this.targets.clear();
  }

  observe(target: Element) {
    this.targets.add(target);
  }

  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }

  unobserve(target: Element) {
    this.targets.delete(target);
  }

  static trigger(target: Element, isIntersecting = true) {
    const observer = TestIntersectionObserver.instances.find((candidate) =>
      candidate.targets.has(target),
    );
    if (!observer) throw new Error("No IntersectionObserver watches the target");
    observer.callback(
      [
        {
          boundingClientRect: target.getBoundingClientRect(),
          intersectionRatio: isIntersecting ? 1 : 0,
          intersectionRect: target.getBoundingClientRect(),
          isIntersecting,
          rootBounds: null,
          target,
          time: 0,
        },
      ],
      observer,
    );
  }
}

class TestResizeObserver implements ResizeObserver {
  disconnect() {}
  observe() {}
  unobserve() {}
}

const wrappers: VueWrapper[] = [];

async function render(options: {
  root?: LibraryRoot;
  listError?: Error;
  pinia?: Pinia;
  viewport?: { width: number; height: number; scrollTop: number };
} = {}) {
  const root = options.root ?? unscannedRoot;
  ipc.ensureConfiguredLibraryRoot.mockResolvedValue(root);
  if (options.listError) ipc.listLibraryRoots.mockRejectedValue(options.listError);
  else ipc.listLibraryRoots.mockResolvedValue([root]);
  const pinia = options.pinia ?? createPinia();
  setActivePinia(pinia);
  const host = document.createElement("div");
  document.body.append(host);
  const wrapper = mount(LibraryView, {
    attachTo: host,
    props: {
      testViewport: options.viewport ?? { width: 960, height: 640, scrollTop: 0 },
    },
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
  wrappers.push(wrapper);
  await flushPromises();
  return wrapper;
}

beforeEach(() => {
  for (const mock of [
    ipc.ensureConfiguredLibraryRoot,
    ipc.listLibraryRoots,
    ipc.startLibraryScan,
    ipc.cancelLibraryScan,
    ipc.queryLibrary,
    ipc.getLibraryItem,
    ipc.openLibraryFile,
    ipc.revealLibraryFile,
    ipc.onLibraryScanProgress,
    ipc.libraryMediaUrl,
  ]) {
    mock.mockReset();
  }
  vi.stubGlobal("IntersectionObserver", TestIntersectionObserver);
  vi.stubGlobal("ResizeObserver", TestResizeObserver);
  TestIntersectionObserver.instances = [];
  ipc.listener = undefined;
  ipc.startLibraryScan.mockResolvedValue("scan-7");
  ipc.cancelLibraryScan.mockResolvedValue(true);
  ipc.queryLibrary.mockResolvedValue(page([]));
  ipc.getLibraryItem.mockResolvedValue(detail());
  ipc.openLibraryFile.mockResolvedValue(undefined);
  ipc.revealLibraryFile.mockResolvedValue(undefined);
  ipc.libraryMediaUrl.mockImplementation(
    (fileId: number) => `library://localhost/media/${fileId}`,
  );
  ipc.onLibraryScanProgress.mockImplementation(
    async (listener: (progress: LibraryScanProgress) => void) => {
      ipc.listener = listener;
      return () => {};
    },
  );
});

afterEach(() => {
  for (const wrapper of wrappers.splice(0)) {
    const host = wrapper.element.parentElement;
    wrapper.unmount();
    host?.remove();
  }
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("Library first-scan flow", () => {
  it("shows only the root initialization error when no active root can be loaded", async () => {
    const wrapper = await render({ listError: new Error("Archive root is unavailable") });

    expect(wrapper.get("[role='alert']").text()).toContain("Archive root is unavailable");
    expect(wrapper.find("[aria-label='Library filters']").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("No media in your local archive yet");
    expect(wrapper.text()).not.toContain("Scan library");
  });

  it("does not treat a missing active root as a completed archive", async () => {
    const wrapper = await render({ root: scannedRoot });
    const store = useLibraryStore();
    store.activeRoot = null;
    await flushPromises();

    expect(wrapper.find("[aria-label='Library filters']").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("No media in your local archive yet");
  });

  it("does not refresh or perform late view work when initialization finishes after unmount", async () => {
    const registration = deferred<() => void>();
    ipc.onLibraryScanProgress.mockImplementationOnce(() => registration.promise);
    const wrapper = await render({ root: scannedRoot });

    const wrapperIndex = wrappers.indexOf(wrapper);
    if (wrapperIndex >= 0) wrappers.splice(wrapperIndex, 1);
    const host = wrapper.element.parentElement;
    wrapper.unmount();
    host?.remove();
    registration.resolve(() => {});
    await flushPromises();

    expect(ipc.queryLibrary).not.toHaveBeenCalled();
  });

  it("offers a scan and a Download link on the first visit to an unscanned root", async () => {
    const wrapper = await render();

    expect(wrapper.text()).toContain("Scan library");
    expect(wrapper.get('a[href="/download"]').text()).toContain("Download");

    await wrapper.get("button[data-action='scan']").trigger("click");
    await flushPromises();
    expect(ipc.onLibraryScanProgress.mock.invocationCallOrder[0]).toBeLessThan(
      ipc.startLibraryScan.mock.invocationCallOrder[0],
    );
    expect(ipc.startLibraryScan).toHaveBeenCalledWith(7);
  });

  it("shows discovered, processed, and warning progress with cancellation", async () => {
    const wrapper = await render();
    await wrapper.get("button[data-action='scan']").trigger("click");
    await flushPromises();

    ipc.listener?.({
      state: "scanning",
      scan_id: "scan-7",
      root_id: 7,
      discovered: 20,
      processed: 12,
      warnings: 2,
    });
    await flushPromises();

    expect(wrapper.text()).toContain("12 processed");
    expect(wrapper.text()).toContain("20 discovered");
    expect(wrapper.text()).toContain("2 warnings");
    await wrapper.get("button[data-action='cancel-scan']").trigger("click");
    expect(ipc.cancelLibraryScan).toHaveBeenCalledWith("scan-7");
    expect(wrapper.text()).not.toContain("Bring your local archive into view");
  });

  it("explains an empty archive after a completed scan and links to Download", async () => {
    const wrapper = await render();
    await wrapper.get("button[data-action='scan']").trigger("click");
    await flushPromises();
    ipc.listener?.({
      state: "done",
      scan_id: "scan-7",
      root_id: 7,
      summary: { imported: 0, updated: 0, missing: 0, warnings: 0 },
    });
    await flushPromises();

    expect(wrapper.text()).toContain("No media in your local archive yet");
    expect(wrapper.text()).toContain("Downloaded posts, reels, stories, and avatars");
    expect(wrapper.get('a[href="/download"]').text()).toContain("Download media");
  });
});

describe("Library browsing", () => {
  it("mounts only visible rows plus two-row overscan for 1,000 cards", async () => {
    ipc.queryLibrary.mockResolvedValue(
      page(Array.from({ length: 1_000 }, (_, index) => card(index + 1))),
    );
    const wrapper = await render({
      root: scannedRoot,
      viewport: { width: 960, height: 640, scrollTop: 0 },
    });

    const rendered = wrapper.findAll("[data-library-card-id]");
    expect(rendered.map((item) => Number(item.attributes("data-library-card-id")))).toEqual(
      Array.from({ length: 20 }, (_, index) => index + 1),
    );

    await wrapper.setProps({
      testViewport: { width: 960, height: 640, scrollTop: 948 },
    });
    await flushPromises();
    expect(
      wrapper.findAll("[data-library-card-id]").map((item) =>
        Number(item.attributes("data-library-card-id")),
      ),
    ).toEqual(Array.from({ length: 20 }, (_, index) => index + 9));
    const grid = wrapper.get("[data-testid='library-virtual-grid']").element;
    expect((grid.children[0] as HTMLElement).style.height).toBe("632px");

    await wrapper.setProps({
      testViewport: { width: 480, height: 640, scrollTop: 0 },
    });
    await flushPromises();
    expect(
      wrapper.findAll("[data-library-card-id]").map((item) =>
        Number(item.attributes("data-library-card-id")),
      ),
    ).toEqual(Array.from({ length: 10 }, (_, index) => index + 1));
  });

  it("debounces search by 250ms and normalizes every query control", async () => {
    vi.useFakeTimers();
    const wrapper = await render({ root: scannedRoot });
    ipc.queryLibrary.mockClear();

    await wrapper.get("input[aria-label='Search library']").setValue("  sunrise  ");
    vi.advanceTimersByTime(249);
    await Promise.resolve();
    expect(ipc.queryLibrary).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1);
    await flushPromises();
    expect(ipc.queryLibrary).toHaveBeenLastCalledWith(
      expect.objectContaining({ search: "sunrise" }),
    );

    for (const [label, kind] of [
      ["Posts", "post"],
      ["Reels", "reel"],
      ["Stories", "story"],
      ["Avatars", "avatar"],
    ] as const) {
      await wrapper.get(`button[aria-label='Filter ${label}']`).trigger("click");
      await flushPromises();
      expect(ipc.queryLibrary).toHaveBeenLastCalledWith(
        expect.objectContaining({ kinds: expect.arrayContaining([kind]) }),
      );
    }

    await wrapper.get("select[aria-label='File availability']").setValue("missing");
    await flushPromises();
    expect(ipc.queryLibrary).toHaveBeenLastCalledWith(
      expect.objectContaining({ availability: "missing" }),
    );

    await wrapper.get("select[aria-label='Sort library']").setValue("imported_at_desc");
    await flushPromises();
    expect(ipc.queryLibrary).toHaveBeenLastCalledWith(
      expect.objectContaining({ sort: "imported_at_desc" }),
    );

    await wrapper.get("input[aria-label='Taken after']").setValue("2026-08-01");
    await wrapper.get("input[aria-label='Taken before']").setValue("2026-08-24");
    await flushPromises();
    expect(ipc.queryLibrary).toHaveBeenLastCalledWith(
      expect.objectContaining({
        taken_after: new Date(2026, 7, 1, 0, 0, 0, 0).getTime() / 1_000,
        taken_before: Math.floor(
          new Date(2026, 7, 24, 23, 59, 59, 999).getTime() / 1_000,
        ),
      }),
    );

    const source = wrapper.get("select[aria-label='Source']");
    expect(source.attributes("disabled")).toBeDefined();
    expect((source.element as HTMLSelectElement).value).toBe("all");
    expect(source.text()).toContain("All sources");
  });

  it("rejects invalid and reversed local date ranges without querying", async () => {
    const wrapper = await render({ root: scannedRoot });
    ipc.queryLibrary.mockClear();
    const after = wrapper.get("input[aria-label='Taken after']");
    const before = wrapper.get("input[aria-label='Taken before']");

    await before.setValue("2026-08-01");
    await flushPromises();
    ipc.queryLibrary.mockClear();
    await after.setValue("2026-08-24");
    await flushPromises();
    expect(wrapper.get("[data-testid='library-date-error']").text()).toContain(
      "start date must be before the end date",
    );
    expect(ipc.queryLibrary).not.toHaveBeenCalled();

    Object.defineProperty(after.element, "value", {
      configurable: true,
      value: "2026-02-30",
    });
    await after.trigger("input");
    await after.trigger("change");
    await flushPromises();
    expect(wrapper.get("[data-testid='library-date-error']").text()).toContain(
      "valid calendar date",
    );
    expect(ipc.queryLibrary).not.toHaveBeenCalled();
  });

  it("restores persisted local date drafts when the view remounts", async () => {
    const pinia = createPinia();
    setActivePinia(pinia);
    const store = useLibraryStore();
    const after = new Date(2026, 7, 1, 0, 0, 0, 0).getTime() / 1_000;
    const before = Math.floor(
      new Date(2026, 7, 24, 23, 59, 59, 999).getTime() / 1_000,
    );
    store.setDateRange(after, before);

    const wrapper = await render({ root: scannedRoot, pinia });

    expect((wrapper.get("input[aria-label='Taken after']").element as HTMLInputElement).value).toBe(
      "2026-08-01",
    );
    expect((wrapper.get("input[aria-label='Taken before']").element as HTMLInputElement).value).toBe(
      "2026-08-24",
    );
  });

  it("shows filtered no-results separately and clears all filters with one refresh", async () => {
    ipc.queryLibrary.mockResolvedValue(page([]));
    const wrapper = await render({ root: scannedRoot });
    ipc.queryLibrary.mockClear();

    await wrapper.get("button[aria-label='Filter Reels']").trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("No matches");
    expect(wrapper.text()).not.toContain("No media in your local archive yet");
    ipc.queryLibrary.mockClear();

    await wrapper.get("button[data-action='clear-library-filters']").trigger("click");
    await flushPromises();

    expect(ipc.queryLibrary).toHaveBeenCalledTimes(1);
    expect(ipc.queryLibrary).toHaveBeenLastCalledWith(
      expect.objectContaining({
        search: null,
        kinds: [],
        source_id: null,
        availability: null,
        taken_after: null,
        taken_before: null,
        sort: "taken_at_desc",
      }),
    );
  });

  it("loads the next page from the bottom observer without replacing prior cards", async () => {
    ipc.queryLibrary
      .mockResolvedValueOnce(page([card(1), card(2), card(3)], "next"))
      .mockResolvedValueOnce(page([card(4), card(5)], null));
    const wrapper = await render({ root: scannedRoot });
    const sentinel = wrapper.get("[data-testid='library-load-more-sentinel']").element;

    TestIntersectionObserver.trigger(sentinel);
    await flushPromises();

    expect(ipc.queryLibrary).toHaveBeenLastCalledWith(
      expect.objectContaining({ cursor: "next" }),
    );
    expect(
      wrapper.findAll("[data-library-card-id]").map((item) => item.attributes("data-library-card-id")),
    ).toEqual(["1", "2", "3", "4", "5"]);
  });

  it("attaches local image and video previews only after cards enter the near viewport", async () => {
    ipc.queryLibrary.mockResolvedValue(
      page([
        card(1),
        card(2, { kind: "story", preview_file_id: 2_002, preview_file_kind: "video" }),
      ]),
    );
    const wrapper = await render({ root: scannedRoot });
    const photoCard = wrapper.get("[data-library-card-id='1']");
    const videoCard = wrapper.get("[data-library-card-id='2']");
    expect(photoCard.get("span.relative").classes()).toContain("h-[216px]");

    expect(photoCard.find("img").exists()).toBe(false);
    expect(videoCard.find("video").exists()).toBe(false);
    TestIntersectionObserver.trigger(photoCard.element);
    TestIntersectionObserver.trigger(videoCard.element);
    await flushPromises();

    expect(photoCard.get("img").attributes("src")).toBe("library://localhost/media/1001");
    expect(videoCard.get("video").attributes("src")).toBe(
      "library://localhost/media/2002",
    );
    expect(videoCard.get("video").attributes("preload")).toBe("metadata");
  });

  it("falls back to the local video placeholder when a near-viewport preview fails", async () => {
    ipc.queryLibrary.mockResolvedValue(
      page([
        card(2, { kind: "story", preview_file_id: 2_002, preview_file_kind: "video" }),
      ]),
    );
    const wrapper = await render({ root: scannedRoot });
    const videoCard = wrapper.get("[data-library-card-id='2']");
    TestIntersectionObserver.trigger(videoCard.element);
    await flushPromises();

    await videoCard.get("video").trigger("error");

    expect(videoCard.find("video").exists()).toBe(false);
    expect(videoCard.find("img").exists()).toBe(false);
    expect(videoCard.text()).toContain("Video");
    expect(videoCard.html()).not.toContain("http://");
    expect(videoCard.html()).not.toContain("https://");
  });

  it("reacts when a same-key card gains or replaces its preview", async () => {
    ipc.queryLibrary.mockResolvedValue(page([card(1, { preview_file_id: null })]));
    const wrapper = await render({ root: scannedRoot });
    const store = useLibraryStore();
    const cardWrapper = () => wrapper.get("[data-library-card-id='1']");

    store.cards = [
      {
        ...card(1, { preview_file_id: 1_001 }),
        previewUrl: "library://localhost/media/1001",
        previewFileKind: "photo",
      },
    ];
    await flushPromises();
    TestIntersectionObserver.trigger(cardWrapper().element);
    await flushPromises();
    expect(cardWrapper().get("img").attributes("src")).toContain("/1001");

    await cardWrapper().get("img").trigger("error");
    expect(cardWrapper().find("img").exists()).toBe(false);
    store.cards = [
      {
        ...card(1, { preview_file_id: 2_001 }),
        previewUrl: "library://localhost/media/2001",
        previewFileKind: "photo",
      },
    ];
    await flushPromises();
    expect(cardWrapper().get("img").attributes("src")).toContain("/2001");
  });

  it("shows all local detail fields and disables actions for missing file rows", async () => {
    ipc.queryLibrary.mockResolvedValue(
      page([
        card(1, { resource_count: 3 }),
        card(2, { availability: "missing", preview_file_id: null }),
      ]),
    );
    ipc.getLibraryItem.mockResolvedValue(detail(1));
    const wrapper = await render({ root: scannedRoot });

    expect(wrapper.get("[data-library-card-id='1']").text()).toContain("3 files");
    expect(wrapper.get("[data-library-card-id='2']").text()).toContain("Missing");
    await wrapper.get("[data-library-card-id='1']").trigger("click");
    await flushPromises();

    const panel = wrapper.get("[role='dialog']");
    const expectedDate = new Intl.DateTimeFormat(undefined, {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(1_700_000_000 * 1_000));
    expect(panel.text()).toContain("@library_owner");
    expect(panel.text()).toContain("A caption kept in the local archive");
    expect(panel.text()).toContain(expectedDate);
    expect(panel.text()).toContain("Sources");
    expect(panel.text()).toContain("Available in a future update");
    expect(panel.findAll("[data-library-file-id]")).toHaveLength(2);
    const available = panel.get("[data-library-file-id='501']");
    const missing = panel.get("[data-library-file-id='502']");
    expect(available.get("button[data-action='open-file']").attributes("disabled")).toBeUndefined();
    expect(missing.text()).toContain("Missing");
    expect(missing.get("button[data-action='open-file']").attributes("disabled")).toBeDefined();
    expect(missing.get("button[data-action='reveal-file']").attributes("disabled")).toBeDefined();

    await available.get("button[data-action='open-file']").trigger("click");
    await flushPromises();
    await available.get("button[data-action='reveal-file']").trigger("click");
    await flushPromises();
    expect(ipc.openLibraryFile).toHaveBeenCalledWith(501);
    expect(ipc.revealLibraryFile).toHaveBeenCalledWith(501);
    expect(typeof ipc.openLibraryFile.mock.calls[0][0]).toBe("number");
  });

  it("supports click and keyboard activation and returns focus when detail closes", async () => {
    ipc.queryLibrary.mockResolvedValue(page([card(1)]));
    ipc.getLibraryItem.mockResolvedValue(detail(1));
    const wrapper = await render({ root: scannedRoot });
    const origin = wrapper.get("[data-library-card-id='1']");

    (origin.element as HTMLElement).focus();
    await origin.trigger("keydown", { key: "Enter" });
    await flushPromises();
    expect(ipc.getLibraryItem).toHaveBeenCalledWith(1);
    expect(wrapper.find("[role='dialog']").exists()).toBe(true);

    await wrapper.get("button[aria-label='Close library detail']").trigger("click");
    await flushPromises();
    expect(wrapper.find("[role='dialog']").exists()).toBe(false);
    expect(document.activeElement).toBe(origin.element);

    await origin.trigger("click");
    await flushPromises();
    expect(ipc.getLibraryItem).toHaveBeenCalledTimes(2);
  });

  it("traps focus, inerts the background, locks scrolling, and uses the grid fallback", async () => {
    document.body.style.overflow = "scroll";
    ipc.queryLibrary.mockResolvedValue(page([card(1)]));
    ipc.getLibraryItem.mockResolvedValue(detail(1));
    const wrapper = await render({ root: scannedRoot });
    const origin = wrapper.get("[data-library-card-id='1']");
    await origin.trigger("click");
    await flushPromises();

    const background = wrapper.get("[data-testid='library-background']");
    const dialog = wrapper.get("[role='dialog']");
    const close = dialog.get("button[aria-label='Close library detail']");
    const last = dialog.findAll("button:not([disabled])").at(-1)!;
    expect(background.attributes("inert")).toBeDefined();
    expect(document.body.style.overflow).toBe("hidden");

    (close.element as HTMLElement).focus();
    await close.trigger("keydown", { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(last.element);
    await last.trigger("keydown", { key: "Tab" });
    expect(document.activeElement).toBe(close.element);

    origin.element.remove();
    await close.trigger("click");
    await flushPromises();
    expect(background.attributes("inert")).toBeUndefined();
    expect(document.body.style.overflow).toBe("scroll");
    expect(document.activeElement).toBe(
      wrapper.get("[data-testid='library-virtual-grid']").element,
    );
  });

  it("shows detail loading and failure feedback and restores the card focus", async () => {
    const pending = deferred<LibraryItemDetail>();
    ipc.queryLibrary.mockResolvedValue(page([card(1)]));
    ipc.getLibraryItem.mockReturnValueOnce(pending.promise);
    const wrapper = await render({ root: scannedRoot });
    const origin = wrapper.get("[data-library-card-id='1']");
    (origin.element as HTMLElement).focus();

    await origin.trigger("click");
    expect(wrapper.get("[data-testid='library-detail-loading']").text()).toContain(
      "Loading media details",
    );
    pending.reject(new Error("Detail record is unavailable"));
    await flushPromises();

    expect(wrapper.get("[data-testid='library-detail-error']").text()).toContain(
      "Detail record is unavailable",
    );
    expect(document.activeElement).toBe(origin.element);
  });

  it("invalidates an in-flight detail request across unmount and remount", async () => {
    const pinia = createPinia();
    const pending = deferred<LibraryItemDetail>();
    ipc.queryLibrary.mockResolvedValue(page([card(1)]));
    ipc.getLibraryItem.mockReturnValueOnce(pending.promise);
    const first = await render({ root: scannedRoot, pinia });
    await first.get("[data-library-card-id='1']").trigger("click");

    const firstIndex = wrappers.indexOf(first);
    if (firstIndex >= 0) wrappers.splice(firstIndex, 1);
    const firstHost = first.element.parentElement;
    first.unmount();
    firstHost?.remove();
    pending.resolve(detail(1));
    await flushPromises();

    const second = await render({ root: scannedRoot, pinia });
    expect(second.find("[role='dialog']").exists()).toBe(false);
  });

  it("does not let terminal scan work refresh after unmount and supersede a remount", async () => {
    const pinia = createPinia();
    const terminalRoots = deferred<LibraryRoot[]>();
    ipc.queryLibrary.mockResolvedValue(page([]));
    const first = await render({ root: scannedRoot, pinia });
    ipc.listLibraryRoots.mockImplementationOnce(() => terminalRoots.promise);
    await first.get("button[data-action='scan']").trigger("click");
    await flushPromises();
    ipc.listener?.({
      state: "done",
      scan_id: "scan-7",
      root_id: 7,
      summary: { imported: 0, updated: 0, missing: 0, warnings: 0 },
    });
    await flushPromises();

    const firstIndex = wrappers.indexOf(first);
    if (firstIndex >= 0) wrappers.splice(firstIndex, 1);
    const firstHost = first.element.parentElement;
    first.unmount();
    firstHost?.remove();
    ipc.listLibraryRoots.mockResolvedValue([scannedRoot]);
    const second = await render({ root: scannedRoot, pinia });
    expect(second.exists()).toBe(true);
    const callsAfterRemount = ipc.queryLibrary.mock.calls.length;

    terminalRoots.resolve([scannedRoot]);
    await flushPromises();
    expect(ipc.queryLibrary).toHaveBeenCalledTimes(callsAfterRemount);
  });

  it("surfaces file action failures inline", async () => {
    ipc.queryLibrary.mockResolvedValue(page([card(1)]));
    ipc.getLibraryItem.mockResolvedValue(detail(1));
    ipc.openLibraryFile.mockRejectedValueOnce(new Error("File moved after scan"));
    const wrapper = await render({ root: scannedRoot });
    await wrapper.get("[data-library-card-id='1']").trigger("click");
    await flushPromises();

    await wrapper
      .get("[data-library-file-id='501'] button[data-action='open-file']")
      .trigger("click");
    await flushPromises();

    expect(wrapper.get("[role='alert']").text()).toContain("File moved after scan");
  });
});
