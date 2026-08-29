import { defineStore } from "pinia";
import { reactive } from "vue";
import { cancelJob, onJobProgress, type JobProgress } from "../lib/ipc";

export interface JobView {
  id: string;
  label: string;
  state: JobProgress["state"];
  currentFile: number;
  totalFiles: number;
  bytesDone: number;
  fileName: string;
  error?: string;
  resultCount?: number;
  resultDir?: string;
  catalogWarnings: number;
  resourceFailures: number;
  conflictKeys?: string[];
}

export const useJobsStore = defineStore("jobs", () => {
  const jobs = reactive(new Map<string, JobView>());
  const conflictReservations = reactive(new Map<symbol, string[]>());
  let started = false;

  function apply(p: JobProgress) {
    const existing = jobs.get(p.job_id);
    const job =
      existing ??
      reactive({
        id: p.job_id,
        label: p.label,
        state: "downloading" as JobProgress["state"],
        currentFile: 0,
        totalFiles: 0,
        bytesDone: 0,
        fileName: "",
        error: undefined,
        resultCount: undefined,
        resultDir: undefined,
        catalogWarnings: 0,
        resourceFailures: 0,
        conflictKeys: [],
      });
    if (!existing) jobs.set(p.job_id, job);
    job.state = p.state;
    if (p.state === "downloading") {
      job.currentFile = p.current_file ?? job.currentFile;
      job.totalFiles = p.total_files ?? job.totalFiles;
      job.bytesDone = Math.max(job.bytesDone, p.bytes_done ?? 0);
      job.fileName = p.file_name ?? job.fileName;
    }
    if (p.state === "done") {
      job.resultCount = p.count ?? 0;
      job.resultDir = p.dir;
      job.catalogWarnings = p.catalog_warnings ?? job.catalogWarnings;
      job.resourceFailures = p.resource_failures ?? job.resourceFailures;
    }
    if (p.state === "failed") {
      job.error = p.error;
    }
  }

  async function init() {
    if (started) return;
    started = true;
    await onJobProgress(apply);
  }

  /** Insert a card before the first backend event arrives so the job is
      visible immediately (merge-on-event by id keeps it consistent). */
  function addPlaceholder(id: string, label: string, conflictKeys: readonly string[] = []) {
    const normalizedKeys = [...new Set(conflictKeys)];
    const existing = jobs.get(id);
    if (existing) {
      existing.conflictKeys = [...new Set([...(existing.conflictKeys ?? []), ...normalizedKeys])];
    } else {
      jobs.set(id, reactive({
        id,
        label,
        state: "fetching",
        currentFile: 0,
        totalFiles: 0,
        bytesDone: 0,
        fileName: "",
        error: undefined,
        resultCount: undefined,
        resultDir: undefined,
        catalogWarnings: 0,
        resourceFailures: 0,
        conflictKeys: normalizedKeys,
      }));
    }
  }

  function transferConflictReservation(
    token: symbol,
    id: string,
    label: string,
    conflictKeys: readonly string[] = [],
  ) {
    const reservedKeys = conflictReservations.get(token) ?? [];
    addPlaceholder(id, label, [...reservedKeys, ...conflictKeys]);
    conflictReservations.delete(token);
  }

  function hasActiveConflict(conflictKeys: readonly string[]): boolean {
    if (conflictKeys.length === 0) return false;
    const requested = new Set(conflictKeys);
    for (const job of jobs.values()) {
      if (
        (job.state === "fetching" || job.state === "downloading") &&
        (job.conflictKeys ?? []).some((key) => requested.has(key))
      ) {
        return true;
      }
    }
    for (const reservedKeys of conflictReservations.values()) {
      if (reservedKeys.some((key) => requested.has(key))) return true;
    }
    return false;
  }

  function reserveConflictKeys(token: symbol, conflictKeys: readonly string[]): boolean {
    if (conflictReservations.has(token)) return false;
    const normalizedKeys = [...new Set(conflictKeys)];
    if (hasActiveConflict(normalizedKeys)) return false;
    conflictReservations.set(token, normalizedKeys);
    return true;
  }

  function releaseConflictKeys(token: symbol) {
    conflictReservations.delete(token);
  }

  async function cancel(id: string) {
    await cancelJob(id);
  }

  function clearFinished() {
    for (const [id, job] of jobs) {
      if (job.state === "done" || job.state === "failed" || job.state === "cancelled") {
        jobs.delete(id);
      }
    }
  }

  return {
    jobs,
    init,
    addPlaceholder,
    transferConflictReservation,
    hasActiveConflict,
    reserveConflictKeys,
    releaseConflictKeys,
    cancel,
    clearFinished,
  };
});
