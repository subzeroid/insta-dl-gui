import { describe, expect, it } from "vitest";
import { buildProfileOptions, hasProfileSelection } from "./profileOptions";

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

  it("does not treat hidden private-profile choices as downloadable", () => {
    const privateOptions = buildProfileOptions(true, selections);

    expect(hasProfileSelection(privateOptions)).toBe(false);
  });

  it("keeps an explicitly selected private-profile avatar downloadable", () => {
    const privateOptions = buildProfileOptions(true, { ...selections, avatar: true });

    expect(privateOptions.avatar).toBe(true);
    expect(hasProfileSelection(privateOptions)).toBe(true);
  });

  it("recognizes every public download category", () => {
    for (const key of ["posts", "reels", "stories", "highlights", "avatar"] as const) {
      const options = buildProfileOptions(false, {
        posts: false,
        reels: false,
        stories: false,
        highlights: false,
        avatar: false,
        max_posts: null,
        [key]: true,
      });

      expect(hasProfileSelection(options), key).toBe(true);
    }
  });
});
