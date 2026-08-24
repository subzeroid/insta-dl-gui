/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

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

const mountedWrappers: Array<{ unmount: () => void }> = [];

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

async function renderSettings() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const app = useAppStore();
  await app.init();
  const host = document.createElement("div");
  document.body.append(host);
  const wrapper = mount(SettingsView, {
    attachTo: host,
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
  mountedWrappers.push(wrapper);
  return { app, wrapper };
}

describe("Settings Library registration warning", () => {
  afterEach(() => {
    for (const wrapper of mountedWrappers.splice(0)) wrapper.unmount();
    document.body.replaceChildren();
  });

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
    const { wrapper } = await renderSettings();

    await wrapper.get("button").trigger("click");
    await flushPromises();

    expect(ipc.saveSettings).toHaveBeenCalledWith({ dest_dir: "/archive/new" });
    expect(wrapper.text()).toContain("Settings were saved");
    const link = wrapper.get('a[href="/library"]');
    expect(link.text()).toContain("Library");
  });

  it("keeps the previous destination and shows a sanitized error when saving fails", async () => {
    dialog.open.mockResolvedValue("/archive/new");
    ipc.saveSettings.mockRejectedValue(
      new Error("failed to write /Users/private/.config/insta-dl-gui/config.json"),
    );
    const { wrapper } = await renderSettings();

    await wrapper.get("button").trigger("click");
    await flushPromises();

    expect(ipc.saveSettings).toHaveBeenCalledOnce();
    expect(ipc.saveSettings).toHaveBeenCalledWith({ dest_dir: "/archive/new" });
    expect(wrapper.get("input[readonly]").attributes("value")).toBe("/archive/old");
    const alert = wrapper.get('[role="alert"]');
    expect(alert.text()).toContain("Settings could not be saved");
    expect(alert.text()).not.toContain("/Users/private");
  });

  it("restores the sidecar control after a rejected save without retrying", async () => {
    ipc.saveSettings.mockRejectedValue(new Error("database connection failed"));
    const { wrapper } = await renderSettings();
    const checkbox = wrapper.get<HTMLInputElement>('input[type="checkbox"]');

    await checkbox.setValue(false);
    await flushPromises();

    expect(ipc.saveSettings).toHaveBeenCalledOnce();
    expect(ipc.saveSettings).toHaveBeenCalledWith({ sidecar: false });
    expect(checkbox.element.checked).toBe(true);
    expect(wrapper.get('[role="alert"]').text()).toContain("Settings could not be saved");
  });

  it("keeps focus while serializing rapid sidecar changes without swallowing the next toggle", async () => {
    const firstSave = deferred<never>();
    ipc.saveSettings
      .mockReturnValueOnce(firstSave.promise)
      .mockImplementation(async (opts: { sidecar?: boolean }) => ({
        has_token: false,
        token_hint: null,
        dest_dir: "/archive/old",
        sidecar: opts.sidecar ?? true,
      }));
    const { wrapper } = await renderSettings();
    const checkbox = wrapper.get<HTMLInputElement>('input[type="checkbox"]');
    checkbox.element.focus();

    await checkbox.setValue(false);
    expect(checkbox.element.disabled).toBe(false);
    expect(checkbox.attributes("aria-disabled")).toBe("true");
    expect(document.activeElement).toBe(checkbox.element);
    await checkbox.setValue(true);
    expect(ipc.saveSettings).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(checkbox.element);

    firstSave.reject(new Error("first save failed"));
    await flushPromises();
    expect(checkbox.element.checked).toBe(true);
    expect(checkbox.attributes("aria-disabled")).toBe("false");
    expect(document.activeElement).toBe(checkbox.element);

    await checkbox.setValue(false);
    await flushPromises();

    expect(ipc.saveSettings).toHaveBeenCalledTimes(2);
    expect(ipc.saveSettings.mock.calls).toEqual([
      [{ sidecar: false }],
      [{ sidecar: false }],
    ]);
    expect(checkbox.element.checked).toBe(false);
    expect(document.activeElement).toBe(checkbox.element);
  });
});
