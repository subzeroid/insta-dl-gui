/**
 * Dev-only Tauri IPC mock so the UI can run in a plain browser
 * (`?mock=1`) for screenshots and UI work without the Rust backend.
 */

import type {
  DirectItem,
  JobOutputFile,
  JobProgress,
  LibraryCard,
  LibraryFile,
  LibraryItemDetail,
  LibraryPage,
  LibraryQuery,
  LibraryRoot,
  LibraryScanProgress,
  Post,
  ProfileOptions,
} from "./ipc";

type CmdArgs = Record<string, unknown>;
type MockLibraryCard = LibraryCard & { preview_url: string };
type MockEventPayloads = {
  "library-scan-progress": LibraryScanProgress;
  "job-progress": JobProgress;
};

interface MockDownloadManifest {
  label: string;
  dir: string;
  requestedItems?: number;
  outputs: JobOutputFile[];
}

const AVATAR =
  "data:image/svg+xml," +
  encodeURIComponent(
    `<svg xmlns='http://www.w3.org/2000/svg' width='160' height='160'>
       <defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'>
         <stop offset='0' stop-color='#833ab4'/><stop offset='.5' stop-color='#e1306c'/><stop offset='1' stop-color='#f77737'/>
       </linearGradient></defs>
       <rect width='160' height='160' rx='80' fill='url(#g)'/>
       <circle cx='80' cy='64' r='26' fill='#ffffff' opacity='.92'/>
       <ellipse cx='80' cy='128' rx='44' ry='30' fill='#ffffff' opacity='.92'/>
     </svg>`,
  );

function libraryPreview(label: string, start: string, end: string): string {
  return (
    "data:image/svg+xml," +
    encodeURIComponent(
      `<svg xmlns='http://www.w3.org/2000/svg' width='480' height='600'>
         <defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'>
           <stop offset='0' stop-color='${start}'/><stop offset='1' stop-color='${end}'/>
         </linearGradient></defs>
         <rect width='480' height='600' fill='url(#g)'/>
         <circle cx='380' cy='120' r='95' fill='#fff' opacity='.12'/>
         <text x='32' y='548' fill='#fff' font-family='system-ui' font-size='34'>${label}</text>
       </svg>`,
    )
  );
}

const LIBRARY_ROOT: LibraryRoot = {
  id: 1,
  path: "/mock/instagram-archive",
  label: "Instagram archive",
  created_at: 1_776_000_000,
  last_scan_started_at: 1_776_000_100,
  last_scan_completed_at: 1_776_000_120,
};

const FIRST_SCAN_LIBRARY_ROOT: LibraryRoot = {
  ...LIBRARY_ROOT,
  last_scan_started_at: null,
  last_scan_completed_at: null,
};

const LIBRARY_CARDS: MockLibraryCard[] = [
  {
    id: 1,
    kind: "post",
    shortcode: "MOCKPHOTO",
    owner_username: "natgeo",
    taken_at: 1_776_000_000,
    caption: "A quiet ridge at sunrise",
    imported_at: 1_776_100_000,
    updated_at: 1_776_100_000,
    preview_file_id: 101,
    preview_file_kind: "photo",
    resource_count: 1,
    availability: "available",
    preview_url: libraryPreview("PHOTO", "#14532d", "#0f766e"),
  },
  {
    id: 2,
    kind: "reel",
    shortcode: "MOCKVIDEO",
    owner_username: "instagram",
    taken_at: 1_775_900_000,
    caption: "City lights after rain",
    imported_at: 1_776_100_020,
    updated_at: 1_776_100_010,
    preview_file_id: 201,
    preview_file_kind: "photo",
    resource_count: 1,
    availability: "available",
    preview_url: libraryPreview("VIDEO", "#312e81", "#be185d"),
  },
  {
    id: 3,
    kind: "post",
    shortcode: "MOCKCAROUSEL",
    owner_username: "design",
    taken_at: 1_775_800_000,
    caption: "Three studies in color",
    imported_at: 1_776_100_020,
    updated_at: 1_776_100_020,
    preview_file_id: 301,
    preview_file_kind: "photo",
    resource_count: 3,
    availability: "available",
    preview_url: libraryPreview("CAROUSEL · 3", "#7c2d12", "#c2410c"),
  },
  {
    id: 4,
    kind: "story",
    shortcode: null,
    owner_username: "archive",
    taken_at: 1_775_700_000,
    caption: "An archived story whose file moved",
    imported_at: 1_776_100_030,
    updated_at: 1_776_100_030,
    preview_file_id: null,
    preview_file_kind: null,
    resource_count: 1,
    availability: "missing",
    preview_url: libraryPreview("MISSING", "#374151", "#111827"),
  },
];

