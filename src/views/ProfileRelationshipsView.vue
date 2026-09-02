<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import {
  fetchProfileSummary,
  fetchRelationships,
  searchRelationships,
  type Profile,
  type RelationshipKind,
  type SearchUser,
} from "../lib/ipc";
import RemoteImage from "../components/RemoteImage.vue";
import { useExplorerStore } from "../stores/explorer";

const props = defineProps<{
  username: string;
  kind: RelationshipKind;
}>();

const explorer = useExplorerStore();
const profile = ref<Profile | null>(null);
const users = ref<SearchUser[]>([]);
const nextCursor = ref<string | null>(null);
const page = ref(0);
const pageSize = ref(0);
const listLoading = ref(false);
const loadingMore = ref(false);
const listError = ref<string | null>(null);
const failedCursor = ref<string | null | undefined>(undefined);
const searchQuery = ref("");
const searchResults = ref<SearchUser[] | null>(null);
const searchLoading = ref(false);
const searchError = ref<string | null>(null);

let listGeneration = 0;
let searchGeneration = 0;
let searchTimer = 0;
const requestedCursors = new Set<string>();

const kindLabel = computed(() => props.kind === "followers" ? "Followers" : "Following");
type RelationshipTab = {
  kind: RelationshipKind;
  label: string;
  count?: number | null;
};

const relationshipTabs = computed<RelationshipTab[]>(() => [
  {
    kind: "followers" as const,
    label: "Followers",
    count: profile.value?.follower_count,
  },
  {
    kind: "following" as const,
    label: "Following",
    count: profile.value?.following_count,
  },
]);
const privateProfile = computed(() => profile.value?.is_private === true);
const relationshipTotal = computed(() =>
  props.kind === "followers" ? profile.value?.follower_count : profile.value?.following_count,
);
const totalPages = computed(() => {
  const total = relationshipTotal.value;
  if (total == null || pageSize.value <= 0) return null;
  return Math.max(page.value, Math.ceil(total / pageSize.value));
});
const pageStatus = computed(() => {
  if (page.value <= 0) return "";
  return totalPages.value === null
    ? `Page ${page.value.toLocaleString("en")}`
    : `Page ${page.value.toLocaleString("en")} of ${totalPages.value.toLocaleString("en")}`;
});
const normalizedSearch = computed(() => searchQuery.value.trim());
const searchMode = computed(() => normalizedSearch.value.length > 0);
const visibleUsers = computed(() => searchMode.value ? (searchResults.value ?? []) : users.value);
const searchStatus = computed(() => {
  if (!searchMode.value) return "";
  if (normalizedSearch.value.length < 2) return "Type at least 2 characters";
  if (searchLoading.value) return "Searching…";
  if (searchError.value) return searchError.value;
  if (searchResults.value === null) return "";
  const count = searchResults.value.length;
  return `${count} result${count === 1 ? "" : "s"}`;
});

function compactCount(count: number | null | undefined) {
  return count == null ? "" : new Intl.NumberFormat("en", { notation: "compact" }).format(count);
}

function relationshipLinkLabel(relationship: RelationshipTab) {
  return relationship.count == null
    ? relationship.label
    : `${relationship.label}, ${relationship.count.toLocaleString("en")}`;
}

function relationshipPath(kind: RelationshipKind) {
  return `/explore/${encodeURIComponent(props.username)}/${kind}`;
}

function mergeUsers(existing: readonly SearchUser[], incoming: readonly SearchUser[]) {
  const seen = new Set(existing.map((user) => user.pk));
  return [...existing, ...incoming.filter((user) => !seen.has(user.pk))];
}

