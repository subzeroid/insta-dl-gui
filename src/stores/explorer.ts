import { defineStore } from "pinia";
import { reactive, ref, type Ref } from "vue";

import type { Post, ProfilePreview, StoryItem } from "../lib/ipc";
import { mergeUniquePosts } from "../lib/mediaPages";

export type ExploreTab = "posts" | "reels" | "stories";
export type PostFilter = "all" | "photos" | "videos" | "carousels";
export interface ExploreSelectionSnapshot {
  pk: string;
  revision: number;
}

export const useExplorerStore = defineStore("explorer", () => {
  const query = ref("");
  const profilePreview = ref<ProfilePreview | null>(null);
  const activeTab = ref<ExploreTab>("posts");
  const postFilter = ref<PostFilter>("all");
  const reels = ref<Post[]>([]);
  const reelsCursor = ref<string | null>(null);
  const reelsLoaded = ref(false);
  const stories = ref<StoryItem[] | null>(null);
  const storiesError: Ref<string | null> = ref(null);
  const storiesLoading = ref(false);
  const selected = reactive<Record<ExploreTab, string[]>>({
    posts: [],
    reels: [],
    stories: [],
  });
  const selectionRevisions: Record<ExploreTab, Map<string, number>> = {
    posts: new Map(),
    reels: new Map(),
    stories: new Map(),
  };
  let nextSelectionRevision = 0;
  const requestedReelsCursors = new Set<string>();
  let storiesRequestGeneration = 0;
  let pendingStoriesRequest: { username: string; token: number } | null = null;

  function beginProfileLoad() {
    profilePreview.value = null;
    activeTab.value = "posts";
    postFilter.value = "all";
    reels.value = [];
    reelsCursor.value = null;
    reelsLoaded.value = false;
    stories.value = null;
    storiesError.value = null;
    storiesLoading.value = false;
    storiesRequestGeneration += 1;
    pendingStoriesRequest = null;
    selected.posts = [];
    selected.reels = [];
    selected.stories = [];
    selectionRevisions.posts.clear();
    selectionRevisions.reels.clear();
    selectionRevisions.stories.clear();
    requestedReelsCursors.clear();
  }

  function commitProfile(value: ProfilePreview) {
    profilePreview.value = value;
  }

  function commitMorePosts(username: string, page: ProfilePreview): boolean {
    if (
      profilePreview.value?.profile.username !== username ||
      page.profile.username !== username
    ) {
      return false;
    }
    profilePreview.value = {
      profile: page.profile,
      recent_posts: mergeUniquePosts(profilePreview.value.recent_posts, page.recent_posts),
      end_cursor: page.end_cursor,
    };
    return true;
  }

  function commitReelsPage(
    userId: string,
    posts: readonly Post[],
    requestedCursor: string | null,
    endCursor: string | null,
  ): boolean {
    if (profilePreview.value?.profile.pk !== userId) return false;
    const requested = requestedCursor?.trim();
    if (requested) requestedReelsCursors.add(requested);
    const next = endCursor?.trim() || null;
    reels.value = mergeUniquePosts(reels.value, posts);
    reelsCursor.value = next && !requestedReelsCursors.has(next) ? next : null;
    reelsLoaded.value = true;
    return true;
  }

  function toggleSelected(tab: ExploreTab, pk: string) {
    const index = selected[tab].indexOf(pk);
    if (index >= 0) {
      selected[tab].splice(index, 1);
      selectionRevisions[tab].delete(pk);
    } else {
      selected[tab].push(pk);
      selectionRevisions[tab].set(pk, ++nextSelectionRevision);
    }
  }

  function isSelected(tab: ExploreTab, pk: string): boolean {
    return selected[tab].includes(pk);
  }

  function selectionSnapshot(tab: ExploreTab): ExploreSelectionSnapshot[] {
    return selected[tab].map((pk) => {
      let revision = selectionRevisions[tab].get(pk);
      if (revision === undefined) {
        revision = ++nextSelectionRevision;
        selectionRevisions[tab].set(pk, revision);
      }
      return { pk, revision };
    });
  }

  function clearSubmitted(tab: ExploreTab, submittedEntries: readonly ExploreSelectionSnapshot[]) {
    const submitted = new Map(submittedEntries.map((entry) => [entry.pk, entry.revision]));
    selected[tab] = selected[tab].filter((pk) => {
      const submittedRevision = submitted.get(pk);
      if (
        submittedRevision === undefined ||
        submittedRevision !== selectionRevisions[tab].get(pk)
      ) {
        return true;
      }
      selectionRevisions[tab].delete(pk);
      return false;
    });
  }

  function beginStoriesRequest(username: string): number | null {
    if (profilePreview.value?.profile.username !== username || pendingStoriesRequest) return null;
    const token = ++storiesRequestGeneration;
    pendingStoriesRequest = { username, token };
    storiesLoading.value = true;
    storiesError.value = null;
    return token;
  }

  function isCurrentStoriesRequest(username: string, token: number): boolean {
    return (
      profilePreview.value?.profile.username === username &&
      pendingStoriesRequest?.username === username &&
      pendingStoriesRequest.token === token
    );
  }

  function commitStories(username: string, token: number, items: readonly StoryItem[]): boolean {
    if (!isCurrentStoriesRequest(username, token)) return false;
    stories.value = [...items];
    storiesError.value = null;
    storiesLoading.value = false;
    pendingStoriesRequest = null;
    const available = new Set(items.map((item) => item.pk));
    selected.stories = selected.stories.filter((pk) => available.has(pk));
    for (const pk of selectionRevisions.stories.keys()) {
      if (!available.has(pk)) selectionRevisions.stories.delete(pk);
    }
    return true;
  }

  function failStories(username: string, token: number, message: string): boolean {
    if (!isCurrentStoriesRequest(username, token)) return false;
    storiesError.value = message;
    storiesLoading.value = false;
    pendingStoriesRequest = null;
    return true;
  }

  return {
    query,
    profilePreview,
    activeTab,
    postFilter,
    reels,
    reelsCursor,
    reelsLoaded,
    stories,
    storiesError,
    storiesLoading,
    selected,
    beginProfileLoad,
    commitProfile,
    commitMorePosts,
    commitReelsPage,
    toggleSelected,
    isSelected,
    selectionSnapshot,
    clearSubmitted,
    beginStoriesRequest,
    commitStories,
    failStories,
  };
});
