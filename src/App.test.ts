/** @vitest-environment happy-dom */

import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  route: { path: "/library" },
  app: {
    ready: true,
    hasToken: true,
    balance: null,
    init: vi.fn(),
    refreshBalance: vi.fn(),
  },
  jobs: { init: vi.fn(), jobs: new Map() },
  push: vi.fn(),
}));

vi.mock("./stores/app", () => ({ useAppStore: () => state.app }));
vi.mock("./stores/jobs", () => ({ useJobsStore: () => state.jobs }));
vi.mock("./lib/ipc", () => ({ formatBalance: vi.fn(() => "10 req") }));
vi.mock("vue-router", () => ({
  useRoute: () => state.route,
  useRouter: () => ({ push: state.push }),
}));

import App from "./App.vue";

beforeEach(() => {
  vi.clearAllMocks();
  state.app.init.mockResolvedValue(undefined);
  state.jobs.init.mockResolvedValue(undefined);
  state.jobs.jobs.clear();
  state.route.path = "/library";
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 375 });
});

describe("application chrome", () => {
  it("mounts global download activity outside onboarding", () => {
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

    expect(wrapper.get("[data-testid='download-activity']").attributes("href")).toBe("/queue");
  });

  it("does not mount download activity during onboarding", () => {
    state.route.path = "/onboarding";
    const wrapper = mount(App, {
      global: { stubs: { RouterLink: true, RouterView: true } },
    });

    expect(wrapper.find("[data-testid='download-activity']").exists()).toBe(false);
  });

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