async function loadPage(cursor: string | null, generation = listGeneration) {
  const firstPage = cursor === null;
  if (!profile.value || (firstPage ? listLoading.value : loadingMore.value)) return;
  if (firstPage) listLoading.value = true;
  else loadingMore.value = true;
  listError.value = null;
  failedCursor.value = undefined;
  try {
    const result = await fetchRelationships(profile.value.pk, props.kind, cursor);
    if (generation !== listGeneration) return;
    if (firstPage) {
      users.value = [...result.users];
      page.value = 1;
      pageSize.value = result.users.length;
      requestedCursors.clear();
    } else {
      requestedCursors.add(cursor);
      users.value = mergeUsers(users.value, result.users);
      page.value += 1;
    }
    const candidate = result.next_cursor?.trim() || null;
    nextCursor.value = candidate && !requestedCursors.has(candidate) ? candidate : null;
  } catch (error) {
    if (generation === listGeneration) {
      listError.value = String(error);
      failedCursor.value = cursor;
    }
  } finally {
    if (generation === listGeneration) {
      listLoading.value = false;
      loadingMore.value = false;
    }
  }
}

async function startSession() {
  const generation = ++listGeneration;
  searchGeneration += 1;
  window.clearTimeout(searchTimer);
  profile.value = null;
  users.value = [];
  nextCursor.value = null;
  page.value = 0;
  pageSize.value = 0;
  listError.value = null;
  failedCursor.value = undefined;
  searchQuery.value = "";
  searchResults.value = null;
  searchLoading.value = false;
  searchError.value = null;
  requestedCursors.clear();
  listLoading.value = true;
  try {
    const retained = explorer.profilePreview?.profile;
    const summary = retained?.username.toLowerCase() === props.username.toLowerCase()
      ? retained
      : await fetchProfileSummary(props.username);
    if (generation !== listGeneration) return;
    profile.value = summary;
    listLoading.value = false;
    if (summary.is_private) return;
    await loadPage(null, generation);
  } catch (error) {
    if (generation === listGeneration) {
      listError.value = String(error);
      listLoading.value = false;
    }
  }
}

function loadMore() {
  if (nextCursor.value) void loadPage(nextCursor.value);
}

function retryList() {
  if (profile.value && failedCursor.value !== undefined) void loadPage(failedCursor.value);
  else void startSession();
}

watch(
  () => [props.username, props.kind] as const,
  () => { void startSession(); },
  { immediate: true },
);

watch(searchQuery, (value) => {
  window.clearTimeout(searchTimer);
  const generation = ++searchGeneration;
  const query = value.trim();
  searchError.value = null;
  searchLoading.value = false;
  if (query.length === 0) {
    searchResults.value = null;
    return;
  }
  searchResults.value = [];
  if (query.length < 2 || !profile.value || privateProfile.value) return;
  searchResults.value = null;
  searchLoading.value = true;
  searchTimer = window.setTimeout(async () => {
    if (generation !== searchGeneration || !profile.value) return;
    try {
      const result = await searchRelationships(profile.value.pk, props.kind, query);
      if (generation === searchGeneration) searchResults.value = result;
    } catch (error) {
      if (generation === searchGeneration) searchError.value = String(error);
    } finally {
      if (generation === searchGeneration) searchLoading.value = false;
    }
  }, 350);
});

onUnmounted(() => {
  listGeneration += 1;
  searchGeneration += 1;
  window.clearTimeout(searchTimer);
});
</script>

