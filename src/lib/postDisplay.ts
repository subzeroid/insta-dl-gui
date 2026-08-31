import type { Post } from "./ipc";

export type PostDisplayKind = "photo" | "video" | "carousel" | "unknown";

export interface PostDisplayType {
  kind: PostDisplayKind;
  count: number;
}

export function classifyPost(post: Pick<Post, "resources">): PostDisplayType {
  const count = post.resources.length;

  if (count > 1) return { kind: "carousel", count };
  if (count === 0) return { kind: "unknown", count: 0 };
  return { kind: post.resources[0].kind, count: 1 };
}

export function canonicalInstagramUrl(code: string, category: "posts" | "reels"): string {
  const path = category === "posts" ? "p" : "reel";
  return `https://www.instagram.com/${path}/${encodeURIComponent(code)}/`;
}
