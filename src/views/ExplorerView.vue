<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from "vue";
import {
  downloadDirect,
  downloadPost,
  enqueueProfileDownload,
  fetchProfile,
  fetchStories,
  resolveInput,
  searchUsers,
  type Post,
  type ProfilePreview,
  type SearchUser,
  type StoryItem,
} from "../lib/ipc";
import { useJobsStore } from "../stores/jobs";
import { createExplorerRequestState, runOnce } from "../lib/asyncState";
import { mergeUniquePosts } from "../lib/mediaPages";
import JobCard from "../components/JobCard.vue";
import PostModal from "../components/PostModal.vue";

const jobs = useJobsStore();

const query = ref("");
const suggestions = ref<SearchUser[]>([]);
const suggestOpen = ref(false);
const highlight = ref(-1);
let debounce = 0;

const preview = ref<ProfilePreview | null>(null);
const loading = ref(false);
const loadingMore = ref(false);
const error = ref<string | null>(null);

const activeTab = ref<"posts" | "reels" | "stories">("posts");
const stories = ref<StoryItem[] | null>(null);
const storiesLoading = ref(false);

const modalPost = ref<Post | null>(null);
const modalStory = ref<StoryItem | null>(null);

const requests = createExplorerRequestState();
const activeActions = reactive(new Set<string>());

const tabs = [
  { id: "posts", label: "Posts" },
  { id: "reels", label: "Reels" },
  { id: "stories", label: "Stories" },
] as const;

const activeJobs = computed(() =>
  [...jobs.jobs.values()].filter((j) => j.state === "fetching" || j.state === "downloading"),
);

function hasVideo(p: Post): boolean {
  return p.resources.some((r) => r.kind === "video");
}

