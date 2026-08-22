<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useJobsStore } from "../stores/jobs";
import JobCard from "../components/JobCard.vue";
import { useAppStore } from "../stores/app";
import {
  downloadPost,
  enqueueProfileDownload,
  fetchProfile,
  resolveInput,
  type ProfileOptions,
  type ProfilePreview,
} from "../lib/ipc";
import { buildProfileOptions, hasProfileSelection } from "../lib/profileOptions";

const app = useAppStore();
const jobs = useJobsStore();
const input = ref("");
const busy = ref(false);
const error = ref<string | null>(null);
const notice = ref<string | null>(null);

const preview = ref<ProfilePreview | null>(null);
const previewLoading = ref(false);
const profileDownloadBusy = ref(false);

const opts = computed<ProfileOptions>(() => {
  const max = maxPosts.value;
  return buildProfileOptions(preview.value?.profile.is_private === true, {
    posts: posts.value,
    reels: reels.value,
    stories: stories.value,
    highlights: highlights.value,
    avatar: avatar.value,
    max_posts: max !== null && max > 0 ? max : null,
  });
});
const canDownload = computed(() => hasProfileSelection(opts.value));

const posts = ref(true);
const reels = ref(false);
const stories = ref(false);
const highlights = ref(false);
const avatar = ref(true);
const maxPosts = ref<number | null>(null); // null = all

async function submit() {
  const raw = input.value.trim();
  if (!raw || busy.value || profileDownloadBusy.value) return;
  error.value = null;
  notice.value = null;
  preview.value = null;
  busy.value = true;
  try {
    const target = await resolveInput(raw);
    if (target.kind === "post") {
      input.value = "";
      const id = await downloadPost(target.code);
      jobs.addPlaceholder(id, target.code);
    } else {
      previewLoading.value = true;
      const p = await fetchProfile(target.username);
      preview.value = p;
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
    previewLoading.value = false;
  }
}

async function startProfileDownload() {
  if (!preview.value || !canDownload.value || profileDownloadBusy.value) return;
  const username = preview.value.profile.username;
  profileDownloadBusy.value = true;
  try {
    const id = await enqueueProfileDownload(username, opts.value);
    jobs.addPlaceholder(id, `@${username}`);
    preview.value = null;
    input.value = "";
  } catch (e) {
    error.value = String(e);
  } finally {
    profileDownloadBusy.value = false;
  }
}

const activeJobs = computed(() =>
  [...jobs.jobs.values()].filter((j) => j.state === "downloading" || j.state === "fetching"),
);

function fmtCount(n?: number) {
  return n === undefined ? "—" : new Intl.NumberFormat("en", { notation: "compact" }).format(n);
}

onMounted(() => {
  if (new URLSearchParams(window.location.search).get("demo") === "profile") {
    input.value = "@instagram";
    void submit();
  }
});
</script>

<template>
  <div class="mx-auto max-w-2xl space-y-6 p-6">
    <form class="flex gap-2" @submit.prevent="submit">
      <input
        v-model="input"
        class="input"
        placeholder="@username or instagram.com/p/… link"
        spellcheck="false"
        autocomplete="off"
        :disabled="busy || profileDownloadBusy"
      />
      <button class="btn-primary shrink-0" type="submit" :disabled="busy || profileDownloadBusy || !input.trim()">
        {{ previewLoading ? "…" : "Fetch" }}
      </button>
    </form>

    <p v-if="error" class="rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-sm text-err">{{ error }}</p>

    <!-- Active downloads -->
    <div v-if="activeJobs.length > 0" class="space-y-3">
      <JobCard v-for="job in activeJobs" :key="job.id" :job="job" />
    </div>

    <!-- Profile preview -->
    <div v-if="preview" class="card overflow-hidden">
      <div class="flex items-center gap-4 p-5">
        <img
          v-if="preview.profile.avatar_url"
          :src="preview.profile.avatar_url"
          class="h-16 w-16 rounded-full border border-line object-cover"
          referrerpolicy="no-referrer"
        />
        <div class="min-w-0 flex-1">
          <div class="flex items-center gap-1.5">
            <span class="truncate font-semibold text-slate-100">{{ preview.profile.username }}</span>
            <span v-if="preview.profile.is_verified" class="text-xs text-sky-400">✔</span>
          </div>
          <p class="truncate text-sm text-slate-400">{{ preview.profile.full_name || "\u00A0" }}</p>
          <p class="mt-0.5 text-xs tabular-nums text-slate-500">
            {{ fmtCount(preview.profile.media_count) }} posts · {{ fmtCount(preview.profile.follower_count) }} followers
          </p>
        </div>
      </div>

      <div class="border-t border-line p-5">
        <div v-if="preview.profile.is_private" class="mb-3 text-sm text-warn">
          Private profile — only the avatar is accessible.
        </div>
        <div class="grid grid-cols-2 gap-2 sm:grid-cols-3">
          <template v-if="!preview.profile.is_private">
            <label class="flex cursor-pointer items-center gap-2 rounded-lg border border-line px-3 py-2 text-sm hover:bg-surface-2" :class="{ 'border-accent/60': posts }">
              <input v-model="posts" type="checkbox" class="accent-[var(--color-accent)]" /> Posts
            </label>
            <label class="flex cursor-pointer items-center gap-2 rounded-lg border border-line px-3 py-2 text-sm hover:bg-surface-2" :class="{ 'border-accent/60': reels }">
              <input v-model="reels" type="checkbox" class="accent-[var(--color-accent)]" /> Reels
            </label>
            <label class="flex cursor-pointer items-center gap-2 rounded-lg border border-line px-3 py-2 text-sm hover:bg-surface-2" :class="{ 'border-accent/60': stories }">
              <input v-model="stories" type="checkbox" class="accent-[var(--color-accent)]" /> Stories
            </label>
            <label class="flex cursor-pointer items-center gap-2 rounded-lg border border-line px-3 py-2 text-sm hover:bg-surface-2" :class="{ 'border-accent/60': highlights }">
              <input v-model="highlights" type="checkbox" class="accent-[var(--color-accent)]" /> Highlights
            </label>
          </template>
          <label class="flex cursor-pointer items-center gap-2 rounded-lg border border-line px-3 py-2 text-sm hover:bg-surface-2" :class="{ 'border-accent/60': avatar }">
            <input v-model="avatar" type="checkbox" class="accent-[var(--color-accent)]" /> Avatar
          </label>
          <label v-if="!preview.profile.is_private" class="flex items-center gap-2 rounded-lg border border-line px-3 py-2 text-sm">
            <span class="text-slate-500">Max</span>
            <input
              v-model.number="maxPosts"
              type="number"
              min="0"
              class="w-full bg-transparent tabular-nums outline-none placeholder-slate-600"
              placeholder="all"
            />
          </label>
        </div>
        <div class="mt-4 flex items-center justify-between gap-3 border-t border-line pt-4">
          <p class="text-xs text-slate-500">saves to <span class="font-mono">{{ app.destDir }}/{{ preview.profile.username }}/</span></p>
          <button
            class="btn-primary shrink-0"
            :disabled="!canDownload || profileDownloadBusy"
            @click="startProfileDownload"
          >
            Download
          </button>
        </div>
      </div>
    </div>

    <div v-if="!preview && !notice" class="card flex items-center justify-center p-12 text-sm text-slate-500">
      Paste a post link or a username to get started.
    </div>
    <p v-if="notice" class="text-sm text-warn">{{ notice }}</p>
  </div>
</template>

<style scoped>
input[type="number"]::-webkit-inner-spin-button {
  opacity: 0.4;
}
</style>