<template>
  <div class="mx-auto max-w-3xl space-y-5 p-6">
    <RouterLink
      :to="{ path: '/explore', query: { profile: username } }"
      class="inline-flex items-center gap-1 text-sm text-slate-400 hover:text-slate-100"
    >
      <span aria-hidden="true">←</span> Back to @{{ username }}
    </RouterLink>

    <section class="card p-5">
      <div class="flex min-w-0 items-center gap-3">
        <RemoteImage
          :source="profile?.avatar_url"
          :alt="`@${profile?.username ?? username} profile picture`"
          variant="avatar"
          class="size-12 shrink-0 rounded-full"
        />
        <div class="min-w-0 flex-1">
          <h1 class="truncate text-xl font-semibold text-slate-100">
            @{{ profile?.username ?? username }} {{ kindLabel }}
          </h1>
          <p v-if="profile?.full_name" class="truncate text-sm text-slate-500">
            {{ profile.full_name }}
          </p>
        </div>
      </div>

      <nav aria-label="Profile relationships" class="mt-4 flex gap-1">
        <RouterLink
          v-for="relationship in relationshipTabs"
          :key="relationship.kind"
          :to="relationshipPath(relationship.kind)"
          class="rounded-lg px-3 py-1.5 text-sm capitalize text-slate-400 hover:bg-surface-2 hover:text-slate-100"
          :class="relationship.kind === kind ? 'bg-surface-3 !text-slate-100' : ''"
          :title="relationshipLinkLabel(relationship)"
          :aria-label="relationshipLinkLabel(relationship)"
        >
          {{ relationship.label }}
          <span
            v-if="relationship.count != null"
            data-relationship-count
            aria-hidden="true"
            class="ml-1 text-slate-500 tabular-nums"
          >
            {{ compactCount(relationship.count) }}
          </span>
        </RouterLink>
      </nav>
    </section>

    <section class="space-y-3">
      <div class="card p-4">
        <label :for="`relationship-search-${kind}`" class="sr-only">
          Search {{ kind }}
        </label>
        <input
          :id="`relationship-search-${kind}`"
          v-model="searchQuery"
          data-relationship-search
          class="input"
          type="search"
          :placeholder="`Search ${kind}`"
          autocomplete="off"
          spellcheck="false"
          :disabled="!profile || privateProfile"
        />
        <p class="mt-2 text-xs text-slate-500">
          Search checks the complete {{ kind }} list through HikerAPI and uses 1 request.
        </p>
      </div>

      <div
        v-if="privateProfile"
        data-private-profile
        class="card px-4 py-6 text-center text-sm text-slate-500"
      >
        Followers and following lists are unavailable for private profiles.
      </div>
      <div
        v-else-if="listError && !searchMode"
        data-list-error
        class="card flex items-center justify-between gap-3 px-4 py-3 text-sm"
      >
        <span class="text-err">{{ listError }}</span>
        <button data-action="retry-list" class="btn-secondary shrink-0" @click="retryList">
          Retry
        </button>
      </div>
      <div v-else-if="listLoading && !searchMode" class="py-12 text-center text-sm text-slate-500">
        Loading {{ kind }}…
      </div>

      <p
        v-if="searchMode"
        data-search-status
        class="px-1 text-sm"
        :class="searchError ? 'text-err' : 'text-slate-500'"
      >
        {{ searchStatus }}
      </p>

      <div v-if="visibleUsers.length > 0" class="card divide-y divide-line overflow-hidden">
        <RouterLink
          v-for="relatedUser in visibleUsers"
          :key="relatedUser.pk"
          data-related-user
          :to="{ path: '/explore', query: { profile: relatedUser.username } }"
          class="flex items-center gap-3 px-4 py-3 hover:bg-surface-2"
        >
          <RemoteImage
            :source="relatedUser.avatar_url"
            alt=""
            variant="compact-avatar"
            class="size-10 shrink-0 rounded-full"
          />
          <span class="min-w-0 flex-1">
            <span class="flex items-center gap-1.5">
              <span class="truncate font-medium text-slate-200">{{ relatedUser.username }}</span>
              <span v-if="relatedUser.is_verified" class="text-xs text-sky-400" title="Verified">✔</span>
            </span>
            <span class="block truncate text-sm text-slate-500">{{ relatedUser.full_name || "\u00A0" }}</span>
          </span>
          <span v-if="relatedUser.is_private" class="shrink-0 text-xs text-slate-500">private</span>
          <span aria-hidden="true" class="shrink-0 text-slate-500">›</span>
        </RouterLink>
      </div>
      <div
        v-else-if="!privateProfile && !listLoading && !listError && (!searchMode || (!searchLoading && normalizedSearch.length >= 2))"
        class="card p-10 text-center text-sm text-slate-500"
      >
        {{ searchMode ? "No matching accounts." : `No ${kind} available.` }}
      </div>

      <div
        v-if="!searchMode && page > 0"
        class="flex flex-wrap items-center justify-center gap-3"
      >
        <span data-page-status class="text-xs tabular-nums text-slate-500">{{ pageStatus }}</span>
        <button
          v-if="nextCursor"
          data-action="load-more"
          class="btn-secondary"
          :disabled="loadingMore"
          @click="loadMore"
        >
          {{ loadingMore ? "Loading…" : "Load more" }}
        </button>
      </div>
    </section>
  </div>
</template>
