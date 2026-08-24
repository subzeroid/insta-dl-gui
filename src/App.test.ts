/** @vitest-environment happy-dom */

import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  app: {
    ready: true,
    hasToken: true,
    balance: null,
    init: vi.fn(),
    refreshBalance: vi.fn(),
  },
  jobs: { init: vi.fn() },
  push: vi.fn(),
}));

vi.mock("./stores/app", () => ({ useAppStore: () => state.app }));
vi.mock("./stores/jobs", () => ({ useJobsStore: () => state.jobs }));
vi.mock("./lib/ipc", () => ({ formatBalance: vi.fn(() => "10 req") }));
vi.mock("vue-router", () => ({
  useRoute: () => ({ path: "/library" }),
  useRouter: () => ({ push: state.push }),
}));

import App from "./App.vue";

beforeEach(() => {
  vi.clearAllMocks();
  state.app.init.mockResolvedValue(undefined);
  state.jobs.init.mockResolvedValue(undefined);
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 375 });
});

describe("application chrome", () => {
  it("contains narrow navigation overflow without widening the page", () => {
    const wrapper = mount(App, {
      global: {
        stubs: {
          RouterLink: {
            props: ["to"],
            template: '<a :href="to"><slot /></a>',
          },
          RouterView: true,
        },
      },
    });

    const header = wrapper.get("header");
    const navigation = wrapper.get("nav");
    expect(header.classes()).toEqual(
      expect.arrayContaining(["min-w-0", "flex-col", "sm:flex-row"]),
    );
    expect(navigation.attributes("aria-label")).toBe("Primary");
    expect(navigation.classes()).toEqual(
      expect.arrayContaining(["w-full", "min-w-0", "max-w-full", "overflow-x-auto"]),
    );
    expect(navigation.findAll("a").map((link) => link.text())).toEqual([
      "Download",
      "Explore",
      "Library",
      "Queue",
      "Settings",
    ]);
    expect(navigation.findAll("a").every((link) => link.classes().includes("shrink-0"))).toBe(
      true,
    );
    expect(navigation.get("button").classes()).toContain("shrink-0");
  });
});
