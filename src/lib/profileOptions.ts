import type { ProfileOptions } from "./ipc";

export function buildProfileOptions(
  isPrivate: boolean,
  selections: Readonly<ProfileOptions>,
): ProfileOptions {
  if (!isPrivate) return { ...selections };
  return {
    posts: false,
    reels: false,
    stories: false,
    highlights: false,
    avatar: selections.avatar,
    max_posts: null,
  };
}