function mockFiles(card: LibraryCard): LibraryFile[] {
  const count = Math.max(1, card.resource_count);
  return Array.from({ length: count }, (_, ordinal) => ({
    id: card.id * 100 + ordinal + 1,
    ordinal,
    kind: card.id === 2 ? "video" : "photo",
    byte_size: card.availability === "missing" ? 0 : 1_500_000 + ordinal * 250_000,
    mtime: 1_776_100_000 + ordinal,
    exists_on_disk: card.availability === "available",
    last_seen_at: 1_776_100_100,
  }));
}

function mockDetail(card: LibraryCard): LibraryItemDetail {
  return {
    id: card.id,
    kind: card.kind,
    remote_pk: `mock-${card.id}`,
    shortcode: card.shortcode,
    owner_pk: `owner-${card.id}`,
    owner_username: card.owner_username,
    taken_at: card.taken_at,
    caption: card.caption,
    like_count: card.id * 120,
    comment_count: card.id * 7,
    imported_at: card.imported_at,
    updated_at: card.updated_at,
    files: mockFiles(card),
    source_ids: [],
  };
}

function isLibraryFirstScanDemo(): boolean {
  return (
    new URLSearchParams(window.location.search).get("demo") === "library-first-scan"
  );
}

function mockLibraryRoot(): LibraryRoot {
  return isLibraryFirstScanDemo() ? FIRST_SCAN_LIBRARY_ROOT : LIBRARY_ROOT;
}

function mockLibraryCards(): MockLibraryCard[] {
  return isLibraryFirstScanDemo() ? [] : LIBRARY_CARDS;
}

function mockLibraryPage(query: LibraryQuery): LibraryPage {
  const search = query.search?.trim().toLocaleLowerCase() ?? "";
  const items = mockLibraryCards().filter((card) => {
    if (query.kinds.length > 0 && !query.kinds.includes(card.kind)) return false;
    if (query.availability !== null && card.availability !== query.availability) return false;
    if (query.taken_after !== null && (card.taken_at ?? 0) < query.taken_after) return false;
    if (query.taken_before !== null && (card.taken_at ?? 0) > query.taken_before) return false;
    if (!search) return true;
    return [card.owner_username, card.shortcode, card.caption]
      .some((value) => value?.toLocaleLowerCase().includes(search));
  });
  items.sort((left, right) => {
    const leftTimestamp =
      query.sort === "taken_at_desc" ? (left.taken_at ?? left.imported_at) : left.imported_at;
    const rightTimestamp =
      query.sort === "taken_at_desc" ? (right.taken_at ?? right.imported_at) : right.imported_at;
    return rightTimestamp - leftTimestamp || right.id - left.id;
  });
  return { items: items.slice(0, query.limit), next_cursor: null };
}

function safeBasenameSegment(value: unknown, fallback: string): string {
  const safe = String(value ?? "")
    .trim()
    .replace(/[^A-Za-z0-9._-]+/g, "_")
    .replace(/^\.+/, "")
    .replace(/_+$/g, "");
  return safe || fallback;
}

