<script setup lang="ts">
import { storeToRefs } from "pinia";
import { computed, onMounted, onUnmounted, reactive, ref } from "vue";
import {
  downloadDirect,
  downloadPost,
  enqueueFetchedPostDownload,
  enqueueProfileDownload,
  fetchProfile,
  fetchReels,
  fetchStories,
  resolveInput,
  searchUsers,
  type Post,
  type SearchUser,
  type StoryItem,
} from "../lib/ipc";
import { useExplorerStore, type ExploreTab } from "../stores/explorer";
import { useJobsStore } from "../stores/jobs";
import { createExplorerRequestState, runOnce } from "../lib/asyncState";
import DownloadScopeGroup from "../components/DownloadScopeGroup.vue";
import MediaSelectionCheckbox from "../components/MediaSelectionCheckbox.vue";
import PostModal from "../components/PostModal.vue";

const jobs = useJobsStore();
const explorer = useExplorerStore();
const {
  query,
  profilePreview: preview,
  activeTab,
  reels,
  reelsCursor,
  reelsLoaded,
  stories,
  storiesError,
  storiesLoading,
} = storeToRefs(explorer);

const suggestions = ref<SearchUser[]>([]);
const suggestOpen = ref(false);
const highlight = ref(-1);
let debounce = 0;

const loading = ref(false);
const loadingMore = ref(false);
const error = ref<string | null>(null);

const reelsLoading = ref(false);
const reelsError = ref<string | null>(null);
const reelsRetryCursor = ref<string | null>(null);

const modalPost = ref<Post | null>(null);
const modalStory = ref<StoryItem | null>(null);

const requests = createExplorerRequestState();
const activeActions = reactive(new Set<string>());
let profileSession = Symbol("explorer-profile-session");
const MAX_EXACT_SNAPSHOT_ITEMS = 500;

const tabs = [
  { id: "posts", label: "Posts" },
  { id: "reels", label: "Reels" },
  { id: "stories", label: "Stories" },
] as const;

function hasVideo(p: Post): boolean {
  return p.resources.some((r) => r.kind === "video");
}

const gridPosts = computed(() => (activeTab.value === "reels" ? reels.value : (preview.value?.recent_posts ?? [])));
const shownCount = computed(() =>
  activeTab.value === "stories" ? (stories.value?.length ?? 0) : gridPosts.value.length,
);
const selectedIdSet = computed(() => new Set(explorer.selected[activeTab.value]));
const selectedCount = computed(() => explorer.selected[activeTab.value].length);
const snapshotHelperText =
  "Exact Shown and Selected snapshots are limited to 500 items. Use All for a complete archive.";
const shownDisabledReason = computed(() =>
  shownCount.value > MAX_EXACT_SNAPSHOT_ITEMS
    ? `Shown has ${shownCount.value} items, above the 500-item exact snapshot limit.`
    : undefined,
);
const selectedDisabledReason = computed(() =>
  selectedCount.value > MAX_EXACT_SNAPSHOT_ITEMS
    ? `Selected has ${selectedCount.value} items, above the 500-item exact snapshot limit.`
    : undefined,
);
const allDownloadTitle = computed(() =>
  activeTab.value === "stories"
    ? "Refreshes and downloads all current Stories; uses additional API requests."
    : `Fetch and download the complete ${activeTab.value === "posts" ? "Posts" : "Reels"} archive; uses API requests.`,
);
const activeGroupBusy = computed(() => {
  const username = preview.value?.profile.username;
  if (!username) return false;
  const conflicts = downloadConflictKeys(username, activeTab.value, "all");
  return (
    jobs.hasActiveConflict(conflicts) ||
    (activeTab.value === "stories" && storiesLoading.value)
  );
});

function thumbUrl(p: Post): string {
  return p.thumbnail_url ?? p.resources.find((r) => r.kind === "photo")?.url ?? p.resources[0]?.url ?? "";
}

function fmt(n?: number): string {
  return n === undefined ? "—" : new Intl.NumberFormat("en", { notation: "compact" }).format(n);
}

