<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";

import {
  formatBytes,
  libraryMediaUrl,
  openLibraryFile,
  requestLibraryPreviewAccess,
  revealLibraryFile,
  type JobOutputFile,
} from "../lib/ipc";
import type { JobView } from "../stores/jobs";

const props = defineProps<{ job: JobView }>();
const emit = defineEmits<{ close: [] }>();
const OUTPUT_PAGE_SIZE = 50;
const VIDEO_PREVIEW_ERROR = "Could not play video preview.";

const dialog = ref<HTMLElement | null>(null);
const accessState = ref<"idle" | "pending" | "allowed" | "denied" | "error" | "unindexed">("idle");
const rowErrors = reactive<Record<number, string>>({});
const busyAction = ref<string | null>(null);
const visibleCount = ref(OUTPUT_PAGE_SIZE);
const previewUrls = reactive(new Map<number, string>());
const playingPreviewIds = reactive(new Set<number>());
let accessGeneration = 0;
let mounted = false;
let previousBodyOverflow = "";

const vInitialFocus = {
  mounted(element: HTMLElement) {
    element.focus();
  },
};

const outputs = computed(() => props.job.outputs ?? []);
const visibleOutputs = computed(() => outputs.value.slice(0, visibleCount.value));
const remainingCount = computed(() => Math.max(0, outputs.value.length - visibleCount.value));
const summary = computed(() => {
  const count = outputs.value.length;
  const files = `${count} file${count === 1 ? "" : "s"} saved`;
  if (props.job.requestedItems === undefined) return files;
  const requested = props.job.requestedItems;
  return `${requested} item${requested === 1 ? "" : "s"} requested · ${files}`;
});

function syncPreviewUrls() {
  if (accessState.value !== "allowed") return;
  for (const output of visibleOutputs.value) {
    if (typeof output.file_id === "number" && !previewUrls.has(output.file_id)) {
      previewUrls.set(output.file_id, libraryMediaUrl(output.file_id));
    }
  }
}

function invalidateAccess() {
  accessGeneration++;
}

async function loadPreviewAccess() {
  const generation = ++accessGeneration;
  previewUrls.clear();
  playingPreviewIds.clear();
  for (const key of Object.keys(rowErrors)) delete rowErrors[Number(key)];
  busyAction.value = null;
  const firstIndexed = outputs.value.find((output) => typeof output.file_id === "number");
  if (!firstIndexed || typeof firstIndexed.file_id !== "number") {
    accessState.value = "unindexed";
    return;
  }
  accessState.value = "pending";
  try {
    const allowed = await requestLibraryPreviewAccess(firstIndexed.file_id);
    if (!mounted || generation !== accessGeneration) return;
    accessState.value = allowed ? "allowed" : "denied";
    if (allowed) syncPreviewUrls();
  } catch {
    if (!mounted || generation !== accessGeneration) return;
    accessState.value = "error";
  }
}

async function showMore() {
  const firstNewIndex = visibleCount.value;
  visibleCount.value = Math.min(outputs.value.length, visibleCount.value + OUTPUT_PAGE_SIZE);
  syncPreviewUrls();
  await nextTick();
  if (!dialog.value) return;
  const focusTarget = remainingCount.value > 0
    ? dialog.value.querySelector<HTMLElement>("[data-action='show-more-outputs']")
    : dialog.value.querySelector<HTMLElement>(`[data-output-row="${firstNewIndex}"]`);
  focusTarget?.focus();
}

function close() {
  invalidateAccess();
  emit("close");
}

function typeLabel(output: JobOutputFile) {
  return output.kind === "video" ? "Video" : "Photo";
}

function isPreviewPlaying(output: JobOutputFile) {
  return typeof output.file_id === "number" && playingPreviewIds.has(output.file_id);
}

function setPreviewPlaying(output: JobOutputFile, playing: boolean) {
  if (typeof output.file_id !== "number") return;
  if (playing) playingPreviewIds.add(output.file_id);
  else playingPreviewIds.delete(output.file_id);
}

function handleVideoPreviewError(event: Event, output: JobOutputFile, index: number) {
  const video = event.currentTarget;
  if (!(video instanceof HTMLVideoElement) || !video.isConnected) return;
  setPreviewPlaying(output, false);
  rowErrors[index] = VIDEO_PREVIEW_ERROR;
}

