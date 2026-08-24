/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => ({
  configState: vi.fn(),
  getBalance: vi.fn(),
  saveSettings: vi.fn(),
}));
const dialog = vi.hoisted(() => ({ open: vi.fn() }));

vi.mock("../lib/ipc", () => ({
  configState: ipc.configState,
  getBalance: ipc.getBalance,
  saveSettings: ipc.saveSettings,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: dialog.open }));

import { useAppStore } from "../stores/app";
import SettingsView from "./SettingsView.vue";

describe("Settings Library registration warning", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    ipc.configState.mockResolvedValue({
      has_token: false,
      token_hint: null,
      dest_dir: "/archive/old",
      sidecar: true,
    });
    ipc.getBalance.mockResolvedValue(null);
  });

  it("shows a Library rescan link when the saved destination could not be registered", async () => {
    dialog.open.mockResolvedValue("/archive/new");
    ipc.saveSettings.mockResolvedValue({
      has_token: false,
      token_hint: null,
      dest_dir: "/archive/new",
      sidecar: true,
      catalog_warning: "Settings were saved, but the folder could not be added to the Library.",
    });
    const pinia = createPinia();
    setActivePinia(pinia);
    const app = useAppStore();
    await app.init();
    const wrapper = mount(SettingsView, {
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

    await wrapper.get("button").trigger("click");
    await flushPromises();

    expect(ipc.saveSettings).toHaveBeenCalledWith({ dest_dir: "/archive/new" });
    expect(wrapper.text()).toContain("Settings were saved");
    const link = wrapper.get('a[href="/library"]');
    expect(link.text()).toContain("Library");
  });
});
