<script setup lang="ts">
import { storeToRefs } from "pinia";
import { computed, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { useRoute, useRouter, type LocationQueryValue } from "vue-router";
import {
  checkDownloadStatuses,
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
  type FetchedPostCategory,
  type DownloadStatus,
  type DownloadStatusRequest,
} from "../lib/ipc";
import { useExplorerStore, type ExploreTab, type PostFilter } from "../stores/explorer";
import { useJobsStore } from "../stores/jobs";
import { createExplorerRequestState, runOnce } from "../lib/asyncState";
import DownloadScopeGroup from "../components/DownloadScopeGroup.vue";
import MediaSelectionCheckbox from "../components/MediaSelectionCheckbox.vue";
import MediaTypeBadge from "../components/MediaTypeBadge.vue";
import PostModal from "../components/PostModal.vue";
import RemoteImage from "../components/RemoteImage.vue";
import { classifyPost } from "../lib/postDisplay";

const jobs = useJobsStore();
const explorer = useExplorerStore();
const route = useRoute();
const router = useRouter();
const {
  query,
  profilePreview: preview,
  activeTab,
  postFilter,
  postsPage,
  postsPageSize,
  reels,
  reelsCursor,
  reelsLoaded,
  reelsPage,
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
const modalPostCategory = ref<FetchedPostCategory>("posts");

const requests = createExplorerRequestState();
const activeActions = reactive(new Set<string>());
let profileSession = Symbol("explorer-profile-session");
const MAX_EXACT_SNAPSHOT_ITEMS = 500;
const DOWNLOAD_STATUS_CHUNK_SIZE = 500;

interface DownloadStatusController {
  generation: number;
  running: boolean;
  dirty: boolean;
}

const downloadStatuses = reactive<Record<ExploreTab, Map<string, DownloadStatus>>>({
  posts: new Map(),
  reels: new Map(),
  stories: new Map(),
});
const downloadStatusControllers: Record<ExploreTab, DownloadStatusController> = {
  posts: { generation: 0, running: false, dirty: false },
  reels: { generation: 0, running: false, dirty: false },
  stories: { generation: 0, running: false, dirty: false },
};
let downloadStatusesMounted = true;

const tabs = [
  { id: "posts", label: "Posts" },
  { id: "reels", label: "Reels" },
  { id: "stories", label: "Stories" },
] as const;

const postFilters: ReadonlyArray<{ id: PostFilter; label: string }> = [
  { id: "all", label: "All" },
  { id: "photos", label: "Photos" },
  { id: "videos", label: "Videos" },
  { id: "carousels", label: "Carousels" },
];

const sourcePosts = computed(() => preview.value?.recent_posts ?? []);
const gridPosts = computed(() => {
  if (activeTab.value === "reels") return reels.value;
  if (activeTab.value !== "posts" || postFilter.value === "all") return sourcePosts.value;
  const kind = postFilter.value === "photos"
    ? "photo"
    : postFilter.value === "videos"
      ? "video"
      : "carousel";
  return sourcePosts.value.filter((post) => classifyPost(post).kind === kind);
});
const postsEmptyMessage = computed(() => {
  if (postFilter.value === "all") return "No posts yet.";
  const filterLabel = postFilters.find((filter) => filter.id === postFilter.value)?.label.toLowerCase();
  const loadedCount = sourcePosts.value.length;
  return `No ${filterLabel ?? "matching posts"} in ${loadedCount} loaded ${loadedCount === 1 ? "post" : "posts"}.`;
});
function isDownloadablePost(post: Post): boolean {
  return (
    Array.isArray(post.resources) &&
    post.resources.length > 0 &&
    post.resources.every((resource) => resource.kind === "photo" || resource.kind === "video")
  );
}

function postStatusRequests(posts: readonly Post[]): DownloadStatusRequest[] {
  const requestsByPk = new Map<string, DownloadStatusRequest>();
  for (const post of posts) {
    if (!isDownloadablePost(post) || requestsByPk.has(post.pk)) continue;
    requestsByPk.set(post.pk, {
      namespace: "post",
      pk: post.pk,
      resources: post.resources.map((resource) => resource.kind as "photo" | "video"),
    });
  }
  return [...requestsByPk.values()];
}

function storyStatusRequests(items: readonly StoryItem[]): DownloadStatusRequest[] {
  const requestsByPk = new Map<string, DownloadStatusRequest>();
  for (const item of items) {
    if (requestsByPk.has(item.pk)) continue;
    requestsByPk.set(item.pk, {
      namespace: "story",
      pk: item.pk,
      resources: [item.kind],
    });
  }
  return [...requestsByPk.values()];
}

function allStatusRequests(tab: ExploreTab): DownloadStatusRequest[] {
  if (tab === "posts") return postStatusRequests(sourcePosts.value);
  if (tab === "reels") return postStatusRequests(reels.value);
  return storyStatusRequests(stories.value ?? []);
}

function invalidateDownloadStatuses(clear: boolean) {
  for (const tab of tabs.map((item) => item.id)) {
    const controller = downloadStatusControllers[tab];
    controller.generation += 1;
    controller.dirty = false;
    if (clear) downloadStatuses[tab] = new Map();
  }
}

function applyDownloadStatusResults(
  tab: ExploreTab,
  mode: "merge" | "full",
  results: readonly DownloadStatus[],
) {
  const next = mode === "full" ? new Map<string, DownloadStatus>() : new Map(downloadStatuses[tab]);
  for (const result of results) {
    if (result.state === "not_downloaded") next.delete(result.pk);
    else next.set(result.pk, result);
  }
  downloadStatuses[tab] = next;
}

async function refreshDownloadStatuses(
  tab: ExploreTab,
  mode: "merge" | "full",
  requestedItems: readonly DownloadStatusRequest[] = allStatusRequests(tab),
) {
  const profile = preview.value?.profile;
  if (!downloadStatusesMounted || !profile) return;
  const controller = downloadStatusControllers[tab];
  if (controller.running) {
    controller.generation += 1;
    controller.dirty = true;
    return;
  }

  const items = [...requestedItems];
  if (items.length === 0) {
    if (mode === "full") downloadStatuses[tab] = new Map();
    return;
  }

  const generation = ++controller.generation;
  const session = profileSession;
  const profilePk = profile.pk;
  controller.running = true;
  try {
    const results: DownloadStatus[] = [];
    for (let start = 0; start < items.length; start += DOWNLOAD_STATUS_CHUNK_SIZE) {
      const chunk = items.slice(start, start + DOWNLOAD_STATUS_CHUNK_SIZE);
      results.push(...await checkDownloadStatuses(chunk));
    }
    if (
      downloadStatusesMounted &&
      profileSession === session &&
      preview.value?.profile.pk === profilePk &&
      controller.generation === generation
    ) {
      applyDownloadStatusResults(tab, mode, results);
    }
  } catch {
    // Status is ancillary. Keep the last verified badge and the main Explore flow usable.
  } finally {
    controller.running = false;
    if (controller.dirty && downloadStatusesMounted && preview.value) {
      controller.dirty = false;
      void refreshDownloadStatuses(tab, "full");
    }
  }
}

function downloadStatus(tab: ExploreTab, pk: string): DownloadStatus | undefined {
  return downloadStatuses[tab].get(pk);
}

function downloadStatusLabel(status: DownloadStatus): string {
  return status.state === "downloaded"
    ? "Downloaded"
    : `Partial ${status.available_resources}/${status.expected_resources}`;
}

function mediaDownloadStatusLabel(tab: ExploreTab, pk: string): string | null {
  const status = downloadStatus(tab, pk);
  return status ? downloadStatusLabel(status) : null;
}

function mediaDownloadStatusClass(tab: ExploreTab, pk: string): string {
  return downloadStatus(tab, pk)?.state === "partial"
    ? "bg-amber-950/90 text-amber-300"
    : "bg-emerald-950/90 text-emerald-300";
}
const downloadableGridPosts = computed(() => gridPosts.value.filter(isDownloadablePost));
const downloadablePostIds = computed(() => {
  const posts = activeTab.value === "reels" ? reels.value : sourcePosts.value;
  return new Set(posts.filter(isDownloadablePost).map((post) => post.pk));
});
const shownCount = computed(() =>
  activeTab.value === "stories"
    ? (stories.value?.length ?? 0)
    : downloadableGridPosts.value.length,
);
const selectedIdSet = computed(() => {
  const selectedIds = explorer.selected[activeTab.value];
  return new Set(
    activeTab.value === "stories"
      ? selectedIds
      : selectedIds.filter((pk) => downloadablePostIds.value.has(pk)),
  );
});
const selectedCount = computed(() => selectedIdSet.value.size);
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
const postsTotalPages = computed(() => {
  const totalItems = preview.value?.profile.media_count;
  if (totalItems === undefined || postsPageSize.value <= 0) return null;
  return Math.max(postsPage.value, Math.ceil(totalItems / postsPageSize.value));
});
const paginationStatus = computed(() => {
  if (activeTab.value === "posts" && postsPage.value > 0) {
    const total = postsTotalPages.value;
    return total === null ? `Page ${postsPage.value}` : `Page ${postsPage.value} of ${total}`;
  }
  if (activeTab.value === "reels" && reelsPage.value > 0) {
    return `Page ${reelsPage.value}`;
  }
  return null;
});

function previewMediaLabel(post: Post): string {
  const display = classifyPost(post);
  if (display.kind === "carousel") return `carousel with ${display.count} resources`;
  return display.kind === "unknown" ? "post" : display.kind;
}
function togglePostSelection(post: Post) {
  if (isDownloadablePost(post)) explorer.toggleSelected(activeTab.value, post.pk);
}
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

function relationshipPath(kind: "followers" | "following") {
  const username = preview.value?.profile.username ?? "";
  return `/explore/${encodeURIComponent(username)}/${kind}`;
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
  invalidateDownloadStatuses(true);
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
    void refreshDownloadStatuses("posts", "merge", postStatusRequests(result.recent_posts));
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
  const existingPostIds = new Set(sourcePosts.value.map((post) => post.pk));
  try {
    const more = await fetchProfile(username, cursor);
    if (
      !requests.profile.isCurrent(seq) ||
      preview.value?.profile.username !== username ||
      more.profile.username !== username
    ) {
      return;
    }
    if (explorer.commitMorePosts(username, more)) {
      const added = sourcePosts.value.filter((post) => !existingPostIds.has(post.pk));
      void refreshDownloadStatuses("posts", "merge", postStatusRequests(added));
    }
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
  const requestedCount = scope === "shown" ? shownCount.value : selectedCount.value;
  if (requestedCount > MAX_EXACT_SNAPSHOT_ITEMS) {
    error.value = `${scope === "shown" ? "Shown" : "Selected"} snapshots are limited to 500 items. Use All for a complete archive.`;
    return;
  }
  const selectedEntriesById = new Map(selectedEntries.map((entry) => [entry.pk, entry]));
  const postCandidates = scope === "shown"
    ? downloadableGridPosts.value
    : (tab === "reels" ? reels.value : sourcePosts.value).filter(isDownloadablePost);
  const postSnapshot =
    tab === "stories"
      ? []
      : [...postCandidates].filter(
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
        if (scope === "selected") {
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
  const existingReelIds = new Set(reels.value.map((post) => post.pk));
  try {
    const page = await fetchReels(userId, cursor);
    if (!requests.reels.isCurrent(seq) || preview.value?.profile.pk !== userId) return;
    if (explorer.commitReelsPage(userId, page.posts, cursor, page.end_cursor)) {
      const added = reels.value.filter((post) => !existingReelIds.has(post.pk));
      void refreshDownloadStatuses("reels", "merge", postStatusRequests(added));
    }
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
    return;
  }
  void refreshDownloadStatuses(tab, "full");
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

function normalizeRouteProfile(
  value: LocationQueryValue | LocationQueryValue[] | undefined,
): string | null {
  const first = Array.isArray(value) ? value[0] : value;
  return first?.trim().replace(/^@/, "") || null;
}

function applyRouteProfile(username: string): void {
  query.value = `@${username}`;
  if (preview.value?.profile.username.toLowerCase() === username.toLowerCase()) {
    resumeRetainedProfile();
    return;
  }
  void loadProfile(username);
}

async function openProfileFromCaption(username: string): Promise<void> {
  const target = username.trim().replace(/^@/, "");
  closeModal();
  if (!target) return;

  const current = preview.value?.profile.username;
  if (current?.toLowerCase() === target.toLowerCase()) return;

  const currentRouteProfile = normalizeRouteProfile(route.query.profile);
  if (current && currentRouteProfile?.toLowerCase() !== current.toLowerCase()) {
    await router.replace({
      path: "/explore",
      query: { ...route.query, profile: current },
    });
  }
  await router.push({
    path: "/explore",
    query: { ...route.query, profile: target },
  });
}

function openPostModal(post: Post) {
  modalPostCategory.value = activeTab.value === "reels" ? "reels" : "posts";
  modalPost.value = post;
}

function resumeRetainedProfile() {
  if (!preview.value) return;
  const startsStoriesLoad =
    !preview.value.profile.is_private &&
    stories.value === null &&
    storiesError.value === null &&
    !storiesLoading.value;
  if (
    startsStoriesLoad
  ) {
    void loadStories();
  }
  if (activeTab.value === "reels" && !reelsLoaded.value) {
    void loadReels(null);
    return;
  }
  if (activeTab.value === "stories" && startsStoriesLoad) return;
  void refreshDownloadStatuses(activeTab.value, "full");
}

const terminalJobStates = new Set(["done", "failed", "cancelled"]);
const seenTerminalJobIds = new Set(
  [...jobs.jobs.values()]
    .filter((job) => terminalJobStates.has(job.state))
    .map((job) => job.id),
);

watch(stories, (items) => {
  if (items !== null) {
    void refreshDownloadStatuses("stories", "merge", storyStatusRequests(items));
  }
});

watch(
  () => [...jobs.jobs.values()].map((job) => [job.id, job.state] as const),
  (states) => {
    let terminalObserved = false;
    for (const [id, state] of states) {
      if (!terminalJobStates.has(state) || seenTerminalJobIds.has(id)) continue;
      seenTerminalJobIds.add(id);
      terminalObserved = true;
    }
    if (terminalObserved) void refreshDownloadStatuses(activeTab.value, "full");
  },
  { flush: "post" },
);

watch(
  () => route.query.profile,
  (value, previousValue) => {
    const username = normalizeRouteProfile(value);
    const previousUsername = normalizeRouteProfile(previousValue);
    if (!username || username.toLowerCase() === previousUsername?.toLowerCase()) return;
    applyRouteProfile(username);
  },
);

onMounted(() => {
  const searchParams = new URLSearchParams(window.location.search);
  const requestedProfile = normalizeRouteProfile(route.query.profile);
  if (requestedProfile) {
    applyRouteProfile(requestedProfile);
    return;
  }
  if (preview.value) {
    resumeRetainedProfile();
    return;
  }
  if (searchParams.get("demo") === "remote-media-failure") {
    query.value = "@preview_demo";
    void loadProfile("preview_demo");
    return;
  }
  if (searchParams.get("demo") === "explore") {
    query.value = "@natgeo";
    void loadProfile("natgeo");
  }
});

onUnmounted(() => {
  downloadStatusesMounted = false;
  profileSession = Symbol("explorer-profile-session");
  invalidateDownloadStatuses(true);
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
          <RemoteImage
            :source="u.avatar_url"
            alt=""
            variant="compact-avatar"
            class="h-6 w-6 shrink-0 rounded-full"
          />
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
          <RemoteImage
            :source="preview.profile.avatar_url"
            :alt="`@${preview.profile.username} profile picture`"
            variant="avatar"
            class="h-16 w-16 shrink-0 rounded-full border border-line"
          />
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-1.5">
              <span class="truncate text-lg font-semibold text-slate-100">{{ preview.profile.username }}</span>
              <span v-if="preview.profile.is_verified" class="text-sm text-sky-400" title="Verified">✔</span>
            </div>
            <p class="truncate text-sm text-slate-400">{{ preview.profile.full_name || "\u00A0" }}</p>
            <p class="mt-0.5 flex flex-wrap items-center gap-x-1 text-xs tabular-nums text-slate-500">
              <span>{{ fmt(preview.profile.media_count) }} posts</span>
              <span aria-hidden="true">·</span>
              <RouterLink
                v-if="!preview.profile.is_private"
                data-relationship="followers"
                :to="relationshipPath('followers')"
                class="underline decoration-transparent underline-offset-2 hover:text-slate-200 hover:decoration-current"
              >
                {{ fmt(preview.profile.follower_count) }} followers
              </RouterLink>
              <span v-else>{{ fmt(preview.profile.follower_count) }} followers</span>
              <span aria-hidden="true">·</span>
              <RouterLink
                v-if="!preview.profile.is_private"
                data-relationship="following"
                :to="relationshipPath('following')"
                class="underline decoration-transparent underline-offset-2 hover:text-slate-200 hover:decoration-current"
              >
                {{ fmt(preview.profile.following_count) }} following
              </RouterLink>
              <span v-else>{{ fmt(preview.profile.following_count) }} following</span>
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
        <!-- Explore controls -->
        <div class="space-y-2">
          <div data-explorer-toolbar class="flex flex-wrap items-center gap-x-2 gap-y-2">
            <div data-explore-tabs class="flex shrink-0 items-center gap-1">
              <button
                v-for="t in tabs"
                :key="t.id"
                type="button"
                class="rounded-lg px-2.5 py-1.5 text-sm transition-colors"
                :class="
                  activeTab === t.id
                    ? 'bg-surface-3 text-slate-100'
                    : 'text-slate-400 hover:bg-surface-2 hover:text-slate-200'
                "
                @click="selectTab(t.id)"
              >
                {{ t.label }}
              </button>
            </div>
            <DownloadScopeGroup
              class="ml-auto shrink-0"
              :shown-count="shownCount"
              :selected-count="selectedCount"
              :busy="activeGroupBusy"
              :all-title="allDownloadTitle"
              :shown-disabled-reason="shownDisabledReason"
              :selected-disabled-reason="selectedDisabledReason"
              @download-all="downloadAll"
              @download-shown="downloadSnapshot('shown')"
              @download-selected="downloadSnapshot('selected')"
            />
          </div>
          <div
            v-if="activeTab === 'posts'"
            data-post-filter-row
            class="flex"
          >
            <div
              role="group"
              aria-label="Posts filter"
              class="inline-flex shrink-0 overflow-hidden rounded-md border border-line bg-surface-1"
            >
              <button
                v-for="filter in postFilters"
                :key="filter.id"
                type="button"
                :data-post-filter="filter.id"
                :aria-pressed="postFilter === filter.id"
                :aria-current="postFilter === filter.id ? 'true' : undefined"
                class="border-l border-line px-2 py-1 text-xs text-slate-400 transition-colors first:border-l-0"
                :class="
                  postFilter === filter.id
                    ? 'bg-accent/15 text-white shadow-[inset_0_-2px_0_var(--color-accent)]'
                    : 'hover:bg-surface-2 hover:text-slate-200'
                "
                @click="postFilter = filter.id"
              >
                {{ filter.label }}
              </button>
            </div>
          </div>
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
                :aria-label="`Preview ${previewMediaLabel(p)} ${p.code}`"
                @click="openPostModal(p)"
              >
                <RemoteImage
                  :source="thumbUrl(p)"
                  alt=""
                  variant="thumbnail"
                  class="h-full w-full rounded-lg"
                  loading="lazy"
                />
              </button>
              <MediaTypeBadge
                v-bind="classifyPost(p)"
                class="pointer-events-none absolute bottom-2 right-2 z-10"
              />
              <MediaSelectionCheckbox
                :selected="selectedIdSet.has(p.pk)"
                :label="`Select ${activeTab === 'reels' ? 'reel' : 'post'} ${p.code}`"
                :disabled="!isDownloadablePost(p)"
                :disabled-reason="!isDownloadablePost(p) ? 'This post has no downloadable media.' : undefined"
                @toggle="togglePostSelection(p)"
              />
              <span
                v-if="mediaDownloadStatusLabel(activeTab, p.pk)"
                data-download-status
                class="pointer-events-none absolute bottom-2 left-2 z-10 rounded px-1.5 py-0.5 text-[10px] font-semibold shadow-sm"
                :class="mediaDownloadStatusClass(activeTab, p.pk)"
              >
                {{ mediaDownloadStatusLabel(activeTab, p.pk) }}
              </span>
              <span
                v-if="!isDownloadablePost(p)"
                data-download-unavailable
                class="pointer-events-none absolute bottom-2 left-2 z-10 rounded bg-slate-900 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-slate-300"
                title="This post has no downloadable media."
              >
                Unavailable
              </span>
            </div>
          </div>
          <div
            v-else-if="activeTab === 'posts' || (activeTab === 'reels' && reelsLoaded && !reelsError)"
            class="card flex items-center justify-center p-12 text-sm text-slate-500"
          >
            {{
              activeTab === "reels" ? "No reels yet." : postsEmptyMessage
            }}
          </div>
          <div
            v-if="paginationStatus"
            class="flex flex-wrap items-center justify-center gap-3"
          >
            <span
              data-pagination-status
              class="text-xs tabular-nums text-slate-500"
              aria-live="polite"
            >{{ paginationStatus }}</span>
            <button
              v-if="activeTab === 'posts' && preview.end_cursor"
              class="btn-secondary"
              :disabled="loadingMore"
              @click="loadMore"
            >
              {{ loadingMore ? "Loading…" : "Load more" }}
            </button>
            <button
              v-else-if="activeTab === 'reels' && reelsCursor"
              class="btn-secondary"
              :disabled="reelsLoading"
              @click="loadReels(reelsCursor)"
            >
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
                <RemoteImage
                  :source="s.thumb_url || s.media_url"
                  alt=""
                  variant="story"
                  class="h-20 w-20 rounded-full"
                />
              </button>
              <MediaSelectionCheckbox
                :selected="selectedIdSet.has(s.pk)"
                :label="`Select story ${s.pk}`"
                @toggle="explorer.toggleSelected('stories', s.pk)"
              />
              <span
                v-if="mediaDownloadStatusLabel('stories', s.pk)"
                data-download-status
                class="pointer-events-none absolute bottom-0 left-0 z-10 rounded px-1.5 py-0.5 text-[10px] font-semibold shadow-sm"
                :class="mediaDownloadStatusClass('stories', s.pk)"
              >
                {{ mediaDownloadStatusLabel("stories", s.pk) }}
              </span>
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
      :post-category="modalPostCategory"
      :story="modalStory"
      @close="closeModal"
      @open-profile="openProfileFromCaption"
    />
  </div>
</template>
