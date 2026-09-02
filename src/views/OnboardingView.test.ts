/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const opener = vi.hoisted(() => ({
  openUrl: vi.fn(),
}));
const router = vi.hoisted(() => ({
  push: vi.fn(),
  replace: vi.fn(),
}));
const ipc = vi.hoisted(() => ({
  configState: vi.fn(),
  formatBalance: vi.fn(),
  getBalance: vi.fn(),
  validateToken: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => opener);
vi.mock("vue-router", () => ({
  useRouter: () => router,
}));
vi.mock("../lib/ipc", () => ({
  configState: ipc.configState,
  formatBalance: ipc.formatBalance,
  getBalance: ipc.getBalance,
  validateToken: ipc.validateToken,
}));

import { useAppStore } from "../stores/app";
import OnboardingView from "./OnboardingView.vue";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

function render() {
  const pinia = createPinia();
  setActivePinia(pinia);
  return {
    app: useAppStore(),
    wrapper: mount(OnboardingView, {
    global: { plugins: [pinia] },
    }),
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("OnboardingView", () => {
  it("shows the product name and app version in the heading", async () => {
    const { app, wrapper } = render();
    app.appVersion = "0.6.0";
    await flushPromises();

    expect(wrapper.get("h1 [data-app-name]").text()).toBe("insta-dl-gui");
    expect(wrapper.get("h1 [data-app-version]").text()).toBe("v0.6.0");
  });

  it("reveals the token on demand without changing its value", async () => {
    const { wrapper } = render();
    const input = wrapper.get<HTMLInputElement>("#token");

    expect(input.attributes("type")).toBe("password");
    expect(input.attributes("placeholder")).toBe("Paste your token…");
    expect(input.attributes("autocomplete")).toBe("off");

    await input.setValue("typed-token");
    await wrapper.get("button[aria-label='Show token']").trigger("click");

    expect(input.attributes("type")).toBe("text");
    expect(input.element.value).toBe("typed-token");
    expect(wrapper.get("button[aria-label='Hide token']").attributes("aria-label")).toBe("Hide token");

    await wrapper.get("button[aria-label='Hide token']").trigger("click");

    expect(input.attributes("type")).toBe("password");
    expect(input.element.value).toBe("typed-token");
  });

  it("opens the tracked HikerAPI signup URL", async () => {
    const { wrapper } = render();

    await wrapper.get("a").trigger("click");

    expect(opener.openUrl).toHaveBeenCalledWith("https://hikerapi.com/p/uk064a1b");
  });

  it("updates the shared token state and replaces the route after a successful connection", async () => {
    vi.useFakeTimers();
    const fullToken = "full-token-1234567890";
    const balance = { requests: 42, rate: 0.01, amount: 1.5, currency: "USD" };
    ipc.validateToken.mockResolvedValue(balance);
    ipc.configState.mockResolvedValue({
      has_token: true,
      token_hint: "***7890",
      has_proxy: false,
      proxy_hint: null,
      dest_dir: "/downloads",
      sidecar: true,
      catalog_warning: null,
    });
    ipc.formatBalance.mockReturnValue("42 requests");
    const { app, wrapper } = render();

    await wrapper.get<HTMLInputElement>("#token").setValue(`  ${fullToken}  `);
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(ipc.validateToken).toHaveBeenCalledWith(fullToken);
    expect(ipc.configState).toHaveBeenCalledOnce();
    expect(app.hasToken).toBe(true);
    expect(app.tokenHint).toBe("***7890");
    expect(app.balance).toEqual(balance);
    expect(ipc.getBalance).not.toHaveBeenCalled();
    expect(app.tokenHint).not.toContain(fullToken);
    expect(wrapper.text()).not.toContain(fullToken);

    await vi.advanceTimersByTimeAsync(699);
    expect(router.replace).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);

    expect(router.replace).toHaveBeenCalledOnce();
    expect(router.replace).toHaveBeenCalledWith("/explore");
    expect(router.push).not.toHaveBeenCalled();
  });

  it("prevents duplicate submissions while a successful connection waits to redirect", async () => {
    vi.useFakeTimers();
    const validation = deferred<{ requests: number; rate: number | null; amount: number | null; currency: string | null }>();
    const balance = { requests: 42, rate: null, amount: null, currency: null };
    ipc.validateToken.mockReturnValue(validation.promise);
    ipc.configState.mockResolvedValue({
      has_token: true,
      token_hint: "***7890",
      has_proxy: false,
      proxy_hint: null,
      dest_dir: "/downloads",
      sidecar: true,
      catalog_warning: null,
    });
    ipc.formatBalance.mockReturnValue("42 requests");
    const { wrapper } = render();
    const input = wrapper.get<HTMLInputElement>("#token");
    const form = wrapper.get("form");
    const connect = wrapper.get("button[type='submit']");

    await input.setValue("full-token-1234567890");
    await form.trigger("submit");

    expect(input.attributes("disabled")).toBeDefined();
    expect(connect.attributes("disabled")).toBeDefined();
    await form.trigger("submit");
    await flushPromises();

    expect(ipc.validateToken).toHaveBeenCalledOnce();
    expect(ipc.configState).not.toHaveBeenCalled();

    validation.resolve(balance);
    await flushPromises();

    expect(input.attributes("disabled")).toBeDefined();
    expect(connect.attributes("disabled")).toBeDefined();
    expect(ipc.configState).toHaveBeenCalledOnce();
    expect(ipc.getBalance).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(700);

    expect(router.replace).toHaveBeenCalledOnce();
    expect(router.replace).toHaveBeenCalledWith("/explore");
    expect(router.push).not.toHaveBeenCalled();
  });

  it("does not schedule a redirect when validation resolves after unmount", async () => {
    vi.useFakeTimers();
    const validation = deferred<{ requests: number; rate: number | null; amount: number | null; currency: string | null }>();
    const balance = { requests: 42, rate: null, amount: null, currency: null };
    ipc.validateToken.mockReturnValue(validation.promise);
    ipc.configState.mockResolvedValue({
      has_token: true,
      token_hint: "***7890",
      has_proxy: false,
      proxy_hint: null,
      dest_dir: "/downloads",
      sidecar: true,
      catalog_warning: null,
    });
    const { wrapper } = render();

    await wrapper.get<HTMLInputElement>("#token").setValue("full-token-1234567890");
    await wrapper.get("form").trigger("submit");
    wrapper.unmount();
    validation.resolve(balance);
    await flushPromises();
    await vi.advanceTimersByTimeAsync(701);

    expect(router.replace).not.toHaveBeenCalled();
    expect(router.push).not.toHaveBeenCalled();
  });

  it("cancels a pending success redirect when unmounted", async () => {
    vi.useFakeTimers();
    const balance = { requests: 42, rate: null, amount: null, currency: null };
    ipc.validateToken.mockResolvedValue(balance);
    ipc.configState.mockResolvedValue({
      has_token: true,
      token_hint: "***7890",
      has_proxy: false,
      proxy_hint: null,
      dest_dir: "/downloads",
      sidecar: true,
      catalog_warning: null,
    });
    ipc.formatBalance.mockReturnValue("42 requests");
    const { wrapper } = render();

    await wrapper.get<HTMLInputElement>("#token").setValue("full-token-1234567890");
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    wrapper.unmount();
    await vi.advanceTimersByTimeAsync(701);

    expect(router.replace).not.toHaveBeenCalled();
    expect(router.push).not.toHaveBeenCalled();
  });

  it("renders the validation error without changing state or scheduling navigation", async () => {
    vi.useFakeTimers();
    const validationError = new Error("Token rejected");
    ipc.validateToken.mockRejectedValue(validationError);
    const { app, wrapper } = render();

    await wrapper.get<HTMLInputElement>("#token").setValue("invalid-token");
    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(app.hasToken).toBe(false);
    expect(app.tokenHint).toBeNull();
    expect(app.balance).toBeNull();
    expect(ipc.configState).not.toHaveBeenCalled();
    expect(ipc.getBalance).not.toHaveBeenCalled();
    expect(wrapper.text()).toContain("Error: Token rejected");

    await vi.advanceTimersByTimeAsync(700);

    expect(router.replace).not.toHaveBeenCalled();
    expect(router.push).not.toHaveBeenCalled();
    expect(app.hasToken).toBe(false);
    expect(app.tokenHint).toBeNull();
    expect(app.balance).toBeNull();
  });
});
