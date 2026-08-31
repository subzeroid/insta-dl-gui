import { describe, expect, it } from "vitest";

import { canonicalInstagramUrl, classifyPost } from "./postDisplay";

describe("classifyPost", () => {
  it("classifies one photo resource as a photo", () => {
    expect(classifyPost({ resources: [{ url: "photo.jpg", kind: "photo" }] })).toEqual({
      kind: "photo",
      count: 1,
    });
  });

  it("classifies one video resource as a video", () => {
    expect(classifyPost({ resources: [{ url: "video.mp4", kind: "video" }] })).toEqual({
      kind: "video",
      count: 1,
    });
  });

  it("classifies multiple resources as a carousel with their count", () => {
    expect(
      classifyPost({
        resources: [
          { url: "photo.jpg", kind: "photo" },
          { url: "video.mp4", kind: "video" },
        ],
      }),
    ).toEqual({ kind: "carousel", count: 2 });
  });

  it("classifies an empty resource list as unknown", () => {
    expect(classifyPost({ resources: [] })).toEqual({ kind: "unknown", count: 0 });
  });
});

describe("canonicalInstagramUrl", () => {
  it("builds a canonical posts URL with an encoded code", () => {
    expect(canonicalInstagramUrl("a/b c", "posts")).toBe("https://www.instagram.com/p/a%2Fb%20c/");
  });

  it("builds a canonical reels URL with an encoded code", () => {
    expect(canonicalInstagramUrl("a/b c", "reels")).toBe("https://www.instagram.com/reel/a%2Fb%20c/");
  });
});
