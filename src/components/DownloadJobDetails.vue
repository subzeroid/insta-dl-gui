<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";

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

const dialog = ref<HTMLElement | null>(null);
const accessState = ref<"idle" | "pending" | "allowed" | "denied" | "error" | "unindexed">("idle");
const rowErrors = reactive<Record<number, string>>({});
const busyAction = ref<string | null>(null);
let accessGeneration = 0;
let mounted = false;
let previousBodyOverflow = "";

const vInitialFocus = {
  mounted(element: HTMLElement) {
    element.focus();
  },
};

const outputs = computed(() => props.job.outputs ?? []);
const summary = computed(() => {
  const count = outputs.value.length;
  const files = `${count} file${count === 1 ? "" : "s"} saved`;
  if (props.job.requestedItems === undefined) return files;
  const requested = props.job.requestedItems;
  return `${requested} item${requested === 1 ? "" : "s"} requested · ${files}`;
});
const previewUrls = computed(() => {
  const urls = new Map<number, string>();
  if (accessState.value !== "allowed") return urls;
  for (const output of outputs.value) {
    if (typeof output.file_id === "number") {
      urls.set(output.file_id, libraryMediaUrl(output.file_id));
    }
  }
  return urls;
});

function invalidateAccess() {
  accessGeneration++;
}

async function loadPreviewAccess() {
  const generation = ++accessGeneration;
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
  } catch {
    if (!mounted || generation !== accessGeneration) return;
    accessState.value = "error";
  }
}

function close() {
  invalidateAccess();
  emit("close");
}

function typeLabel(output: JobOutputFile) {
  return output.kind === "video" ? "Video" : "Photo";
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
      'button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), video[controls], [tabindex]:not([tabindex="-1"])',
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
            v-for="(output, index) in outputs"
            :key="`${output.ordinal}-${index}`"
            :data-output-row="index"
            class="rounded-lg border border-line bg-surface-2 p-3"
          >
            <div class="flex flex-wrap gap-4">
              <div class="flex h-24 w-24 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-black text-xs text-slate-500">
                <template v-if="typeof output.file_id === 'number' && previewUrls.has(output.file_id)">
                  <video
                    v-if="output.kind === 'video'"
                    data-output-preview
                    :data-file-id="output.file_id"
                    :src="previewUrls.get(output.file_id)"
                    controls
                    class="h-full w-full object-contain"
                  />
                  <img
                    v-else
                    data-output-preview
                    :data-file-id="output.file_id"
                    :src="previewUrls.get(output.file_id)"
                    alt=""
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
                      class="btn-secondary !px-3 !py-1.5 text-xs"
                      :disabled="typeof output.file_id !== 'number' || busyAction !== null"
                      @click="runAction('open', output, index)"
                    >
                      Open
                    </button>
                    <button
                      type="button"
                      data-action="reveal-output"
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
        </div>
      </section>
    </div>
  </Teleport>
</template>
