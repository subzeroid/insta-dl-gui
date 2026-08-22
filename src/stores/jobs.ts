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
}

export const useJobsStore = defineStore("jobs", () => {
  const jobs = reactive(new Map<string, JobView>());
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

  return { jobs, init, cancel, clearFinished };
});
