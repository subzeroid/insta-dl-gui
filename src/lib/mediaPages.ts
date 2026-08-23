import type { Post } from "./ipc";

export function mergeUniquePosts(current: readonly Post[], incoming: readonly Post[]): Post[] {
  const seen = new Set(current.map((post) => post.pk));
  return [
    ...current,
    ...incoming.filter((post) => {
      if (seen.has(post.pk)) return false;
      seen.add(post.pk);
      return true;
    }),
  ];
}
