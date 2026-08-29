import { defineStore } from "pinia";
import { ref } from "vue";

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

  function beginProfileLoad() {
    profilePreview.value = null;
    activeTab.value = "posts";
    reels.value = [];
    reelsCursor.value = null;
    reelsLoaded.value = false;
    stories.value = null;
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
    endCursor: string | null,
  ): boolean {
    if (profilePreview.value?.profile.pk !== userId) return false;
    reels.value = mergeUniquePosts(reels.value, posts);
    reelsCursor.value = endCursor;
    reelsLoaded.value = true;
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
    beginProfileLoad,
    commitProfile,
    commitMorePosts,
    commitReelsPage,
  };
});
