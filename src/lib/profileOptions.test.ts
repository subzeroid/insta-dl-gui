import { describe, expect, it } from "vitest";
import { buildProfileOptions } from "./profileOptions";

const selections = {
  posts: true,
  reels: true,
  stories: true,
  highlights: true,
  avatar: false,
  max_posts: 25,
};

describe("buildProfileOptions", () => {
  it("submits only the visible avatar choice for private profiles", () => {
    expect(buildProfileOptions(true, selections)).toEqual({
      posts: false,
      reels: false,
      stories: false,
      highlights: false,
      avatar: false,
      max_posts: null,
    });
  });

  it("preserves public-profile selections", () => {
    expect(buildProfileOptions(false, selections)).toEqual(selections);
  });
});
