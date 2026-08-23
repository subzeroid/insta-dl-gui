/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

const opener = vi.hoisted(() => ({
  openUrl: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => opener);
vi.mock("vue-router", () => ({
  useRouter: () => ({ push: vi.fn() }),
}));
vi.mock("../lib/ipc", () => ({
  formatBalance: vi.fn(),
  getBalance: vi.fn(),
  validateToken: vi.fn(),
}));

import OnboardingView from "./OnboardingView.vue";

function render() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return mount(OnboardingView, {
    global: { plugins: [pinia] },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("OnboardingView", () => {
  it("opens the tracked HikerAPI signup URL", async () => {
    const wrapper = render();

    await wrapper.get("a").trigger("click");

    expect(opener.openUrl).toHaveBeenCalledWith("https://hikerapi.com/p/uk064a1b");
  });
});