const reels = computed(() => preview.value?.recent_posts.filter(hasVideo) ?? []);
const gridPosts = computed(() => (activeTab.value === "reels" ? reels.value : (preview.value?.recent_posts ?? [])));

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
  requests.stories.invalidate();
  loading.value = false;
  loadingMore.value = false;
  storiesLoading.value = false;
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
  requests.stories.invalidate();
  loading.value = true;
  loadingMore.value = false;
  storiesLoading.value = false;
  error.value = null;
  preview.value = null;
  stories.value = null;
  modalPost.value = null;
  modalStory.value = null;
  activeTab.value = "posts";
  try {
    const result = await fetchProfile(username, null);
    if (!requests.profile.isCurrent(seq)) return;
    preview.value = result;
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
    preview.value = {
      profile: more.profile,
      recent_posts: mergeUniquePosts(preview.value.recent_posts, more.recent_posts),
      end_cursor: more.end_cursor,
    };
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

async function downloadAll(kind: "posts" | "reels") {
  if (!preview.value) return;
  const username = preview.value.profile.username;
  await runOnce(activeActions, actionKey(kind, username), async () => {
    try {
      const id = await enqueueProfileDownload(username, {
        posts: kind === "posts",
        reels: kind === "reels",
        stories: false,
        highlights: false,
        avatar: false,
        max_posts: null,
      });
      jobs.addPlaceholder(id, `@${username} ${kind}`);
    } catch (e) {
      if (preview.value?.profile.username === username) error.value = String(e);
    }
  });
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
  const username = preview.value?.profile.username;
  if (!username || storiesLoading.value) return;
  const seq = requests.stories.begin();
  storiesLoading.value = true;
  error.value = null;
  try {
    const items = await fetchStories(username);
    if (!requests.stories.isCurrent(seq) || preview.value?.profile.username !== username) return;
    stories.value = items;
  } catch (e) {
    if (!requests.stories.isCurrent(seq) || preview.value?.profile.username !== username) return;
    error.value = String(e);
  } finally {
    if (requests.stories.isCurrent(seq) && preview.value?.profile.username === username) {
      storiesLoading.value = false;
    }
  }
}

async function downloadAllStories() {
  if (!preview.value || !stories.value || stories.value.length === 0) return;
  const username = preview.value.profile.username;
  const items = stories.value.map((s) => ({
    url: s.media_url,
    pk: s.pk,
    taken_at: s.taken_at,
  }));
  await runOnce(activeActions, actionKey("stories", username), async () => {
    try {
      const id = await downloadDirect(username, "stories", items);
      jobs.addPlaceholder(id, `@${username} stories`);
    } catch (e) {
      if (preview.value?.profile.username === username) error.value = String(e);
    }
  });
}

function closeModal() {
  modalPost.value = null;
  modalStory.value = null;
}

onMounted(() => {
  if (new URLSearchParams(window.location.search).get("demo") === "explore") {
    query.value = "@natgeo";
    void loadProfile("natgeo");
  }
});

onUnmounted(() => {
  window.clearTimeout(debounce);
  requests.autocomplete.invalidate();
  requests.profile.invalidate();
  requests.stories.invalidate();
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
    <div v-if="activeJobs.length > 0" class="space-y-3">
      <JobCard v-for="job in activeJobs" :key="job.id" :job="job" />
    </div>

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
            <button
              v-if="!preview.profile.is_private"
              class="btn-primary"
              :disabled="isActionBusy('posts', preview.profile.username)"
              @click="downloadAll('posts')"
            >
              Download all posts
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
            @click="activeTab = t.id"
          >
            {{ t.label }}
          </button>
          <div class="ml-auto">
            <button
              v-if="activeTab === 'posts'"
              class="btn-secondary"
              :disabled="isActionBusy('posts', preview.profile.username)"
              @click="downloadAll('posts')"
            >Download all</button>
            <button
              v-else-if="activeTab === 'reels'"
              class="btn-secondary"
              :disabled="isActionBusy('reels', preview.profile.username)"
              @click="downloadAll('reels')"
            >
              Download all
            </button>
            <button
              v-else-if="stories && stories.length > 0"
              class="btn-secondary"
              :disabled="isActionBusy('stories', preview.profile.username)"
              @click="downloadAllStories"
            >
              Download all stories
            </button>
          </div>
        </div>

        <!-- Posts / Reels grid -->
        <div v-if="activeTab !== 'stories'" class="space-y-3">
          <div v-if="gridPosts.length > 0" class="grid grid-cols-2 gap-2 sm:grid-cols-3">
            <button
              v-for="p in gridPosts"
              :key="p.pk"
              type="button"
              class="relative aspect-square cursor-pointer overflow-hidden rounded-lg bg-surface-2 transition hover:brightness-110"
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
          </div>
          <div v-else class="card flex items-center justify-center p-12 text-sm text-slate-500">
            {{ activeTab === "reels" ? "No reels yet." : "No posts yet." }}
          </div>
          <div v-if="activeTab === 'posts' && preview.end_cursor" class="flex justify-center">
            <button class="btn-secondary" :disabled="loadingMore" @click="loadMore">
              {{ loadingMore ? "Loading…" : "Load more" }}
            </button>
          </div>
        </div>

        <!-- Stories -->
        <div v-else class="space-y-3">
          <div v-if="stories === null" class="flex justify-center py-12">
            <button class="btn-secondary" :disabled="storiesLoading" @click="loadStories">
              {{ storiesLoading ? "Loading…" : "Load stories · costs 2 requests" }}
            </button>
          </div>
          <div v-else-if="stories.length > 0" class="flex gap-3 overflow-x-auto py-1">
            <button
              v-for="s in stories"
              :key="s.pk"
              type="button"
              class="shrink-0 cursor-pointer rounded-full border-2 border-[var(--color-accent)] p-0.5"
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
          </div>
          <div v-else class="card flex items-center justify-center p-12 text-sm text-slate-500">
            No active stories.
          </div>
        </div>
      </template>
    </template>

    <div
      v-if="!preview && !loading && !error && activeJobs.length === 0"
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