function mockOutput(
  jobNumber: number,
  outputIndex: number,
  stem: string,
  kind: "photo" | "video",
  ordinal: number,
): JobOutputFile {
  return {
    file_id: 10_000 + jobNumber * 100 + outputIndex + 1,
    basename: `${safeBasenameSegment(stem, "media")}_${ordinal + 1}.${kind === "video" ? "mp4" : "jpg"}`,
    kind,
    byte_size: kind === "video" ? 2_000_000 : 1_500_000,
    ordinal,
  };
}

function fetchedPostManifest(args: CmdArgs | undefined, jobNumber: number): MockDownloadManifest {
  const username = safeBasenameSegment(args?.username, "instagram");
  const category = args?.category === "reels" ? "reels" : "posts";
  const scope = args?.scope === "selected" ? "selected" : "shown";
  const posts = Array.isArray(args?.posts) ? (args.posts as Post[]) : [];
  let outputIndex = 0;
  const outputs = posts.flatMap((post, postIndex) => {
    const stem = safeBasenameSegment(post.code, `post-${postIndex + 1}`);
    return post.resources.map((resource, ordinal) =>
      mockOutput(jobNumber, outputIndex++, stem, resource.kind, ordinal),
    );
  });
  return {
    label: `@${username} ${category} · ${scope} · ${posts.length}`,
    dir: `/mock/instagram-archive/${username}/${category}`,
    requestedItems: posts.length,
    outputs,
  };
}

