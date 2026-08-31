import { beforeEach, describe, expect, it, vi } from "vitest";

const core = vi.hoisted(() => ({
  convertFileSrc: vi.fn((path: string, protocol = "asset") => `${protocol}://localhost/${path}`),
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => core);
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import * as ipc from "./ipc";

function remoteMediaUrl(url: string): string {
  const helper = (ipc as unknown as { remoteMediaUrl?: (value: string) => string }).remoteMediaUrl;
  expect(helper, "ipc.remoteMediaUrl must exist").toBeTypeOf("function");
  return helper!(url);
}

beforeEach(() => {
  core.convertFileSrc.mockClear();
});

describe("remoteMediaUrl", () => {
  it("encodes the complete UTF-8 HTTPS URL as one canonical URL-safe base64 path segment", () => {
    const upstream =
      "https://scontent.cdninstagram.com/фото.jpg?x=hello world&sig=a/b+=";

    expect(remoteMediaUrl(upstream)).toBe(
      "remote-media://localhost/media/aHR0cHM6Ly9zY29udGVudC5jZG5pbnN0YWdyYW0uY29tL9GE0L7RgtC-LmpwZz94PWhlbGxvIHdvcmxkJnNpZz1hL2IrPQ",
    );
    expect(core.convertFileSrc).toHaveBeenCalledWith(
      "media/aHR0cHM6Ly9zY29udGVudC5jZG5pbnN0YWdyYW0uY29tL9GE0L7RgtC-LmpwZz94PWhlbGxvIHdvcmxkJnNpZz1hL2IrPQ",
      "remote-media",
    );
  });

  it.each(["", "data:image/svg+xml,%3Csvg%3E%3C/svg%3E", "http://127.0.0.1/mock.jpg"])(
    "preserves the non-production preview URL %s",
    (url) => {
      expect(remoteMediaUrl(url)).toBe(url);
      expect(core.convertFileSrc).not.toHaveBeenCalled();
    },
  );

  it("does not turn malformed or non-HTTPS input into a custom protocol URL", () => {
    expect(remoteMediaUrl("not a URL")).toBe("not a URL");
    expect(remoteMediaUrl("ftp://cdninstagram.com/media.jpg")).toBe(
      "ftp://cdninstagram.com/media.jpg",
    );
    expect(core.convertFileSrc).not.toHaveBeenCalled();
  });

  it("fails closed before base64 conversion when an HTTPS URL exceeds the protocol limit", () => {
    const oversized = `https://cdninstagram.com/${"a".repeat(16 * 1024)}`;

    expect(remoteMediaUrl(oversized)).toBe("");
    expect(core.convertFileSrc).not.toHaveBeenCalled();
  });
});
