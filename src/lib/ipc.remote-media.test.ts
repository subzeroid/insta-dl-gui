import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const MOCK_REMOTE_MEDIA_URL_RESOLVER = Symbol.for(
  "insta-dl-gui.mock-remote-media-url-resolver",
);

function tauriConvertFileSrc(
  os: "unix" | "windows",
): (path: string, protocol?: string) => string {
  return (path, protocol = "asset") => {
    const encoded = encodeURIComponent(path);
    return os === "windows"
      ? `http://${protocol}.localhost/${encoded}`
      : `${protocol}://localhost/${encoded}`;
  };
}

const core = vi.hoisted(() => ({
  convertFileSrc: vi.fn<(path: string, protocol?: string) => string>(),
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
  core.convertFileSrc.mockReset().mockImplementation(tauriConvertFileSrc("unix"));
});

afterEach(() => {
  delete (globalThis as unknown as Record<PropertyKey, unknown>)[
    MOCK_REMOTE_MEDIA_URL_RESOLVER
  ];
});

describe("remoteMediaUrl", () => {
  it("encodes the complete UTF-8 HTTPS URL as one canonical URL-safe base64 path segment", () => {
    const upstream =
      "https://scontent.cdninstagram.com/фото.jpg?x=hello world&sig=a/b+=";

    expect(remoteMediaUrl(upstream)).toBe(
      "remote-media://localhost/media/aHR0cHM6Ly9zY29udGVudC5jZG5pbnN0YWdyYW0uY29tL9GE0L7RgtC-LmpwZz94PWhlbGxvIHdvcmxkJnNpZz1hL2IrPQ",
    );
    expect(core.convertFileSrc).toHaveBeenCalledWith(
      "media",
      "remote-media",
    );
    expect(remoteMediaUrl(upstream)).not.toContain("%2F");
  });

  it("builds the same literal media path delimiter for Tauri's Windows URL shape", () => {
    core.convertFileSrc.mockImplementation(tauriConvertFileSrc("windows"));

    const result = remoteMediaUrl("https://cdninstagram.com/media.jpg?sig=a/b");

    expect(result).toMatch(/^http:\/\/remote-media\.localhost\/media\/[A-Za-z0-9_-]+$/);
    expect(result).not.toContain("%2F");
    expect(core.convertFileSrc).toHaveBeenCalledWith("media", "remote-media");
  });

  it.each([
    "",
    "not a URL",
    "data:image/svg+xml,%3Csvg%3E%3C/svg%3E",
    "data:video/mp4;base64,AAAA",
    "http://127.0.0.1/mock.jpg",
    "ftp://cdninstagram.com/media.jpg",
    "javascript:alert(1)",
    "blob:https://cdninstagram.com/id",
  ])(
    "fails closed for non-HTTPS production input %s",
    (url) => {
      expect(remoteMediaUrl(url)).toBe("");
      expect(core.convertFileSrc).not.toHaveBeenCalled();
    },
  );

  it("permits only the exact media value approved by an explicit mock resolver", () => {
    const fixture = "data:image/svg+xml,%3Csvg%3Eknown%3C/svg%3E";
    const resolver = vi.fn((value: string) => (value === fixture ? value : undefined));
    (globalThis as unknown as Record<PropertyKey, unknown>)[MOCK_REMOTE_MEDIA_URL_RESOLVER] =
      resolver;

    expect(remoteMediaUrl(fixture)).toBe(fixture);
    expect(remoteMediaUrl("data:image/svg+xml,%3Csvg%3Eunknown%3C/svg%3E")).toBe("");
    expect(resolver).toHaveBeenCalledTimes(2);
    expect(core.convertFileSrc).not.toHaveBeenCalled();
  });

  it("fails closed before base64 conversion when an HTTPS URL exceeds the protocol limit", () => {
    const oversized = `https://cdninstagram.com/${"a".repeat(16 * 1024)}`;

    expect(remoteMediaUrl(oversized)).toBe("");
    expect(core.convertFileSrc).not.toHaveBeenCalled();
  });
});