function onQueryInput() {
  window.clearTimeout(debounce);
  const seq = requests.autocomplete.begin();
  requests.profile.invalidate();
  requests.reels.invalidate();
  loading.value = false;
  loadingMore.value = false;
  reelsLoading.value = false;
  reelsError.value = null;
  suggestions.value = [];
  suggestOpen.value = false;
  highlight.value = -1;
  const q = query.value.trim();
  if (q.length < 2) {
    return;
  }
  debounce = window.setTimeout(async () => {
    try {
      const found = await searchUsers(q);
      if (!requests.autocomplete.isCurrent(seq) || query.value.trim() !== q) return;
      suggestions.value = found;
    } catch {
      if (!requests.autocomplete.isCurrent(seq) || query.value.trim() !== q) return;
      suggestions.value = [];
    }
    highlight.value = suggestions.value.length > 0 ? 0 : -1;
    suggestOpen.value = true;
  }, 250);
}

function closeSuggestions() {
  window.clearTimeout(debounce);
  requests.autocomplete.invalidate();
  suggestOpen.value = false;
  highlight.value = -1;
}

function moveHighlight(delta: number) {
  if (!suggestOpen.value || suggestions.value.length === 0) return;
  const n = suggestions.value.length;
  highlight.value = (highlight.value + delta + n) % n;
}

function onEnter(e: KeyboardEvent) {
  if (suggestOpen.value && highlight.value >= 0) {
    const u = suggestions.value[highlight.value];
    if (u) {
      e.preventDefault();
      pickSuggestion(u);
    }
  }
}

function pickSuggestion(u: SearchUser) {
  closeSuggestions();
  suggestions.value = [];
  query.value = `@${u.username}`;
  void loadProfile(u.username);
}

async function submit() {
  const raw = query.value.trim();
  if (!raw || loading.value) return;
  closeSuggestions();
  error.value = null;
  const seq = requests.profile.begin();
  loading.value = true;
  try {
    const target = await resolveInput(raw);
    if (!requests.profile.isCurrent(seq)) return;
    if (target.kind === "post") {
      const id = await downloadPost(target.code);
      jobs.addPlaceholder(id, `Post ${target.code}`);
    } else {
      await loadProfile(target.username);
    }
  } catch (e) {
    if (!requests.profile.isCurrent(seq)) return;
    error.value = String(e);
  } finally {
    if (requests.profile.isCurrent(seq)) {
      loading.value = false;
    }
  }
}

async function loadProfile(username: string) {
  const seq = requests.profile.begin();
  profileSession = Symbol("explorer-profile-session");
  requests.reels.invalidate();
  loading.value = true;
  loadingMore.value = false;
  reelsLoading.value = false;
  reelsError.value = null;
  reelsRetryCursor.value = null;
  error.value = null;
  explorer.beginProfileLoad();
  modalPost.value = null;
  modalStory.value = null;
  try {
    const result = await fetchProfile(username, null);
    if (!requests.profile.isCurrent(seq)) return;
    explorer.commitProfile(result);
    if (!result.profile.is_private) {
      void loadStories();
    }
  } catch (e) {
    if (!requests.profile.isCurrent(seq)) return;
    error.value = String(e);
  } finally {
    if (requests.profile.isCurrent(seq)) {
      loading.value = false;
    }
  }
}

async function loadMore() {
  const cursor = preview.value?.end_cursor;
  const username = preview.value?.profile.username;
  if (!preview.value || !username || !cursor || loadingMore.value) return;
  const seq = requests.profile.snapshot();
  loadingMore.value = true;
  error.value = null;
  try {
    const more = await fetchProfile(username, cursor);
    if (
      !requests.profile.isCurrent(seq) ||
      preview.value?.profile.username !== username ||
      more.profile.username !== username
    ) {
      return;
    }
    explorer.commitMorePosts(username, more);
  } catch (e) {
    if (requests.profile.isCurrent(seq) && preview.value?.profile.username === username) {
      error.value = String(e);
    }
  } finally {
    if (requests.profile.isCurrent(seq) && preview.value?.profile.username === username) {
      loadingMore.value = false;
    }
  }
}

function actionKey(kind: string, username: string) {
  return `${kind}:${username.toLowerCase()}`;
}

function isActionBusy(kind: string, username: string) {
  return activeActions.has(actionKey(kind, username));
}

function downloadFolderKey(username: string, tab: ExploreTab) {
  const folder = tab === "stories" ? "stories" : "posts";
  return `folder:${username.toLowerCase()}:${folder}`;
}

function downloadConflictKeys(username: string, tab: ExploreTab, scope: "all" | "snapshot") {
  const folderKey = downloadFolderKey(username, tab);
  return scope === "all" ? [`profile:${username.toLowerCase()}`, folderKey] : [folderKey];
}

