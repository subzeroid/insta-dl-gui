import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { getVersion } from "@tauri-apps/api/app";
import * as ipc from "../lib/ipc";
import { useAppStore } from "./app";

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(),
}));

vi.mock("../lib/ipc", () => ({
  configState: vi.fn(),
}));

const configStateFixture: ipc.ConfigState = {
  has_token: false,
  token_hint: null,
  has_proxy: false,
  proxy_hint: null,
  dest_dir: "",
  sidecar: true,
};

async function flushPromises() {
  await Promise.resolve();
  await Promise.resolve();
}

describe("app store version state", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setActivePinia(createPinia());
    vi.mocked(ipc.configState).mockResolvedValue(configStateFixture);
  });

  it("normalizes and exposes a resolved version", async () => {
    vi.mocked(getVersion).mockResolvedValue(" 0.6.0 ");
    const app = useAppStore();

    await app.init();
    await flushPromises();

    expect(app.appVersion).toBe("0.6.0");
    expect(app.ready).toBe(true);
    expect(getVersion).toHaveBeenCalledOnce();
  });

  it("keeps startup successful when version lookup rejects", async () => {
    vi.mocked(getVersion).mockRejectedValue(new Error("version unavailable"));
    const app = useAppStore();

    await app.init();
    await flushPromises();

    expect(app.ready).toBe(true);
    expect(app.appVersion).toBeNull();
    expect(getVersion).toHaveBeenCalledOnce();
  });

  it.each(["", "   "])("normalizes a blank version %j to null", async (version) => {
    vi.mocked(getVersion).mockResolvedValue(version);
    const app = useAppStore();

    await app.init();
    await flushPromises();

    expect(app.appVersion).toBeNull();
  });

  it("does not wait for a never-resolving version lookup", async () => {
    vi.mocked(getVersion).mockReturnValue(new Promise<string>(() => {}));
    const app = useAppStore();

    await app.init();

    expect(app.ready).toBe(true);
    expect(app.appVersion).toBeNull();
  });
});