async function toggleVideoPreview(event: MouseEvent, output: JobOutputFile, index: number) {
  const container = (event.currentTarget as HTMLButtonElement).parentElement;
  const video = container?.querySelector<HTMLVideoElement>("video[data-output-preview]");
  if (!video) return;

  if (isPreviewPlaying(output)) {
    video.pause();
    return;
  }

  delete rowErrors[index];
  const generation = accessGeneration;
  try {
    if (video.error) video.load();
    await video.play();
  } catch (cause) {
    if (!mounted || generation !== accessGeneration) return;
    if (cause instanceof DOMException && cause.name === "AbortError") return;
    setPreviewPlaying(output, false);
    rowErrors[index] = VIDEO_PREVIEW_ERROR;
  }
}

async function runAction(action: "open" | "reveal", output: JobOutputFile, index: number) {
  if (typeof output.file_id !== "number" || busyAction.value !== null) return;
  const key = `${action}-${index}`;
  busyAction.value = key;
  delete rowErrors[index];
  try {
    if (action === "open") await openLibraryFile(output.file_id);
    else await revealLibraryFile(output.file_id);
  } catch (cause) {
    rowErrors[index] = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (busyAction.value === key) busyAction.value = null;
  }
}

function focusableElements() {
  if (!dialog.value) return [];
  return Array.from(
    dialog.value.querySelectorAll<HTMLElement>(
      'button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])',
    ),
  ).filter((element) => !element.hasAttribute("hidden"));
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    close();
    return;
  }
  if (event.key !== "Tab") return;
  const focusable = focusableElements();
  if (focusable.length === 0) {
    event.preventDefault();
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const active = document.activeElement;
  if (event.shiftKey && (active === first || !dialog.value?.contains(active))) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && (active === last || !dialog.value?.contains(active))) {
    event.preventDefault();
    first.focus();
  }
}

watch(
  () => props.job.id,
  () => {
    visibleCount.value = OUTPUT_PAGE_SIZE;
    if (mounted) void loadPreviewAccess();
  },
);

onMounted(() => {
  mounted = true;
  previousBodyOverflow = document.body.style.overflow;
  document.body.style.overflow = "hidden";
  window.addEventListener("keydown", onKeydown);
  void loadPreviewAccess();
});

onBeforeUnmount(() => {
  mounted = false;
  invalidateAccess();
  window.removeEventListener("keydown", onKeydown);
  document.body.style.overflow = previousBodyOverflow;
});
</script>

