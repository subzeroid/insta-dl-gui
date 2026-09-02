/** @vitest-environment happy-dom */

import { createPinia, setActivePinia } from "pinia";
import { flushPromises, mount } from "@vue/test-utils";
import { nextTick } from "vue";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const router = vi.hoisted(() => ({ push: vi.fn() }));

vi.mock("vue-router", () => ({ useRouter: () => router }));
vi.mock("../lib/ipc", () => ({}));

import { useAppStore } from "../stores/app";
import { useRemoteMediaHealthStore } from "../stores/remoteMediaHealth";
import RemoteMediaFailureBanner from "./RemoteMediaFailureBanner.vue";

const mountedWrappers: Array<{ unmount: () => void }> = [];

function renderBanner() {
  const pinia = createPinia();
  setActivePinia(pinia);
  const app = useAppStore();
  const health = useRemoteMediaHealthStore();
  const host = document.createElement("div");
  document.body.append(host);
  const wrapper = mount(RemoteMediaFailureBanner, {
    attachTo: host,
    global: { plugins: [pinia] },
  });
  mountedWrappers.push(wrapper);
  return { app, health, wrapper };
}

async function renderActiveBanner() {
  const rendered = renderBanner();
  rendered.health.reportFailure("https://cdn.example/first.jpg?token=private");
  rendered.health.reportFailure("https://cdn.example/second.jpg?token=secret");
  await nextTick();
  return rendered;
}

function appendMainTarget() {
  const main = document.createElement("main");
  main.id = "app-main-content";
  main.tabIndex = -1;
  document.body.append(main);
  return main;
}

beforeEach(() => {
  router.push.mockReset();
  router.push.mockResolvedValue(undefined);
});

afterEach(() => {
  for (const wrapper of mountedWrappers.splice(0)) wrapper.unmount();
  document.body.replaceChildren();
});

describe("RemoteMediaFailureBanner", () => {
  it("keeps an empty live region mounted before announcing failures", async () => {
    const { health, wrapper } = renderBanner();

    const status = wrapper.get("[data-testid='remote-media-status']");
    expect(status.attributes("role")).toBe("status");
    expect(status.attributes("aria-live")).toBe("polite");
    expect(status.attributes("aria-atomic")).toBe("true");
    expect(status.text()).toBe("");
    expect(wrapper.find("[data-testid='remote-media-failure-banner']").exists()).toBe(false);

    health.reportFailure("https://cdn.example/first.jpg?token=private");
    health.reportFailure("https://cdn.example/second.jpg?token=secret");
    await nextTick();

    const banner = wrapper.get("[data-testid='remote-media-failure-banner']");
    expect(banner.attributes("role")).toBeUndefined();
    expect(banner.attributes("aria-live")).toBeUndefined();
    expect(banner.get("h2").text()).toBe("Instagram previews are unavailable");
    expect(banner.get("p").text()).toBe(
      "Your network may be blocking Instagram media. Turn on a VPN or configure a proxy in Settings.",
    );
  });

  it("summarizes multiple direct-connection failures once without exposing URLs or secrets", async () => {
    const { app, health, wrapper } = await renderActiveBanner();
    app.proxyHint = "socks5h://alice:password@proxy.example:1080";
    health.reportFailure("https://cdn.example/third.jpg?credential=hidden");

    const banners = wrapper.findAll("[data-testid='remote-media-failure-banner']");
    expect(banners).toHaveLength(1);
    expect(banners[0].get("h2").text()).toBe("Instagram previews are unavailable");
    expect(banners[0].get("p").text()).toBe(
      "Your network may be blocking Instagram media. Turn on a VPN or configure a proxy in Settings.",
    );
    expect(wrapper.html()).not.toContain("cdn.example");
    expect(wrapper.html()).not.toContain("alice");
    expect(wrapper.html()).not.toContain("password");
  });

  it("reactively gives safe proxy-specific guidance", async () => {
    const { app, wrapper } = await renderActiveBanner();

    app.hasProxy = true;
    await nextTick();

    expect(wrapper.get("p").text()).toBe(
      "Check the configured proxy or try a VPN, then retry previews.",
    );
  });

  it("orders and styles the Open Settings, Retry, and Dismiss actions", async () => {
    const { wrapper } = await renderActiveBanner();
    const banner = wrapper.get("[data-testid='remote-media-failure-banner']");
    const buttons = banner.findAll("button");

    expect(buttons.map((button) => button.text())).toEqual([
      "Open Settings",
      "Retry",
      "Dismiss",
    ]);
    expect(buttons.map((button) => button.classes())).toEqual([
      expect.arrayContaining(["btn-secondary"]),
      expect.arrayContaining(["btn-primary"]),
      expect.arrayContaining(["btn-secondary"]),
    ]);
    expect(banner.classes()).toEqual(
      expect.arrayContaining(["border-b", "border-warn/30", "bg-warn/10"]),
    );
  });

  it("opens the proxy settings anchor and focuses it after navigation", async () => {
    const { wrapper } = await renderActiveBanner();
    const proxyCard = document.createElement("form");
    proxyCard.id = "network-proxy";
    proxyCard.tabIndex = -1;
    document.body.append(proxyCard);

    await wrapper.get("button").trigger("click");
    await nextTick();

    expect(router.push).toHaveBeenCalledWith({ path: "/settings", hash: "#network-proxy" });
    expect(document.activeElement).toBe(proxyCard);
  });

  it("retries all previews, advances the generation, and hides itself", async () => {
    const { health, wrapper } = await renderActiveBanner();
    const retryAll = vi.spyOn(health, "retryAll");
    const main = appendMainTarget();
    const retry = wrapper.findAll<HTMLButtonElement>("button")[1];
    retry.element.focus();
    expect(document.activeElement).toBe(retry.element);

    await retry.trigger("click");
    await flushPromises();

    expect(retryAll).toHaveBeenCalledOnce();
    expect(health.retryGeneration).toBe(1);
    expect(wrapper.find("[data-testid='remote-media-failure-banner']").exists()).toBe(false);
    expect(document.activeElement).toBe(main);
    expect(wrapper.find("[data-testid='remote-media-status']").exists()).toBe(true);
  });

  it("dismisses the banner and suppresses later failures for the session", async () => {
    const { health, wrapper } = await renderActiveBanner();
    const dismiss = vi.spyOn(health, "dismiss");
    const main = appendMainTarget();
    const dismissButton = wrapper.findAll<HTMLButtonElement>("button")[2];
    dismissButton.element.focus();
    expect(document.activeElement).toBe(dismissButton.element);

    await dismissButton.trigger("click");
    await flushPromises();
    health.reportFailure("https://cdn.example/later-one.jpg");
    health.reportFailure("https://cdn.example/later-two.jpg");
    await nextTick();

    expect(dismiss).toHaveBeenCalledOnce();
    expect(health.dismissed).toBe(true);
    expect(health.bannerVisible).toBe(false);
    expect(wrapper.find("[data-testid='remote-media-failure-banner']").exists()).toBe(false);
    expect(document.activeElement).toBe(main);
    expect(wrapper.find("[data-testid='remote-media-status']").exists()).toBe(true);
  });
});
