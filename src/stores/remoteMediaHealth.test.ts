import { createPinia } from "pinia";
import { describe, expect, it } from "vitest";

import {
  REMOTE_MEDIA_FAILURE_WINDOW_MS,
  createRemoteMediaHealthStore,
} from "./remoteMediaHealth";

function createStore(startedAt = 0) {
  let currentTime = startedAt;
  const store = createRemoteMediaHealthStore(() => currentTime)(createPinia());

  return {
    store,
    setTime(at: number) {
      currentTime = at;
    },
  };
}

describe("remote media health", () => {
  it("requires failures from two unique normalized sources", () => {
    const { store } = createStore();

    store.reportFailure(" https://cdn.example/one.jpg ");
    store.reportFailure("https://cdn.example/one.jpg");
    store.reportFailure("   ");
    expect(store.bannerVisible).toBe(false);

    store.reportFailure("https://cdn.example/two.jpg");
    expect(store.bannerVisible).toBe(true);
  });

  it("keeps the exact failure-window boundary and prunes older failures", () => {
    const inclusive = createStore();
    inclusive.store.reportFailure("one");
    inclusive.setTime(REMOTE_MEDIA_FAILURE_WINDOW_MS);
    inclusive.store.reportFailure("two");
    expect(inclusive.store.bannerVisible).toBe(true);

    const expired = createStore();
    expired.store.reportFailure("one");
    expired.setTime(REMOTE_MEDIA_FAILURE_WINDOW_MS + 1);
    expired.store.reportFailure("two");
    expect(expired.store.bannerVisible).toBe(false);
    expired.store.reportFailure("three");
    expect(expired.store.bannerVisible).toBe(true);
  });

  it("removes a recovered source before activation", () => {
    const { store } = createStore();

    store.reportFailure("one");
    store.reportSuccess(" one ");
    store.reportFailure("two");

    expect(store.bannerVisible).toBe(false);
  });

  it("keeps an activated banner visible after sources recover", () => {
    const { store } = createStore();

    store.reportFailure("one");
    store.reportFailure("two");
    store.reportSuccess("one");
    store.reportSuccess("two");

    expect(store.bannerVisible).toBe(true);
  });

  it("resets the window on retry and permits later reactivation", () => {
    const { store } = createStore();
    store.reportFailure("one");
    store.reportFailure("two");

    store.retryAll();

    expect(store.bannerVisible).toBe(false);
    expect(store.retryGeneration).toBe(1);
    store.reportFailure("two");
    expect(store.bannerVisible).toBe(false);
    store.reportFailure("three");
    expect(store.bannerVisible).toBe(true);
  });

  it("dismisses permanently only for the current store instance", () => {
    const useStore = createRemoteMediaHealthStore(() => 0);
    const dismissedStore = useStore(createPinia());
    dismissedStore.reportFailure("one");

    dismissedStore.dismiss();
    dismissedStore.reportFailure("two");
    dismissedStore.reportFailure("three");

    expect(dismissedStore.dismissed).toBe(true);
    expect(dismissedStore.bannerVisible).toBe(false);

    const freshStore = useStore(createPinia());
    expect(freshStore.dismissed).toBe(false);
    freshStore.reportFailure("one");
    freshStore.reportFailure("two");
    expect(freshStore.bannerVisible).toBe(true);
  });
});
