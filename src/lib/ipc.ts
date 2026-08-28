import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface ConfigState {
  has_token: boolean;
  token_hint: string | null;
  dest_dir: string;
  sidecar: boolean;
  catalog_warning?: string;
}

export interface Balance {
  requests: number;
  rate: number | null;
  amount: number | null;
  currency: string | null;
}

export type MediaItemKind = "post" | "reel" | "story" | "avatar";
export type MediaFileKind = "photo" | "video" | "metadata" | "unknown";
export type FileAvailability = "available" | "missing";
export type LibrarySort = "taken_at_desc" | "imported_at_desc";

export interface LibraryRoot {
  id: number;
  path: string;
  label: string;
  created_at: number;
  last_scan_started_at: number | null;
  last_scan_completed_at: number | null;
}

export interface LibraryQuery {
  search: string | null;
  kinds: MediaItemKind[];
  source_id: number | null;
  availability: FileAvailability | null;
  taken_after: number | null;
  taken_before: number | null;
  sort: LibrarySort;
  cursor: string | null;
  limit: number;
}

export interface LibraryCard {
  id: number;
  kind: MediaItemKind;
  shortcode: string | null;
  owner_username: string | null;
  taken_at: number | null;
  caption: string | null;
  imported_at: number;
  updated_at: number;
  preview_file_id: number | null;
  preview_file_kind: MediaFileKind | null;
  resource_count: number;
  availability: FileAvailability;
}

export interface LibraryPage {
  items: LibraryCard[];
  next_cursor: string | null;
}

export interface LibraryFile {
  id: number;
  ordinal: number;
  kind: MediaFileKind;
  byte_size: number;
  mtime: number;
  exists_on_disk: boolean;
  last_seen_at: number;
}

export interface LibraryItemDetail {
  id: number;
  kind: MediaItemKind;
  remote_pk: string | null;
  shortcode: string | null;
  owner_pk: string | null;
  owner_username: string | null;
  taken_at: number | null;
  caption: string | null;
  like_count: number | null;
  comment_count: number | null;
  imported_at: number;
  updated_at: number;
  files: LibraryFile[];
  source_ids: number[];
}

export interface ScanSummary {
  imported: number;
  updated: number;
  missing: number;
  warnings: number;
}

export type LibraryScanProgress =
  | {
      state: "scanning";
      scan_id: string;
      root_id: number;
      discovered: number;
      processed: number;
      warnings: number;
    }
  | {
      state: "done";
      scan_id: string;
      root_id: number;
      summary: ScanSummary;
    }
  | {
      state: "failed";
      scan_id: string;
      root_id: number;
      error: string;
    }
  | {
      state: "cancelled";
      scan_id: string;
      root_id: number;
      summary: ScanSummary;
    };

export async function configState(): Promise<ConfigState> {
  return invoke("config_state");
}

export async function validateToken(token: string): Promise<Balance> {
  return invoke("validate_token", { token });
}

export async function getBalance(): Promise<Balance> {
  return invoke("get_balance");
}

export async function ensureConfiguredLibraryRoot(): Promise<LibraryRoot> {
  return invoke("ensure_configured_library_root");
}

export async function listLibraryRoots(): Promise<LibraryRoot[]> {
  return invoke("list_library_roots");
}

export async function startLibraryScan(rootId: number): Promise<string> {
  return invoke("start_library_scan", { rootId });
}

export async function cancelLibraryScan(scanId: string): Promise<boolean> {
  return invoke("cancel_library_scan", { scanId });
}

export async function queryLibrary(query: LibraryQuery): Promise<LibraryPage> {
  return invoke("query_library", { query });
}

export async function getLibraryItem(id: number): Promise<LibraryItemDetail> {
  return invoke("get_library_item", { id });
}

export async function openLibraryFile(fileId: number): Promise<void> {
  return invoke("open_library_file", { fileId });
}

export async function revealLibraryFile(fileId: number): Promise<void> {
  return invoke("reveal_library_file", { fileId });
}

export async function onLibraryScanProgress(
  cb: (event: LibraryScanProgress) => void,
): Promise<() => void> {
  const unlisten = await listen<LibraryScanProgress>("library-scan-progress", (event) =>
    cb(event.payload),
  );
  return unlisten;
}

export function libraryMediaUrl(fileId: number): string {
  const mediaBase = convertFileSrc("media", "library").replace(/\/$/, "");
  return `${mediaBase}/${fileId}`;
}

export async function saveSettings(opts: { dest_dir?: string; sidecar?: boolean }): Promise<ConfigState> {
  return invoke("save_settings", { destDir: opts.dest_dir, sidecar: opts.sidecar });
}

export interface JobProgress {
  job_id: string;
  state: "fetching" | "downloading" | "done" | "failed" | "cancelled";
  label: string;
  current_file?: number;
  total_files?: number;
  bytes_done?: number;
  file_name?: string;
  error?: string;
  count?: number;
  dir?: string;
  catalog_warnings?: number;
  resource_failures?: number;
}

export type Target =
  | { kind: "profile"; username: string }
  | { kind: "post"; code: string };

export interface Post {
  pk: string;
  code: string;
  taken_at?: number;
  caption?: string;
  owner_username?: string;
  resources: { url: string; kind: "photo" | "video" }[];
  thumbnail_url?: string;
}

export interface Profile {
  pk: string;
  username: string;
  full_name?: string;
  media_count: number;
  follower_count?: number;
  is_private: boolean;
  is_verified: boolean;
  avatar_url?: string;
}

export interface ProfilePreview {
  profile: Profile;
  recent_posts: Post[];
  end_cursor: string | null;
}

export interface SearchUser {
  pk: string;
  username: string;
  full_name?: string;
  is_verified: boolean;
  is_private: boolean;
  avatar_url?: string;
}

export interface StoryItem {
  pk: string;
  taken_at?: number;
  kind: "photo" | "video";
  media_url: string;
  thumb_url?: string;
}

export interface DirectItem {
  url: string;
  taken_at?: number;
  pk: string;
}

export interface ProfileOptions {
  posts: boolean;
  reels: boolean;
  stories: boolean;
  highlights: boolean;
  avatar: boolean;
  max_posts?: number | null;
}

export async function resolveInput(input: string): Promise<Target> {
  return invoke("resolve_input", { input });
}

export async function fetchProfile(username: string, endCursor?: string | null): Promise<ProfilePreview> {
  return invoke("fetch_profile", { username, endCursor: endCursor ?? null });
}

export async function searchUsers(query: string): Promise<SearchUser[]> {
  return invoke("search_users", { query });
}

export async function fetchStories(username: string): Promise<StoryItem[]> {
  return invoke("fetch_stories", { username });
}

export async function downloadDirect(
  label: string,
  subfolder: string,
  items: DirectItem[],
): Promise<string> {
  return invoke("download_direct", { label, subfolder, items });
}

export async function downloadPost(code: string): Promise<string> {
  return invoke("download_post", { code });
}

export async function enqueueProfileDownload(username: string, opts: ProfileOptions): Promise<string> {
  return invoke("enqueue_profile_download", { username, opts });
}

export async function cancelJob(jobId: string): Promise<boolean> {
  return invoke("cancel_job", { jobId });
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
