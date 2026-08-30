<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import {
  downloadDirect,
  downloadPost,
  type FetchedPostCategory,
  type Post,
  type StoryItem,
} from "../lib/ipc";
import { canonicalInstagramUrl } from "../lib/postDisplay";
import { useJobsStore } from "../stores/jobs";

const props = defineProps<{
  username: string;
  post?: Post | null;
  story?: StoryItem | null;
  postCategory?: FetchedPostCategory;
}>();

const emit = defineEmits<{ close: [] }>();
const jobs = useJobsStore();
const busy = ref(false);
const error = ref<string | null>(null);
const copyError = ref<string | null>(null);
const copyFeedback = ref<"description" | "link" | null>(null);
let clearCopyFeedbackTimer: number | undefined;
let copyGeneration = 0;

const videoUrl = computed(() => {
  if (props.post) return props.post.resources.find((r) => r.kind === "video")?.url ?? null;
  return props.story?.kind === "video" ? props.story.media_url : null;
});

const imageUrl = computed(() => {
  if (props.post) {
    return props.post.thumbnail_url ?? props.post.resources.find((r) => r.kind === "photo")?.url ?? "";
  }
  return props.story?.kind === "photo" ? props.story.media_url : "";
});

const caption = computed(() => props.post?.caption ?? "");
const hasCaption = computed(() => caption.value.trim().length > 0);
const canonicalPostUrl = computed(() =>
  props.post ? canonicalInstagramUrl(props.post.code, props.postCategory ?? "posts") : "",
);
const meta = computed(() => {
  if (props.post) {
    const who = props.post.owner_username ?? props.username;
    return `@${who} · ${props.post.code}`;
  }
  return `Story · @${props.username}`;
});

function clearCopyFeedback() {
  if (clearCopyFeedbackTimer !== undefined) {
    window.clearTimeout(clearCopyFeedbackTimer);
    clearCopyFeedbackTimer = undefined;
  }
  copyFeedback.value = null;
  copyError.value = null;
}

function invalidateCopyOperations() {
  copyGeneration++;
  clearCopyFeedback();
}

async function copy(value: string, kind: "description" | "link") {
  if (!value) return;
  const generation = ++copyGeneration;
  clearCopyFeedback();
  try {
    await writeText(value);
    if (generation !== copyGeneration) return;
    copyFeedback.value = kind;
    clearCopyFeedbackTimer = window.setTimeout(clearCopyFeedback, 2000);
  } catch {
    if (generation !== copyGeneration) return;
    copyError.value = "Could not copy. Please try again.";
  }
}

function close() {
  invalidateCopyOperations();
  emit("close");
}

async function download() {
  if (busy.value) return;
  busy.value = true;
  error.value = null;
  try {
    if (props.post) {
      const id = await downloadPost(props.post.code);
      jobs.addPlaceholder(id, `Post ${props.post.code}`);
    } else if (props.story) {
      const id = await downloadDirect(props.username, "stories", [
        { url: props.story.media_url, pk: props.story.pk, taken_at: props.story.taken_at },
      ]);
      jobs.addPlaceholder(id, `@${props.username} story`);
    }
    close();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") close();
}

onMounted(() => window.addEventListener("keydown", onKey));
onUnmounted(() => {
  window.removeEventListener("keydown", onKey);
  invalidateCopyOperations();
});

watch(
  [() => props.post?.code, () => props.story?.pk, () => props.postCategory],
  invalidateCopyOperations,
);
</script>

<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4" @click.self="close">
      <div class="card w-full max-w-xl overflow-hidden">
        <video v-if="videoUrl" :src="videoUrl" controls class="max-h-[60vh] w-full bg-black" />
        <img
          v-else-if="imageUrl"
          :src="imageUrl"
          class="max-h-[60vh] w-full bg-black object-contain"
          referrerpolicy="no-referrer"
        />
        <div v-else class="flex h-64 items-center justify-center bg-surface-2 text-sm text-slate-500">
          No preview available
        </div>

        <div class="space-y-3 p-4">
          <p class="text-xs text-slate-500">{{ meta }}</p>
          <p v-if="caption" class="line-clamp-4 text-sm text-slate-300">{{ caption }}</p>
          <p v-if="error" class="rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-sm text-err">{{ error }}</p>
          <p
            v-if="copyError"
            data-copy-error
            role="alert"
            class="rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-sm text-err"
          >
            {{ copyError }}
          </p>
          <p class="sr-only" aria-live="polite">
            {{ copyFeedback === "description" ? "Copy description copied" : copyFeedback === "link" ? "Copy link copied" : "" }}
          </p>
          <div class="flex flex-wrap items-center justify-end gap-2">
            <button
              v-if="post"
              data-action="copy-description"
              class="btn-secondary"
              :disabled="!hasCaption"
              :aria-disabled="!hasCaption"
              :title="hasCaption ? undefined : 'This post has no description to copy.'"
              @click="copy(caption, 'description')"
            >
              Copy description<span v-if="copyFeedback === 'description'" class="ml-2 text-ok">Copied</span>
            </button>
            <button
              v-if="post"
              data-action="copy-link"
              class="btn-secondary"
              @click="copy(canonicalPostUrl, 'link')"
            >
              Copy link<span v-if="copyFeedback === 'link'" class="ml-2 text-ok">Copied</span>
            </button>
            <button class="btn-primary" :disabled="busy" @click="download">
              {{ busy ? "Starting…" : "Download" }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>
