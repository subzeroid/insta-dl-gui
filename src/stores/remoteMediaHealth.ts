import { defineStore } from "pinia";
import { ref } from "vue";

export const REMOTE_MEDIA_FAILURE_WINDOW_MS = 10_000;

export function createRemoteMediaHealthStore(now: () => number = Date.now) {
  return defineStore("remoteMediaHealth", () => {
    const bannerVisible = ref(false);
    const dismissed = ref(false);
    const retryGeneration = ref(0);
    const failures = new Map<string, number>();

    function normalizeSource(source: string): string {
      return source.trim();
    }

    function pruneFailures(at: number) {
      const cutoff = at - REMOTE_MEDIA_FAILURE_WINDOW_MS;
      for (const [source, failedAt] of failures) {
        if (failedAt < cutoff) failures.delete(source);
      }
    }

    function reportFailure(source: string) {
      if (dismissed.value) return;
      const normalized = normalizeSource(source);
      if (!normalized) return;
      const at = now();
      pruneFailures(at);
      failures.set(normalized, at);
      if (failures.size >= 2) bannerVisible.value = true;
    }

    function reportSuccess(source: string) {
      const normalized = normalizeSource(source);
      if (!normalized) return;
      failures.delete(normalized);
    }

    function retryAll() {
      failures.clear();
      bannerVisible.value = false;
      retryGeneration.value += 1;
    }

    function dismiss() {
      failures.clear();
      bannerVisible.value = false;
      dismissed.value = true;
    }

    return {
      bannerVisible,
      dismissed,
      retryGeneration,
      reportFailure,
      reportSuccess,
      retryAll,
      dismiss,
    };
  });
}

export const useRemoteMediaHealthStore = createRemoteMediaHealthStore();