async function runWithDownloadConflicts<T>(
  conflictKeys: readonly string[],
  blockedByKeys: readonly string[],
  action: (reservationToken: symbol) => Promise<T>,
): Promise<T | undefined> {
  if (jobs.hasActiveConflict(blockedByKeys)) return undefined;
  const reservationToken = Symbol("explore-download-enqueue");
  if (!jobs.reserveConflictKeys(reservationToken, conflictKeys)) return undefined;
  try {
    return await action(reservationToken);
  } finally {
    jobs.releaseConflictKeys(reservationToken);
  }
}

function isCurrentProfileSession(session: symbol, username: string, profilePk: string) {
  return (
    profileSession === session &&
    preview.value?.profile.username === username &&
    preview.value.profile.pk === profilePk
  );
}

async function downloadAll() {
  const profile = preview.value?.profile;
  if (!profile) return;
  const tab = activeTab.value;
  if (tab === "stories" && storiesLoading.value) return;
  const session = profileSession;
  const conflictKeys = downloadConflictKeys(profile.username, tab, "all");
  await runWithDownloadConflicts(conflictKeys, conflictKeys, async (reservationToken) => {
    error.value = null;
    try {
      const id = await enqueueProfileDownload(profile.username, {
        posts: tab === "posts",
        reels: tab === "reels",
        stories: tab === "stories",
        highlights: false,
        avatar: false,
        max_posts: null,
      });
      jobs.transferConflictReservation(
        reservationToken,
        id,
        `@${profile.username} ${tab} · all`,
        conflictKeys,
      );
    } catch (e) {
      if (isCurrentProfileSession(session, profile.username, profile.pk)) {
        error.value = String(e);
      }
    }
  });
}

async function downloadSnapshot(scope: "shown" | "selected") {
  const profile = preview.value?.profile;
  if (!profile) return;
  const tab = activeTab.value;
  if (tab === "stories" && storiesLoading.value) return;
  const session = profileSession;
  const selectedEntries = scope === "selected" ? explorer.selectionSnapshot(tab) : [];
  const requestedCount = scope === "shown" ? shownCount.value : selectedEntries.length;
  if (requestedCount > MAX_EXACT_SNAPSHOT_ITEMS) {
    error.value = `${scope === "shown" ? "Shown" : "Selected"} snapshots are limited to 500 items. Use All for a complete archive.`;
    return;
  }
  const selectedEntriesById = new Map(selectedEntries.map((entry) => [entry.pk, entry]));
  const postSnapshot =
    tab === "stories"
      ? []
      : [...gridPosts.value].filter(
          (item) => scope === "shown" || selectedEntriesById.has(item.pk),
        );
  const storySnapshot =
    tab === "stories"
      ? [...(stories.value ?? [])].filter(
          (item) => scope === "shown" || selectedEntriesById.has(item.pk),
        )
      : [];
  const submittedIds = (tab === "stories" ? storySnapshot : postSnapshot).map((item) => item.pk);
  const submittedSelections = submittedIds
    .map((pk) => selectedEntriesById.get(pk))
    .filter((entry) => entry !== undefined);
  if (submittedIds.length === 0) return;

  const conflictKeys = downloadConflictKeys(profile.username, tab, "snapshot");
  const groupConflictKeys = downloadConflictKeys(profile.username, tab, "all");
  await runWithDownloadConflicts(
    conflictKeys,
    groupConflictKeys,
    async (reservationToken) => {
      error.value = null;
      try {
        const id =
          tab === "stories"
            ? await downloadDirect(
                profile.username,
                "stories",
                storySnapshot.map((item) => ({
                  url: item.media_url,
                  pk: item.pk,
                  taken_at: item.taken_at,
                })),
              )
            : await enqueueFetchedPostDownload(
                profile.username,
                tab,
                scope,
                postSnapshot,
              );
        jobs.transferConflictReservation(
          reservationToken,
          id,
          `@${profile.username} ${tab} · ${scope} · ${submittedIds.length}`,
          conflictKeys,
        );
        if (
          scope === "selected" &&
          isCurrentProfileSession(session, profile.username, profile.pk)
        ) {
          explorer.clearSubmitted(tab, submittedSelections);
        }
      } catch (e) {
        if (isCurrentProfileSession(session, profile.username, profile.pk)) {
          error.value = String(e);
        }
      }
    },
  );
}

