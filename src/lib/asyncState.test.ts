import { describe, expect, it, vi } from "vitest";
import { createExplorerRequestState, createRequestGate, runOnce } from "./asyncState";

describe("createRequestGate", () => {
  it("invalidates an older request when a newer intent starts", () => {
    const gate = createRequestGate();
    const old = gate.begin();
    const current = gate.begin();

    expect(gate.isCurrent(old)).toBe(false);
    expect(gate.isCurrent(current)).toBe(true);
  });

  it("invalidates a pending request when the UI closes it", () => {
    const gate = createRequestGate();
    const token = gate.begin();

    gate.invalidate();

    expect(gate.isCurrent(token)).toBe(false);
  });
});

describe("createExplorerRequestState", () => {
  it("keeps unrelated request streams independent", () => {
    const requests = createExplorerRequestState();
    const profile = requests.profile.begin();

    requests.autocomplete.begin();
    requests.autocomplete.invalidate();

    expect(requests.profile.snapshot()).toBe(profile);
    expect(requests.profile.isCurrent(profile)).toBe(true);
    requests.profile.begin();
    expect(requests.profile.isCurrent(profile)).toBe(false);
  });
});

describe("runOnce", () => {
  it("runs only one action per key and releases the key", async () => {
    const active = new Set<string>();
    let release!: () => void;
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    const action = vi.fn(() => pending);

    const first = runOnce(active, "avatar:nike", action);
    const second = runOnce(active, "avatar:nike", action);

    expect(action).toHaveBeenCalledTimes(1);
    expect(active.has("avatar:nike")).toBe(true);
    release();
    await Promise.all([first, second]);
    expect(active.has("avatar:nike")).toBe(false);
  });

  it("releases the key when the action rejects", async () => {
    const active = new Set<string>();

    await expect(
      runOnce(active, "stories:nike", async () => {
        throw new Error("network");
      }),
    ).rejects.toThrow("network");

    expect(active.has("stories:nike")).toBe(false);
  });
});