<template>
  <Teleport to="body">
    <div
      data-testid="download-details-backdrop"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/75 p-4"
      @click.self="close"
    >
      <section
        ref="dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="download-details-title"
        class="card max-h-[88vh] w-full max-w-3xl overflow-y-auto shadow-2xl"
      >
        <header class="sticky top-0 z-10 flex items-start justify-between border-b border-line bg-surface-1 p-5">
          <div class="min-w-0">
            <p class="text-xs font-semibold uppercase tracking-[0.14em] text-accent">Downloaded files</p>
            <h2 id="download-details-title" class="mt-1 truncate text-lg font-semibold text-slate-100">
              {{ job.label }}
            </h2>
            <p data-output-summary class="mt-1 text-sm text-slate-400">{{ summary }}</p>
          </div>
          <button
            v-initial-focus
            type="button"
            class="btn-secondary !px-3"
            aria-label="Close download details"
            @click="close"
          >
            Close
          </button>
        </header>

        <div class="space-y-3 p-5">
          <p
            v-if="accessState === 'denied' || accessState === 'error'"
            data-preview-state
            class="rounded-lg border border-line bg-surface-2 px-3 py-2 text-sm text-slate-400"
          >
            Preview unavailable. Files can still be opened from this list.
          </p>

          <article
            v-for="(output, index) in visibleOutputs"
            :key="`${output.ordinal}-${index}`"
            :data-output-row="index"
            tabindex="-1"
            class="rounded-lg border border-line bg-surface-2 p-3"
          >
            <div class="flex flex-wrap gap-4">
              <div class="relative flex h-24 w-24 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-black text-xs text-slate-500">
                <template v-if="typeof output.file_id === 'number' && previewUrls.has(output.file_id)">
                  <template v-if="output.kind === 'video'">
                    <video
                      data-output-preview
                      :data-file-id="output.file_id"
                      :src="previewUrls.get(output.file_id)"
                      aria-hidden="true"
                      preload="none"
                      class="pointer-events-none h-full w-full object-contain"
                      @play="setPreviewPlaying(output, true)"
                      @pause="setPreviewPlaying(output, false)"
                      @ended="setPreviewPlaying(output, false)"
                      @error="handleVideoPreviewError($event, output, index)"
                    />
                    <button
                      type="button"
                      data-action="toggle-video-preview"
                      :aria-label="`${isPreviewPlaying(output) ? 'Pause' : 'Play'} ${output.basename} preview`"
                      class="group absolute inset-0 flex items-center justify-center rounded-lg focus:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-inset"
                      @click="toggleVideoPreview($event, output, index)"
                    >
                      <span
                        data-video-control-indicator
                        class="flex h-8 w-8 items-center justify-center rounded-full bg-black/65 text-white shadow-lg transition-colors group-hover:bg-black/80"
                      >
                        <svg
                          v-if="isPreviewPlaying(output)"
                          aria-hidden="true"
                          viewBox="0 0 24 24"
                          fill="currentColor"
                          class="h-4 w-4"
                        >
                          <path d="M6 5h4v14H6zM14 5h4v14h-4z" />
                        </svg>
                        <svg
                          v-else
                          aria-hidden="true"
                          viewBox="0 0 24 24"
                          fill="currentColor"
                          class="h-4 w-4"
                        >
                          <path d="M8 5v14l11-7z" />
                        </svg>
                      </span>
                    </button>
                  </template>
                  <img
                    v-else
                    data-output-preview
                    :data-file-id="output.file_id"
                    :src="previewUrls.get(output.file_id)"
                    alt=""
                    loading="lazy"
                    class="h-full w-full object-contain"
                  />
                </template>
                <span v-else>{{ accessState === "pending" ? "Loading preview…" : "No preview" }}</span>
              </div>

              <div class="min-w-0 flex-1">
                <div class="flex flex-wrap items-start justify-between gap-3">
                  <div class="min-w-0">
                    <p class="text-xs font-semibold uppercase tracking-wide text-slate-500">
                      {{ typeLabel(output) }} · File {{ output.ordinal + 1 }}
                    </p>
                    <p data-output-basename class="mt-1 break-all text-sm font-medium text-slate-200">
                      {{ output.basename }}
                    </p>
                    <p class="mt-1 text-xs text-slate-500">{{ formatBytes(output.byte_size) }}</p>
                    <p v-if="typeof output.file_id !== 'number'" class="mt-1 text-xs text-warn">
                      Not indexed. Rescan Library to enable preview and file actions.
                    </p>
                  </div>
                  <div class="flex shrink-0 gap-2">
                    <button
                      type="button"
                      data-action="open-output"
                      :aria-label="`Open ${output.basename}`"
                      class="btn-secondary !px-3 !py-1.5 text-xs"
                      :disabled="typeof output.file_id !== 'number' || busyAction !== null"
                      @click="runAction('open', output, index)"
                    >
                      Open
                    </button>
                    <button
                      type="button"
                      data-action="reveal-output"
                      :aria-label="`Show ${output.basename} in Finder`"
                      class="btn-secondary !px-3 !py-1.5 text-xs"
                      :disabled="typeof output.file_id !== 'number' || busyAction !== null"
                      @click="runAction('reveal', output, index)"
                    >
                      Show in Finder
                    </button>
                  </div>
                </div>
                <p v-if="rowErrors[index]" data-row-error role="alert" class="mt-2 text-xs text-err">
                  {{ rowErrors[index] }}
                </p>
              </div>
            </div>
          </article>
          <div v-if="remainingCount > 0" class="flex justify-center pt-2">
            <button
              type="button"
              data-action="show-more-outputs"
              class="btn-secondary"
              @click="showMore"
            >
              Show more · {{ remainingCount }} remaining
            </button>
          </div>
        </div>
      </section>
    </div>
  </Teleport>
</template>
