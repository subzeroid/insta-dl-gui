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
import { tokenizeCaptionMentions } from "../lib/captionMentions";
import { canonicalInstagramUrl } from "../lib/postDisplay";
import { useJobsStore } from "../stores/jobs";
import RemoteImage from "./RemoteImage.vue";
import RemoteVideo from "./RemoteVideo.vue";

const props = defineProps<{
  username: string;
  post?: Post | null;
  story?: StoryItem | null;
  postCategory?: FetchedPostCategory;
}>();

const emit = defineEmits<{ close: []; "open-profile": [username: string] }>();
const jobs = useJobsStore();
const busy = ref(false);
const error = ref<string | null>(null);
const copyError = ref<string | null>(null);
const copyFeedback = ref<"description" | "link" | null>(null);
let clearCopyFeedbackTimer: number | undefined;
let copyGeneration = 0;

const videoSource = computed(() =>
  props.post
    ? props.post.resources.find((r) => r.kind === "video")?.url ?? ""
    : props.story?.kind === "video"
      ? props.story.media_url
      : "",
);

const imageSource = computed(() => {
  if (props.post) {
    return props.post.thumbnail_url ?? props.post.resources.find((r) => r.kind === "photo")?.url ?? "";
  }
  return props.story?.kind === "photo" ? props.story.media_url : "";
});

const caption = computed(() => props.post?.caption ?? "");
const captionTokens = computed(() => tokenizeCaptionMentions(caption.value));
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

function openProfile(event: MouseEvent, username: string) {
  if (
    event.button !== 0 ||
    event.metaKey ||
    event.ctrlKey ||
    event.shiftKey ||
    event.altKey
  ) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();
  emit("open-profile", username);
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
      <div
        data-testid="post-modal-card"
        class="card max-h-[calc(100vh-2rem)] w-full max-w-xl overflow-y-auto"
      >
        <RemoteVideo
          v-if="videoSource"
          :source="videoSource"
          :label="`${meta} video preview`"
          :controls="true"
          class="h-[60vh] max-h-[60vh] w-full"
        />
        <RemoteImage
          v-else-if="imageSource"
          :source="imageSource"
          :alt="`${meta} preview`"
          variant="modal"
          class="h-[60vh] max-h-[60vh] w-full bg-black"
        />
        <div v-else class="flex h-64 items-center justify-center bg-surface-2 text-sm text-slate-500">
          No preview available
        </div>

        <div class="space-y-3 p-4">
          <p class="text-xs text-slate-500">{{ meta }}</p>
          <p
            v-if="caption"
            data-caption
            class="whitespace-pre-wrap break-words text-sm text-slate-300"
            :class="{ 'line-clamp-4': !captionTokens.some((token) => token.kind === 'mention') }"
          >
            <template v-for="(token, index) in captionTokens" :key="index">
              <a
                v-if="token.kind === 'mention'"
                :data-caption-mention="token.username"
                :href="`/explore?profile=${encodeURIComponent(token.username)}`"
                class="rounded-sm text-accent underline decoration-current underline-offset-2 hover:decoration-current focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
                @click="openProfile($event, token.username)"
              >{{ token.text }}</a>
              <template v-else>{{ token.text }}</template>
            </template>
          </p>
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
