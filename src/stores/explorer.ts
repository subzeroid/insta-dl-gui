import { defineStore } from "pinia";
import { reactive, ref, type Ref } from "vue";

import type { Post, ProfilePreview, StoryItem } from "../lib/ipc";
import { mergeUniquePosts } from "../lib/mediaPages";

export type ExploreTab = "posts" | "reels" | "stories";

export const useExplorerStore = defineStore("explorer", () => {
  const query = ref("");
  const profilePreview = ref<ProfilePreview | null>(null);
  const activeTab = ref<ExploreTab>("posts");
  const reels = ref<Post[]>([]);
  const reelsCursor = ref<string | null>(null);
  const reelsLoaded = ref(false);
  const stories = ref<StoryItem[] | null>(null);
  const storiesError: Ref<string | null> = ref(null);
  const selected = reactive<Record<ExploreTab, string[]>>({
    posts: [],
    reels: [],
    stories: [],
  });
  const requestedReelsCursors = new Set<string>();

  function beginProfileLoad() {
    profilePreview.value = null;
    activeTab.value = "posts";
    reels.value = [];
    reelsCursor.value = null;
    reelsLoaded.value = false;
    stories.value = null;
    storiesError.value = null;
    selected.posts = [];
    selected.reels = [];
    selected.stories = [];
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
    } else {
      selected[tab].push(pk);
    }
  }

  function isSelected(tab: ExploreTab, pk: string): boolean {
    return selected[tab].includes(pk);
  }

  function clearSubmitted(tab: ExploreTab, submittedIds: readonly string[]) {
    const submitted = new Set(submittedIds);
    selected[tab] = selected[tab].filter((pk) => !submitted.has(pk));
  }

  function beginStoriesRequest(username: string): boolean {
    if (profilePreview.value?.profile.username !== username) return false;
    storiesError.value = null;
    return true;
  }

  function commitStories(username: string, items: readonly StoryItem[]): boolean {
    if (profilePreview.value?.profile.username !== username) return false;
    stories.value = [...items];
    storiesError.value = null;
    const available = new Set(items.map((item) => item.pk));
    selected.stories = selected.stories.filter((pk) => available.has(pk));
    return true;
  }

  function failStories(username: string, message: string): boolean {
    if (profilePreview.value?.profile.username !== username) return false;
    storiesError.value = message;
    return true;
  }

  return {
    query,
    profilePreview,
    activeTab,
    reels,
    reelsCursor,
    reelsLoaded,
    stories,
    storiesError,
    selected,
    beginProfileLoad,
    commitProfile,
    commitMorePosts,
    commitReelsPage,
    toggleSelected,
    isSelected,
    clearSubmitted,
    beginStoriesRequest,
    commitStories,
    failStories,
  };
});
