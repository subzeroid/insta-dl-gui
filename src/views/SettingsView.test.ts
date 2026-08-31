/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const ipc = vi.hoisted(() => ({
  configState: vi.fn(),
  getBalance: vi.fn(),
  saveSettings: vi.fn(),
  setProxy: vi.fn(),
  validateToken: vi.fn(),
}));
const dialog = vi.hoisted(() => ({ open: vi.fn() }));

vi.mock("../lib/ipc", () => ({
  configState: ipc.configState,
  formatBalance: (balance: { requests: number }) => `${balance.requests} req`,
  getBalance: ipc.getBalance,
  saveSettings: ipc.saveSettings,
  setProxy: ipc.setProxy,
  validateToken: ipc.validateToken,
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
      has_token: true,
      token_hint: "***old1",
      has_proxy: true,
      proxy_hint: "socks5h://***@proxy.example:1080/",
      dest_dir: "/archive/old",
      sidecar: true,
    });
    ipc.getBalance.mockResolvedValue({ requests: 10, rate: null, amount: null, currency: null });
  });

  it("shows a Library rescan link when the saved destination could not be registered", async () => {
    dialog.open.mockResolvedValue("/archive/new");
    ipc.saveSettings.mockResolvedValue({
      has_token: false,
      token_hint: null,
      has_proxy: true,
      proxy_hint: "socks5h://***@proxy.example:1080/",
      dest_dir: "/archive/new",
      sidecar: true,
      catalog_warning: "Settings were saved, but the folder could not be added to the Library.",
    });
    const { wrapper } = await renderSettings();

    await wrapper.findAll("button").find((button) => button.text() === "Browse…")!.trigger("click");
    await flushPromises();

    expect(ipc.saveSettings).toHaveBeenCalledWith({ dest_dir: "/archive/new" });
    expect(wrapper.text()).toContain("Settings were saved");
    const link = wrapper.get('a[href="/library"]');
    expect(link.text()).toContain("Library");
  });

  it("shows the masked current token and keeps empty replacement disabled", async () => {
    const { wrapper } = await renderSettings();

    expect(wrapper.get("[data-testid='token-hint']").text()).toContain("***old1");
    expect(wrapper.get("input[name='hiker-token']").attributes("type")).toBe("password");
    expect(wrapper.get("[data-testid='replace-token']").attributes("disabled")).toBeDefined();
  });

  it("shows the safe current proxy hint without exposing credentials", async () => {
    const { wrapper } = await renderSettings();

    expect(wrapper.get("[data-testid='proxy-hint']").text()).toBe(
      "socks5h://***@proxy.example:1080/",
    );
    const input = wrapper.get<HTMLInputElement>("input[name='network-proxy']");
    expect(input.attributes("type")).toBe("password");
    expect(input.element.value).toBe("");
    expect(wrapper.html()).not.toContain("alice");
    expect(wrapper.html()).not.toContain("secret");
  });

  it("applies a trimmed replacement proxy and updates the safe stored hint", async () => {
    ipc.setProxy.mockResolvedValue({
      has_token: true,
      token_hint: "***old1",
      has_proxy: true,
      proxy_hint: "socks5h://***@new-proxy.example:1080/",
      dest_dir: "/archive/old",
      sidecar: true,
    });
    const { app, wrapper } = await renderSettings();
    const input = wrapper.get<HTMLInputElement>("input[name='network-proxy']");
    await input.setValue("  socks5h://alice:secret@proxy.example:1080  ");

    await wrapper.get("[data-testid='proxy-form']").trigger("submit");
    await flushPromises();

    expect(ipc.setProxy).toHaveBeenCalledWith("socks5h://alice:secret@proxy.example:1080");
    expect(app.hasProxy).toBe(true);
    expect(app.proxyHint).toBe("socks5h://***@new-proxy.example:1080/");
    expect(wrapper.get("[data-testid='proxy-hint']").text()).toBe(
      "socks5h://***@new-proxy.example:1080/",
    );
    expect(input.element.value).toBe("");
    expect(wrapper.html()).not.toContain("alice:secret");
    expect(wrapper.get("[data-testid='proxy-success']").text()).toBe(
      "Proxy applied to HikerAPI and Instagram CDN",
    );
  });

  it("clears a configured proxy and shows a direct connection", async () => {
    ipc.setProxy.mockResolvedValue({
      has_token: true,
      token_hint: "***old1",
      has_proxy: false,
      proxy_hint: null,
      dest_dir: "/archive/old",
      sidecar: true,
    });
    const { app, wrapper } = await renderSettings();

    await wrapper.get("[data-testid='clear-proxy']").trigger("click");
    await flushPromises();

    expect(ipc.setProxy).toHaveBeenCalledWith(null);
    expect(app.hasProxy).toBe(false);
    expect(app.proxyHint).toBeNull();
    expect(wrapper.get("[data-testid='proxy-hint']").text()).toBe("Direct connection");
    expect(wrapper.find("[data-testid='clear-proxy']").exists()).toBe(false);
    expect(wrapper.get("[data-testid='proxy-success']").text()).toBe("Proxy cleared");
  });

  it("keeps the configured proxy and entered replacement when applying fails", async () => {
    ipc.setProxy.mockRejectedValue(new Error("Proxy URL must use a supported scheme."));
    const { app, wrapper } = await renderSettings();
    const input = wrapper.get<HTMLInputElement>("input[name='network-proxy']");
    await input.setValue("bad-proxy");

    await wrapper.get("[data-testid='proxy-form']").trigger("submit");
    await flushPromises();

    expect(ipc.setProxy).toHaveBeenCalledOnce();
    expect(ipc.configState).toHaveBeenCalledOnce();
    expect(app.hasProxy).toBe(true);
    expect(app.proxyHint).toBe("socks5h://***@proxy.example:1080/");
    expect(input.element.value).toBe("bad-proxy");
    expect(wrapper.get("[data-testid='proxy-error']").text()).toBe(
      "Proxy URL must use a supported scheme.",
    );
  });

  it("prevents duplicate pending proxy applies and disables its controls", async () => {
    const pending = deferred<never>();
    ipc.setProxy.mockReturnValueOnce(pending.promise);
    const { wrapper } = await renderSettings();
    const input = wrapper.get<HTMLInputElement>("input[name='network-proxy']");
    input.element.focus();
    await input.setValue("http://proxy.example:8080");

    await wrapper.get("[data-testid='proxy-form']").trigger("submit");
    await wrapper.get("[data-testid='proxy-form']").trigger("submit");

    expect(ipc.setProxy).toHaveBeenCalledOnce();
    expect(input.attributes("disabled")).toBeDefined();
    expect(wrapper.get("[data-testid='apply-proxy']").attributes("disabled")).toBeDefined();
    expect(wrapper.get("[data-testid='clear-proxy']").attributes("disabled")).toBeDefined();
    expect(document.activeElement).toBe(input.element);

    pending.reject(new Error("Proxy URL is invalid."));
    await flushPromises();
    expect(ipc.setProxy).toHaveBeenCalledOnce();
    expect(document.activeElement).toBe(input.element);
  });

  it("keeps the configured proxy after a failed clear without exposing sensitive details", async () => {
    ipc.setProxy.mockRejectedValue(
      new Error("failed to save socks5h://alice:secret@proxy.example:1080 in /Users/private/config"),
    );
    const { app, wrapper } = await renderSettings();

    await wrapper.get("[data-testid='clear-proxy']").trigger("click");
    await flushPromises();

    expect(ipc.setProxy).toHaveBeenCalledWith(null);
    expect(app.hasProxy).toBe(true);
    expect(wrapper.find("[data-testid='clear-proxy']").exists()).toBe(true);
    const error = wrapper.get("[data-testid='proxy-error']").text();
    expect(error).toBe("Proxy settings could not be saved. The previous proxy is still active.");
    expect(error).not.toContain("alice");
    expect(error).not.toContain("/Users/private");
  });

  it("validates a trimmed token before replacing the hint and balance", async () => {
    const replacementBalance = { requests: 99, rate: 0.001, amount: 1, currency: "USD" };
    ipc.validateToken.mockResolvedValue(replacementBalance);
    ipc.configState
      .mockResolvedValueOnce({
        has_token: true,
        token_hint: "***old1",
        has_proxy: true,
        proxy_hint: "socks5h://***@proxy.example:1080/",
        dest_dir: "/archive/old",
        sidecar: true,
      })
      .mockResolvedValueOnce({
        has_token: true,
        token_hint: "***new9",
        has_proxy: true,
        proxy_hint: "socks5h://***@proxy.example:1080/",
        dest_dir: "/archive/old",
        sidecar: true,
      });
    const { app, wrapper } = await renderSettings();
    const input = wrapper.get<HTMLInputElement>("input[name='hiker-token']");
    await input.setValue("  fresh-token  ");

    await wrapper.get("[data-testid='token-form']").trigger("submit");
    await flushPromises();

    expect(ipc.validateToken).toHaveBeenCalledWith("fresh-token");
    expect(ipc.configState).toHaveBeenCalledTimes(2);
    expect(app.tokenHint).toBe("***new9");
    expect(app.balance).toEqual(replacementBalance);
    expect(input.element.value).toBe("");
    expect(wrapper.get("[data-testid='token-success']").text()).toContain("99 req");
  });

  it("keeps the old token state and entered value when validation fails", async () => {
    ipc.validateToken.mockRejectedValue(new Error("Invalid token"));
    const { app, wrapper } = await renderSettings();
    const previousBalance = app.balance;
    const input = wrapper.get<HTMLInputElement>("input[name='hiker-token']");
    await input.setValue("bad-token");

    await wrapper.get("[data-testid='token-form']").trigger("submit");
    await flushPromises();

    expect(app.tokenHint).toBe("***old1");
    expect(app.balance).toBe(previousBalance);
    expect(input.element.value).toBe("bad-token");
    expect(wrapper.get("[data-testid='token-error']").text()).toContain("Invalid token");
  });

  it("keeps the previous destination and shows a sanitized error when saving fails", async () => {
    dialog.open.mockResolvedValue("/archive/new");
    ipc.saveSettings.mockRejectedValue(
      new Error("failed to write /Users/private/.config/insta-dl-gui/config.json"),
    );
    const { wrapper } = await renderSettings();

    await wrapper.findAll("button").find((button) => button.text() === "Browse…")!.trigger("click");
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
        has_proxy: true,
        proxy_hint: "socks5h://***@proxy.example:1080/",
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
