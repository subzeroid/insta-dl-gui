/** @vitest-environment happy-dom */

import { afterEach, describe, expect, it } from "vitest";
import type { ProfilePreview } from "./ipc";
import { installTauriMock } from "./mock";

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

function invoke(): Invoke {
  return (window as unknown as { __TAURI_INTERNALS__: { invoke: Invoke } }).__TAURI_INTERNALS__.invoke;
}

afterEach(() => {
  delete (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
});

describe("profile pagination mock", () => {
  it("returns a distinct final page for the supplied end cursor", async () => {
    installTauriMock();
    const first = (await invoke()("fetch_profile", {
      username: "instagram",
      endCursor: null,
    })) as ProfilePreview;
    const second = (await invoke()("fetch_profile", {
      username: "instagram",
      endCursor: first.end_cursor,
    })) as ProfilePreview;

    const allIds = [...first.recent_posts, ...second.recent_posts].map((post) => post.pk);
    expect(first.end_cursor).toBe("cursor");
    expect(second.end_cursor).toBeNull();
    expect(first.recent_posts).toHaveLength(12);
    expect(second.recent_posts).toHaveLength(12);
    expect(new Set(allIds).size).toBe(24);
  });
});
