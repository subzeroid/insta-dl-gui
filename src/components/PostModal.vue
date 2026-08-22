<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { downloadDirect, downloadPost, type Post, type StoryItem } from "../lib/ipc";
import { useJobsStore } from "../stores/jobs";

const props = defineProps<{
  username: string;
  post?: Post | null;
  story?: StoryItem | null;
}>();

const emit = defineEmits<{ close: [] }>();
const jobs = useJobsStore();
const busy = ref(false);
const error = ref<string | null>(null);

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
const meta = computed(() => {
  if (props.post) {
    const who = props.post.owner_username ?? props.username;
    return `@${who} · ${props.post.code}`;
  }
  return `Story · @${props.username}`;
});

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
    emit("close");
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

function onKey(e: KeyboardEvent) {
  if (e.key === "Escape") emit("close");
}

onMounted(() => window.addEventListener("keydown", onKey));
onUnmounted(() => window.removeEventListener("keydown", onKey));
</script>

<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4" @click.self="emit('close')">
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
          <div class="flex justify-end">
            <button class="btn-primary" :disabled="busy" @click="download">
              {{ busy ? "Starting…" : "Download" }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>