async function loadReels(cursor: string | null) {
  const userId = preview.value?.profile.pk;
  if (!userId || reelsLoading.value) return;
  const seq = requests.reels.begin();
  reelsLoading.value = true;
  reelsError.value = null;
  reelsRetryCursor.value = cursor;
  try {
    const page = await fetchReels(userId, cursor);
    if (!requests.reels.isCurrent(seq) || preview.value?.profile.pk !== userId) return;
    explorer.commitReelsPage(userId, page.posts, cursor, page.end_cursor);
    reelsRetryCursor.value = null;
  } catch (e) {
    if (!requests.reels.isCurrent(seq) || preview.value?.profile.pk !== userId) return;
    reelsError.value = String(e);
  } finally {
    if (requests.reels.isCurrent(seq) && preview.value?.profile.pk === userId) {
      reelsLoading.value = false;
    }
  }
}

async function selectTab(tab: ExploreTab) {
  activeTab.value = tab;
  if (tab === "reels" && !reelsLoaded.value) {
    await loadReels(null);
  }
}

async function downloadAvatar() {
  const profile = preview.value?.profile;
  if (!profile?.avatar_url) return;
  await runOnce(activeActions, actionKey("avatar", profile.username), async () => {
    try {
      const id = await downloadDirect(profile.username, "propic", [
        { url: profile.avatar_url!, pk: profile.pk },
      ]);
      jobs.addPlaceholder(id, `@${profile.username} avatar`);
    } catch (e) {
      if (preview.value?.profile.username === profile.username) error.value = String(e);
    }
  });
}

async function loadStories() {
  const profile = preview.value?.profile;
  if (!profile) return;
  const { pk: userId, username } = profile;
  const token = explorer.beginStoriesRequest(username);
  if (token === null) return;
  try {
    const items = await fetchStories(userId);
    explorer.commitStories(username, token, items);
  } catch (e) {
    explorer.failStories(username, token, String(e));
  }
}

function closeModal() {
  modalPost.value = null;
  modalStory.value = null;
}

onMounted(() => {
  if (preview.value) {
    if (
      !preview.value.profile.is_private &&
      stories.value === null &&
      storiesError.value === null &&
      !storiesLoading.value
    ) {
      void loadStories();
    }
    if (activeTab.value === "reels" && !reelsLoaded.value) {
      void loadReels(null);
    }
    return;
  }
  if (new URLSearchParams(window.location.search).get("demo") === "explore") {
    query.value = "@natgeo";
    void loadProfile("natgeo");
  }
});

onUnmounted(() => {
  profileSession = Symbol("explorer-profile-session");
  window.clearTimeout(debounce);
  requests.autocomplete.invalidate();
  requests.profile.invalidate();
  requests.reels.invalidate();
});
</script>