function directManifest(args: CmdArgs | undefined, jobNumber: number): MockDownloadManifest {
  const label = safeBasenameSegment(args?.label, "instagram");
  const subfolder = safeBasenameSegment(args?.subfolder, "media");
  const items = Array.isArray(args?.items) ? (args.items as DirectItem[]) : [];
  const outputs = items.map((item, outputIndex) => {
    const kind = /\.(?:mp4|mov|webm)(?:[?#]|$)/i.test(item.url) ? "video" : "photo";
    return mockOutput(
      jobNumber,
      outputIndex,
      safeBasenameSegment(item.pk, `media-${outputIndex + 1}`),
      kind,
      0,
    );
  });
  return {
    label: `@${label} ${subfolder}`,
    dir: `/mock/instagram-archive/${label}/${subfolder}`,
    requestedItems: items.length,
    outputs,
  };
}

function standalonePostManifest(args: CmdArgs | undefined, jobNumber: number): MockDownloadManifest {
  const code = safeBasenameSegment(args?.code, "post");
  return {
    label: `Post ${code}`,
    dir: "/mock/instagram-archive/posts",
    requestedItems: 1,
    outputs: [mockOutput(jobNumber, 0, code, "photo", 0)],
  };
}

function profileManifest(args: CmdArgs | undefined, jobNumber: number): MockDownloadManifest {
  const username = safeBasenameSegment(args?.username, "instagram");
  const opts = (args?.opts ?? {}) as Partial<ProfileOptions>;
  const kinds: Array<{ suffix: string; kind: "photo" | "video" }> = [];
  if (opts.posts) kinds.push({ suffix: "post", kind: "photo" });
  if (opts.reels) kinds.push({ suffix: "reel", kind: "video" });
  if (opts.stories) kinds.push({ suffix: "story", kind: "photo" });
  if (opts.highlights) kinds.push({ suffix: "highlight", kind: "video" });
  if (opts.avatar) kinds.push({ suffix: "avatar", kind: "photo" });
  if (kinds.length === 0) kinds.push({ suffix: "post", kind: "photo" });
  return {
    label: `@${username} archive`,
    dir: `/mock/instagram-archive/${username}`,
    outputs: kinds.map(({ suffix, kind }, outputIndex) =>
      mockOutput(jobNumber, outputIndex, `${username}_${suffix}`, kind, 0),
    ),
  };
}

function downloadManifest(
  cmd: string,
  args: CmdArgs | undefined,
  jobNumber: number,
): MockDownloadManifest {
  switch (cmd) {
    case "enqueue_fetched_post_download":
      return fetchedPostManifest(args, jobNumber);
    case "download_direct":
      return directManifest(args, jobNumber);
    case "download_post":
      return standalonePostManifest(args, jobNumber);
    case "enqueue_profile_download":
      return profileManifest(args, jobNumber);
    default:
      throw new Error(`mock download: unhandled command "${cmd}"`);
  }
}

function reply(cmd: string, args?: CmdArgs): unknown {
  switch (cmd) {
    case "config_state":
      return {
        has_token: true,
        token_hint: "***9f3a",
        dest_dir: "~/Downloads/insta-dl",
        sidecar: true,
      };
    case "get_balance":
    case "__balance":
      return { requests: 14_700_000, rate: 10, amount: 123.45, currency: "usd" };
    case "ensure_configured_library_root":
      return mockLibraryRoot();
    case "list_library_roots":
      return [mockLibraryRoot()];
    case "start_library_scan":
      return "mock-library-scan";
    case "cancel_library_scan":
      return true;
    case "query_library":
      return mockLibraryPage(args?.query as LibraryQuery);
    case "get_library_item": {
      const id = Number(args?.id);
      const card = mockLibraryCards().find((candidate) => candidate.id === id);
      if (!card) throw new Error("Library item was not found");
      return mockDetail(card);
    }
    case "request_library_preview_access":
      return Number.isInteger(args?.fileId) && Number(args?.fileId) > 0;
    case "open_library_file":
    case "reveal_library_file":
      if (!Number.isInteger(args?.fileId) || Number(args?.fileId) <= 0) {
        throw new Error("Library file is unavailable");
      }
      return null;
    case "resolve_input": {
      const input = String(args?.input ?? "");
      if (/instagram\.com\/(p|reel|reels|tv)\//.test(input)) {
        return { kind: "post", code: "DXZlTiKEpxw" };
      }
      const username = input.replace(/^@/, "").trim() || "instagram";
      return { kind: "profile", username };
    }
    case "fetch_profile": {
      const pageStart = args?.endCursor ? 12 : 0;
      return {
        profile: {
          pk: "25025320",
          username: String(args?.username ?? "instagram"),
          full_name: "Instagram",
          media_count: 7421,
          follower_count: 713_000_000,
          is_private: false,
          is_verified: true,
          avatar_url: AVATAR,
        },
        recent_posts: Array.from({ length: 12 }, (_, i) => {
          const index = pageStart + i;
          const hue = Math.round((index * 137) % 360);
          const thumb =
            "data:image/svg+xml," +
            encodeURIComponent(
              `<svg xmlns='http://www.w3.org/2000/svg' width='400' height='400'>
                 <defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'>
                   <stop offset='0' stop-color='hsl(${hue},60%,30%)'/>
                   <stop offset='1' stop-color='hsl(${(hue + 60) % 360},60%,18%)'/>
                 </linearGradient></defs>
                 <rect width='400' height='400' fill='url(#g)'/>
                 <circle cx='${80 + ((index * 97) % 240)}' cy='${100 + ((index * 61) % 200)}' r='56' fill='#ffffff' opacity='.14'/>
               </svg>`,
            );
          const isVideo = index % 3 === 0;
          return {
            pk: `p${index}`,
            code: `DEMO${index}`,
            caption: `Demo post #${index} — golden hour somewhere far away`,
            like_count: 1200 * (24 - index),
            comment_count: 40 + index,
            taken_at: 1776000000 + index * 86400,
            owner_username: "natgeo",
            thumbnail_url: thumb,
            resources: [
              { url: "", kind: isVideo ? ("video" as const) : ("photo" as const) },
            ],
          };
        }),
        end_cursor: args?.endCursor ? null : "cursor",
      };
    }
    case "fetch_reels": {
      const pageStart = args?.endCursor ? 11 : 0;
      return {
        posts: Array.from({ length: 11 }, (_, i) => {
          const index = pageStart + i;
          const hue = Math.round((index * 137) % 360);
          const thumbnail =
            "data:image/svg+xml," +
            encodeURIComponent(
              `<svg xmlns='http://www.w3.org/2000/svg' width='400' height='400'>
                 <rect width='400' height='400' fill='hsl(${hue},45%,24%)'/>
                 <text x='28' y='360' fill='white' font-family='system-ui' font-size='28'>REEL ${index + 1}</text>
               </svg>`,
            );
          return {
            pk: `r${index}`,
            code: `REEL${index}`,
            caption: `Demo reel #${index}`,
            taken_at: 1_776_000_000 + index * 86_400,
            owner_username: "natgeo",
            thumbnail_url: thumbnail,
            resources: [{ url: "", kind: "video" as const }],
          };
        }),
        end_cursor: args?.endCursor ? null : "reels-cursor",
      };
    }
    case "search_users": {
      const q = String(args?.query ?? "").toLowerCase();
      const pool = [
        { pk: "25025320", username: "instagram", full_name: "Instagram", is_verified: true, is_private: false, avatar_url: AVATAR },
        { pk: "1234", username: "nike", full_name: "Nike", is_verified: true, is_private: false, avatar_url: AVATAR },
        { pk: "5678", username: "nikelife", full_name: "Nike Life", is_verified: false, is_private: false, avatar_url: AVATAR },
        { pk: "9012", username: "nikita.runs", full_name: "Nikita", is_verified: false, is_private: false, avatar_url: AVATAR },
        { pk: "3456", username: "nikolciaak", full_name: "Nikol", is_verified: false, is_private: true, avatar_url: AVATAR },
      ];
      return pool.filter((u) => u.username.includes(q.replace(/^@/, "")));
    }
    case "fetch_stories":
      return [
        { pk: "s1", taken_at: 1776787455, kind: "photo", media_url: "mock://stories/s1.jpg", thumb_url: "" },
        { pk: "s2", taken_at: 1776787500, kind: "video", media_url: "mock://stories/s2.mp4", thumb_url: "" },
        { pk: "s3", taken_at: 1776787600, kind: "photo", media_url: "mock://stories/s3.jpg", thumb_url: "" },
      ];
    case "validate_token":
      return reply("__balance");
    case "save_settings":
      return reply("config_state");
    // event plugin stubs
    case "plugin:event|listen":
      return 1;
    case "plugin:event|unlisten":
      return null;
    case "plugin:opener|open_url":
      return null;
    case "plugin:clipboard-manager|write_text":
      return null;
    default:
      throw new Error(`mock ipc: unhandled command "${cmd}"`);
  }
}

export function installTauriMock(): void {
  const w = window as unknown as Record<string, unknown>;
  const callbacks = new Map<number, (data: unknown) => void>();
  const listeners = new Map<string, number[]>();
  const activeJobs = new Map<
    string,
    { label: string; timers: Array<ReturnType<typeof setTimeout>> }
  >();
  let nextCallbackId = 1;
  let nextJobNumber = 1;

  function unregisterCallback(id: number) {
    callbacks.delete(id);
  }

  function transformCallback(callback?: (data: unknown) => void, once = false): number {
    const id = nextCallbackId++;
    callbacks.set(id, (data) => {
      if (once) unregisterCallback(id);
      callback?.(data);
    });
    return id;
  }

  function runCallback(id: number, data: unknown) {
    callbacks.get(id)?.(data);
  }

  function emit<EventName extends keyof MockEventPayloads>(
    event: EventName,
    payload: MockEventPayloads[EventName],
  ) {
    for (const handler of listeners.get(event) ?? []) {
      runCallback(handler, { event, id: handler, payload });
    }
  }

  function enqueueDownload(cmd: string, args?: CmdArgs): string {
    const jobNumber = nextJobNumber++;
    const jobId = `mock-job-${jobNumber}`;
    const manifest = downloadManifest(cmd, args, jobNumber);
    const active = { label: manifest.label, timers: [] as Array<ReturnType<typeof setTimeout>> };
    activeJobs.set(jobId, active);
    active.timers.push(
      setTimeout(() => {
        if (!activeJobs.has(jobId)) return;
        const first = manifest.outputs[0];
        emit("job-progress", {
          job_id: jobId,
          state: "downloading",
          label: manifest.label,
          current_file: first ? 1 : 0,
          total_files: manifest.outputs.length,
          bytes_done: first ? Math.max(1, Math.floor(first.byte_size / 2)) : 0,
          file_name: first?.basename ?? "Preparing download",
        });
      }, 10),
      setTimeout(() => {
        if (!activeJobs.delete(jobId)) return;
        const done: JobProgress = {
          job_id: jobId,
          state: "done",
          label: manifest.label,
          count: manifest.outputs.length,
          dir: manifest.dir,
          catalog_warnings: 0,
          resource_failures: 0,
          outputs: manifest.outputs.map((output) => ({ ...output })),
        };
        if (manifest.requestedItems !== undefined) {
          done.requested_items = manifest.requestedItems;
        }
        emit("job-progress", done);
      }, 900),
    );
    return jobId;
  }

  function cancelDownload(jobId: string): boolean {
    const active = activeJobs.get(jobId);
    if (!active) return false;
    activeJobs.delete(jobId);
    for (const timer of active.timers) clearTimeout(timer);
    emit("job-progress", {
      job_id: jobId,
      state: "cancelled",
      label: active.label,
    });
    return true;
  }

  function emitScan(rootId: number) {
    const emptyLibraryDemo = isLibraryFirstScanDemo();
    emit("library-scan-progress", {
      state: "scanning",
      scan_id: "mock-library-scan",
      root_id: rootId,
      discovered: emptyLibraryDemo ? 0 : 4,
      processed: emptyLibraryDemo ? 0 : 2,
      warnings: 0,
    });
    emit("library-scan-progress", {
      state: "done",
      scan_id: "mock-library-scan",
      root_id: rootId,
      summary: {
        imported: emptyLibraryDemo ? 0 : 4,
        updated: 0,
        missing: emptyLibraryDemo ? 0 : 1,
        warnings: 0,
      },
    });
  }

  w.__TAURI_INTERNALS__ = {
    invoke: (cmd: string, args?: CmdArgs) => {
      if (cmd === "plugin:event|listen") {
        const event = String(args?.event);
        const handler = Number(args?.handler);
        listeners.set(event, [...(listeners.get(event) ?? []), handler]);
        return Promise.resolve(handler);
      }
      if (cmd === "plugin:event|unlisten") {
        const event = String(args?.event);
        const eventId = Number(args?.eventId);
        listeners.set(
          event,
          (listeners.get(event) ?? []).filter((id) => id !== eventId),
        );
        return Promise.resolve(null);
      }
      if (
        cmd === "download_direct" ||
        cmd === "download_post" ||
        cmd === "enqueue_fetched_post_download" ||
        cmd === "enqueue_profile_download"
      ) {
        const response = Promise.resolve().then(() => enqueueDownload(cmd, args));
        return response;
      }
      if (cmd === "cancel_job") {
        return Promise.resolve(cancelDownload(String(args?.jobId ?? "")));
      }
      try {
        const response = Promise.resolve(reply(cmd, args));
        if (cmd === "start_library_scan") {
          void response.then(() => {
            queueMicrotask(() => emitScan(Number(args?.rootId)));
          });
        }
        return response;
      } catch (e) {
        return Promise.reject(e);
      }
    },
    transformCallback,
    unregisterCallback,
    runCallback,
    callbacks,
    convertFileSrc: (path: string, protocol = "asset") =>
      `${protocol}://localhost${path.startsWith("/") ? path : `/${path}`}`,
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    plugins: {},
  };
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (event: string, eventId: number) => {
      unregisterCallback(eventId);
      listeners.set(
        event,
        (listeners.get(event) ?? []).filter((id) => id !== eventId),
      );
    },
  };
}

export function isMockMode(): boolean {
  return new URLSearchParams(window.location.search).has("mock");
}

export function isDemoProfile(): boolean {
  return new URLSearchParams(window.location.search).get("demo") === "profile";
}
