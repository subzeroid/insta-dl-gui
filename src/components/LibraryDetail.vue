<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";

import type { LibraryFile, LibraryItemDetail } from "../lib/ipc";
import { useLibraryStore } from "../stores/library";

const props = defineProps<{ detail: LibraryItemDetail }>();
const emit = defineEmits<{ close: [] }>();
const library = useLibraryStore();
const dialog = ref<HTMLElement | null>(null);
const closeButton = ref<HTMLButtonElement | null>(null);
const actionError = ref<string | null>(null);
const busyAction = ref<string | null>(null);
let previousBodyOverflow = "";

const owner = computed(() =>
  props.detail.owner_username ? `@${props.detail.owner_username}` : "Unknown owner",
);

const date = computed(() => {
  const timestamp = props.detail.taken_at ?? props.detail.imported_at;
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(timestamp * 1_000));
});

function formatBytes(bytes: number) {
  if (bytes < 1_000) return `${bytes} B`;
  if (bytes < 1_000_000) return `${(bytes / 1_000).toFixed(1)} KB`;
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

async function runAction(action: "open" | "reveal", file: LibraryFile) {
  if (!file.exists_on_disk || busyAction.value !== null) return;
  const key = `${action}-${file.id}`;
  busyAction.value = key;
  actionError.value = null;
  try {
    if (action === "open") await library.openFile(file.id);
    else await library.revealFile(file.id);
  } catch (cause) {
    actionError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (busyAction.value === key) busyAction.value = null;
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    emit("close");
    return;
  }
  if (event.key !== "Tab" || !dialog.value) return;
  const focusable = Array.from(
    dialog.value.querySelectorAll<HTMLElement>(
      'button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])',
    ),
  ).filter((element) => !element.hasAttribute("hidden"));
  if (focusable.length === 0) {
    event.preventDefault();
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const active = document.activeElement;
  if (event.shiftKey && (active === first || !dialog.value.contains(active))) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && (active === last || !dialog.value.contains(active))) {
    event.preventDefault();
    first.focus();
  }
}

onMounted(() => {
  previousBodyOverflow = document.body.style.overflow;
  document.body.style.overflow = "hidden";
  window.addEventListener("keydown", onKeydown);
  closeButton.value?.focus();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  document.body.style.overflow = previousBodyOverflow;
});
</script>

<template>
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/75 p-4"
    @click.self="emit('close')"
  >
    <section
      ref="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="library-detail-title"
      class="card max-h-[88vh] w-full max-w-2xl overflow-y-auto shadow-2xl"
    >
      <header class="sticky top-0 z-10 flex items-start justify-between border-b border-line bg-surface-1 p-5">
        <div class="min-w-0">
          <p class="text-xs font-semibold uppercase tracking-[0.14em] text-accent">{{ detail.kind }}</p>
          <h2 id="library-detail-title" class="mt-1 truncate text-lg font-semibold text-slate-100">
            {{ owner }}
          </h2>
          <p class="mt-1 text-xs text-slate-500">{{ date }}</p>
        </div>
        <button
          ref="closeButton"
          type="button"
          class="btn-secondary !px-3"
          aria-label="Close library detail"
          @click="emit('close')"
        >
          Close
        </button>
      </header>

      <div class="space-y-6 p-5">
        <section>
          <h3 class="text-xs font-semibold uppercase tracking-wide text-slate-500">Caption</h3>
          <p class="mt-2 whitespace-pre-wrap text-sm leading-6 text-slate-300">
            {{ detail.caption || "No caption" }}
          </p>
        </section>

        <section>
          <h3 class="text-xs font-semibold uppercase tracking-wide text-slate-500">Sources</h3>
          <div class="mt-2 rounded-lg border border-dashed border-line px-3 py-2 text-sm text-slate-500">
            Available in a future update
          </div>
        </section>

        <section>
          <div class="flex items-baseline justify-between">
            <h3 class="text-xs font-semibold uppercase tracking-wide text-slate-500">Files</h3>
            <span class="text-xs text-slate-500">{{ detail.files.length }} total</span>
          </div>
          <div class="mt-2 space-y-2">
            <article
              v-for="file in detail.files"
              :key="file.id"
              class="rounded-lg border border-line bg-surface-2 p-3"
              :data-library-file-id="file.id"
            >
              <div class="flex flex-wrap items-center justify-between gap-3">
                <div>
                  <p class="text-sm font-medium capitalize text-slate-200">
                    {{ file.kind }} {{ file.ordinal + 1 }}
                  </p>
                  <p class="mt-1 text-xs text-slate-500">{{ formatBytes(file.byte_size) }}</p>
                  <p v-if="!file.exists_on_disk" class="mt-1 text-xs font-medium text-warn">Missing</p>
                </div>
                <div class="flex gap-2">
                  <button
                    type="button"
                    class="btn-secondary !px-3 !py-1.5 text-xs"
                    data-action="open-file"
                    :disabled="!file.exists_on_disk || busyAction !== null"
                    @click="runAction('open', file)"
                  >
                    Open file
                  </button>
                  <button
                    type="button"
                    class="btn-secondary !px-3 !py-1.5 text-xs"
                    data-action="reveal-file"
                    :disabled="!file.exists_on_disk || busyAction !== null"
                    @click="runAction('reveal', file)"
                  >
                    Show in folder
                  </button>
                </div>
              </div>
            </article>
          </div>
        </section>

        <p
          v-if="actionError"
          role="alert"
          class="rounded-lg border border-err/40 bg-err/10 px-3 py-2 text-sm text-err"
        >
          {{ actionError }}
        </p>
      </div>
    </section>
  </div>
</template>