<template>
  <div class="mx-auto max-w-3xl space-y-5 p-6">
    <!-- Search -->
    <div class="relative">
      <form class="flex gap-2" @submit.prevent="submit">
        <input
          v-model="query"
          class="input"
          placeholder="@username or instagram.com/p/… link"
          spellcheck="false"
          autocomplete="off"
          @input="onQueryInput"
          @keydown.down.prevent="moveHighlight(1)"
          @keydown.up.prevent="moveHighlight(-1)"
          @keydown.enter="onEnter"
          @keydown.escape="closeSuggestions"
          @blur="closeSuggestions"
        />
        <button class="btn-primary shrink-0" type="submit" :disabled="loading || !query.trim()">Fetch</button>
      </form>

      <div
        v-if="suggestOpen"
        class="card absolute inset-x-0 top-full z-20 mt-1 max-h-72 divide-y divide-line overflow-y-auto py-1"
      >
        <div v-if="suggestions.length === 0" class="px-3 py-2 text-sm text-slate-500">No results</div>
        <button
          v-for="(u, i) in suggestions"
          :key="u.pk"
          type="button"
          class="flex w-full items-center gap-2.5 px-3 py-2 text-left"
          :class="i === highlight ? 'bg-surface-3' : 'hover:bg-surface-2'"
          @mousedown.prevent="pickSuggestion(u)"
          @mousemove="highlight = i"
        >
          <img
            v-if="u.avatar_url"
            :src="u.avatar_url"
            class="h-6 w-6 shrink-0 rounded-full object-cover"
            referrerpolicy="no-referrer"
          />
          <span v-else class="h-6 w-6 shrink-0 rounded-full bg-surface-3"></span>
          <span class="font-semibold text-slate-100">{{ u.username }}</span>
          <span v-if="u.is_verified" class="text-xs text-sky-400" title="Verified">✔</span>
          <span class="truncate text-sm text-slate-500">{{ u.full_name }}</span>
          <span v-if="u.is_private" class="ml-auto shrink-0 text-xs text-slate-500">private</span>
        </button>
      </div>
    </div>

    <p v-if="error" class="rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-sm text-err">{{ error }}</p>

    <!-- Active jobs -->
    <div v-if="loading" class="animate-pulse py-16 text-center text-sm text-slate-500">Loading profile…</div>

    <template v-if="preview">
      <!-- Profile header -->
      <div class="card p-5">
        <div class="flex items-center gap-4">
          <img
            v-if="preview.profile.avatar_url"
            :src="preview.profile.avatar_url"
            class="h-16 w-16 shrink-0 rounded-full border border-line object-cover"
            referrerpolicy="no-referrer"
          />
          <span v-else class="h-16 w-16 shrink-0 rounded-full bg-surface-3"></span>
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-1.5">
              <span class="truncate text-lg font-semibold text-slate-100">{{ preview.profile.username }}</span>
              <span v-if="preview.profile.is_verified" class="text-sm text-sky-400" title="Verified">✔</span>
            </div>
            <p class="truncate text-sm text-slate-400">{{ preview.profile.full_name || "\u00A0" }}</p>
            <p class="mt-0.5 text-xs tabular-nums text-slate-500">
              {{ fmt(preview.profile.media_count) }} posts · {{ fmt(preview.profile.follower_count) }} followers
            </p>
          </div>
          <div class="flex shrink-0 items-center gap-2">
            <button
              v-if="preview.profile.avatar_url"
              class="inline-flex items-center gap-1 rounded-lg border border-line bg-surface-3 px-2.5 py-1.5 text-xs text-slate-300 transition-colors hover:bg-line"
              title="Download profile picture"
              :disabled="isActionBusy('avatar', preview.profile.username)"
              @click="downloadAvatar"
            >
              ↓ Avatar
            </button>
          </div>
        </div>
        <p v-if="preview.profile.is_private" class="mt-3 text-sm text-slate-500">
          Private profile — only the avatar is accessible
        </p>
      </div>

      <template v-if="!preview.profile.is_private">
        <!-- Tabs -->
        <div class="flex items-center gap-1">
          <button
            v-for="t in tabs"
            :key="t.id"
            type="button"
            class="rounded-lg px-3 py-1.5 text-sm transition-colors"
            :class="
              activeTab === t.id
                ? 'bg-surface-3 text-slate-100'
                : 'text-slate-400 hover:bg-surface-2 hover:text-slate-200'
            "
            @click="selectTab(t.id)"
          >
            {{ t.label }}
          </button>
          <DownloadScopeGroup
            class="ml-auto"
            :shown-count="shownCount"
            :selected-count="selectedCount"
            :busy="activeGroupBusy"
            :all-title="allDownloadTitle"
            :helper-text="snapshotHelperText"
            :shown-disabled-reason="shownDisabledReason"
            :selected-disabled-reason="selectedDisabledReason"
            @download-all="downloadAll"
            @download-shown="downloadSnapshot('shown')"
            @download-selected="downloadSnapshot('selected')"
          />
        </div>

        <!-- Posts / Reels grid -->
        <div v-if="activeTab !== 'stories'" class="space-y-3">
          <div
            v-if="activeTab === 'reels' && reelsLoading && reels.length === 0"
            class="animate-pulse py-12 text-center text-sm text-slate-500"
          >
            Loading reels…
          </div>
          <div
            v-if="activeTab === 'reels' && reelsError"
            class="card flex items-center justify-between gap-3 px-3 py-2 text-sm"
          >
            <span class="text-err">{{ reelsError }}</span>
            <button
              type="button"
              class="btn-secondary shrink-0"
              :disabled="reelsLoading"
              @click="loadReels(reelsRetryCursor)"
            >
              Retry reels
            </button>
          </div>
          <div
            v-if="gridPosts.length > 0 && !(activeTab === 'reels' && reelsLoading && reels.length === 0)"
            class="grid grid-cols-2 gap-2 sm:grid-cols-3"
          >
            <div
              v-for="p in gridPosts"
              :key="p.pk"
              :data-media-id="p.pk"
              class="relative aspect-square rounded-lg bg-surface-2"
              :class="
                selectedIdSet.has(p.pk)
                  ? 'ring-2 ring-accent ring-offset-2 ring-offset-surface-0'
                  : ''
              "
            >
              <button
                type="button"
                data-action="preview"
                class="absolute inset-0 cursor-pointer overflow-hidden rounded-lg transition hover:brightness-110"
                :aria-label="`Preview ${activeTab === 'reels' ? 'reel' : 'post'} ${p.code}`"
                @click="modalPost = p"
              >
                <img
                  v-if="thumbUrl(p)"
                  :src="thumbUrl(p)"
                  class="h-full w-full object-cover"
                  referrerpolicy="no-referrer"
                  loading="lazy"
                />
                <span v-else class="block h-full w-full bg-gradient-to-br from-surface-2 to-surface-3"></span>
                <span
                  v-if="activeTab === 'reels' && hasVideo(p)"
                  class="absolute bottom-1.5 right-1.5 rounded-md bg-black/60 px-1.5 py-0.5 text-[10px] leading-none text-white"
                  >▶</span
                >
              </button>
              <MediaSelectionCheckbox
                :selected="selectedIdSet.has(p.pk)"
                :label="`Select ${activeTab === 'reels' ? 'reel' : 'post'} ${p.code}`"
                @toggle="explorer.toggleSelected(activeTab, p.pk)"
              />
            </div>
          </div>
          <div
            v-else-if="activeTab === 'posts' || (activeTab === 'reels' && reelsLoaded && !reelsError)"
            class="card flex items-center justify-center p-12 text-sm text-slate-500"
          >
            {{ activeTab === "reels" ? "No reels yet." : "No posts yet." }}
          </div>
          <div v-if="activeTab === 'posts' && preview.end_cursor" class="flex justify-center">
            <button class="btn-secondary" :disabled="loadingMore" @click="loadMore">
              {{ loadingMore ? "Loading…" : "Load more" }}
            </button>
          </div>
          <div v-if="activeTab === 'reels' && reelsCursor" class="flex justify-center">
            <button class="btn-secondary" :disabled="reelsLoading" @click="loadReels(reelsCursor)">
              {{ reelsLoading ? "Loading…" : "Load more" }}
            </button>
          </div>
        </div>

        <!-- Stories -->
        <div v-else class="space-y-3">
          <div
            v-if="storiesError"
            class="card flex items-center justify-between gap-3 px-3 py-2 text-sm"
          >
            <span class="text-err">{{ storiesError }}</span>
            <button
              type="button"
              class="btn-secondary shrink-0"
              :disabled="storiesLoading"
              @click="loadStories"
            >
              Retry stories
            </button>
          </div>
          <div
            v-if="stories === null && !storiesError"
            class="animate-pulse py-12 text-center text-sm text-slate-500"
          >
            Loading stories…
          </div>
          <div v-else-if="stories && stories.length > 0" class="flex gap-3 overflow-x-auto py-1">
            <div
              v-for="s in stories"
              :key="s.pk"
              :data-story-id="s.pk"
              class="relative shrink-0 rounded-full border-2 border-[var(--color-accent)] p-0.5"
              :class="
                selectedIdSet.has(s.pk)
                  ? 'ring-2 ring-accent ring-offset-2 ring-offset-surface-0'
                  : ''
              "
            >
              <button
                type="button"
                data-action="preview"
                class="block cursor-pointer rounded-full transition hover:brightness-110"
                :aria-label="`Preview story ${s.pk}`"
                @click="modalStory = s"
              >
                <img
                  v-if="s.thumb_url || s.media_url"
                  :src="s.thumb_url || s.media_url"
                  class="h-20 w-20 rounded-full object-cover"
                  referrerpolicy="no-referrer"
                />
                <span v-else class="block h-20 w-20 rounded-full bg-gradient-to-br from-surface-2 to-surface-3"></span>
              </button>
              <MediaSelectionCheckbox
                :selected="selectedIdSet.has(s.pk)"
                :label="`Select story ${s.pk}`"
                @toggle="explorer.toggleSelected('stories', s.pk)"
              />
            </div>
          </div>
          <div
            v-else-if="stories"
            class="card flex items-center justify-center p-12 text-sm text-slate-500"
          >
            No active stories.
          </div>
        </div>
      </template>
    </template>

    <div
      v-if="!preview && !loading && !error"
      class="card flex items-center justify-center p-12 text-sm text-slate-500"
    >
      Search a username or paste a post link to explore.
    </div>

    <PostModal
      v-if="modalPost || modalStory"
      :username="preview?.profile.username ?? ''"
      :post="modalPost"
      :story="modalStory"
      @close="closeModal"
    />
  </div>
</template>
