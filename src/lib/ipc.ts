import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface ConfigState {
  has_token: boolean;
  token_hint: string | null;
  dest_dir: string;
  sidecar: boolean;
}

export interface Balance {
  requests: number;
  rate: number | null;
  amount: number | null;
  currency: string | null;
}

export async function configState(): Promise<ConfigState> {
  return invoke("config_state");
}

export async function validateToken(token: string): Promise<Balance> {
  return invoke("validate_token", { token });
}

export async function getBalance(): Promise<Balance> {
  return invoke("get_balance");
}

export async function saveSettings(opts: { dest_dir?: string; sidecar?: boolean }): Promise<ConfigState> {
  return invoke("save_settings", { destDir: opts.dest_dir, sidecar: opts.sidecar });
}

export interface JobProgress {
  job_id: string;
  state: "fetching" | "downloading" | "done" | "failed";
  label: string;
  current_file?: number;
  total_files?: number;
  bytes_done?: number;
  file_name?: string;
  error?: string;
  files?: string[];
}

export type Target =
  | { kind: "profile"; username: string }
  | { kind: "post"; code: string };

export async function resolveInput(input: string): Promise<Target> {
  return invoke("resolve_input", { input });
}

export async function downloadPost(code: string): Promise<string> {
  return invoke("download_post", { code });
}

export async function onJobProgress(cb: (p: JobProgress) => void): Promise<() => void> {
  const unlisten = await listen<JobProgress>("job-progress", (e) => cb(e.payload));
  return unlisten;
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 100 ? 0 : 1)} ${units[i]}`;
}

export function formatBalance(b: Balance): string {
  const req = new Intl.NumberFormat("en", { notation: b.requests >= 1_000_000 ? "compact" : "standard" }).format(b.requests);
  const parts = [`${req} req`];
  if (b.amount !== null && b.currency !== null) {
    parts.push(`$${b.amount.toFixed(2)}`);
  }
  if (b.rate !== null) {
    parts.push(`${b.rate} rps`);
  }
  return parts.join(" · ");
}
