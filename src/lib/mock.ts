/**
 * Dev-only Tauri IPC mock so the UI can run in a plain browser
 * (`?mock=1`) for screenshots and UI work without the Rust backend.
 */

import type {
  DirectItem,
  ConfigState,
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
  RelationshipKind,
  SearchUser,
} from "./ipc";

type CmdArgs = Record<string, unknown>;
type MockLibraryCard = LibraryCard & { preview_url: string };
type MockWindow = Record<PropertyKey, unknown>;
type DownloadMediaKind = "photo" | "video";
type FileIdAllocator = (kind: DownloadMediaKind) => number;
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

const MOCK_DISPOSER = Symbol("insta-dl-gui-tauri-mock-disposer");
const MOCK_LIBRARY_MEDIA_URL_RESOLVER = Symbol.for(
  "insta-dl-gui.mock-library-media-url-resolver",
);
const MOCK_REMOTE_MEDIA_URL_RESOLVER = Symbol.for(
  "insta-dl-gui.mock-remote-media-url-resolver",
);
const MAX_DOWNLOAD_ITEMS = 500;
const MAX_RESOURCES_PER_POST = 20;
const MAX_SHORTCODE_BYTES = 256;
const ALLOWED_CDN_HOSTS = ["cdninstagram.com", "fbcdn.net"];
const MOCK_PROFILE_PK_START = 9_000_000;
const MOCK_REEL_PK_START = 9_100_000;
const PROXY_VALIDATION_ERROR = "Enter a valid HTTP, HTTPS, SOCKS5, or SOCKS5H proxy URL";

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

const MOCK_VIDEO =
  "data:video/mp4;base64," +
  "AAAAIGZ0eXBpc29tAAACAGlzb21pc28yYXZjMW1wNDEAAAMvbW9vdgAAAGxtdmhkAAAAAAAAAAAAAAAAAAAD6AAAAHgAAQAAAQAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAll0cmFrAAAAXHRraGQAAAADAAAAAAAAAAAAAAABAAAAAAAAAHgAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAABAAAAAABAAAAAQAAAAAAAkZWR0cwAAABxlbHN0AAAAAAAAAAEAAAB4AAAAAAABAAAAAAHRbWRpYQAAACBtZGhkAAAAAAAAAAAAAAAAAAAyAAAABgBVxAAAAAAALWhkbHIAAAAAAAAAAHZpZGUAAAAAAAAAAAAAAABWaWRlb0hhbmRsZXIAAAABfG1pbmYAAAAUdm1oZAAAAAEAAAAAAAAAAAAAACRkaW5mAAAAHGRyZWYAAAAAAAAAAQAAAAx1cmwgAAAAAQAAATxzdGJsAAAAuHN0c2QAAAAAAAAAAQAAAKhhdmMxAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAABAAEABIAAAASAAAAAAAAAABFUxhdmM2Mi4yOC4xMDAgbGlieDI2NAAAAAAAAAAAAAAAGP//AAAALmF2Y0MBQsAe/+EAFmdCwB7ZHsBEAAADAAQAAAMAyDxYuSABAAVoy4PLIAAAABBwYXNwAAAAAQAAAAEAAAAUYnRydAAAAAAAAAiYAAAAAAAAABhzdHRzAAAAAAAAAAEAAAADAAACAAAAABRzdHNzAAAAAAAAAAEAAAABAAAAHHN0c2MAAAAAAAAAAQAAAAEAAAADAAAAAQAAACBzdHN6AAAAAAAAAAAAAAADAAAADgAAAAkAAAAKAAAAFHN0Y28AAAAAAAAAAQAAA18AAABidWR0YQAAAFptZXRhAAAAAAAAACFoZGxyAAAAAAAAAABtZGlyYXBwbAAAAAAAAAAAAAAAAC1pbHN0AAAAJal0b28AAAAdZGF0YQAAAAEAAAAATGF2ZjYyLjEyLjEwMAAAAAhmcmVlAAAAKW1kYXQAAAAKZYiEC/JigACrzgAAAAVBmjgX6gAAAAZBmlQFeoA=";

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

function profilePreview(index: number): string {
  const hue = Math.round((index * 137) % 360);
  return (
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
    )
  );
}

function reelPreview(index: number): string {
  const hue = Math.round((index * 137) % 360);
  return (
    "data:image/svg+xml," +
    encodeURIComponent(
      `<svg xmlns='http://www.w3.org/2000/svg' width='400' height='400'>
         <rect width='400' height='400' fill='hsl(${hue},45%,24%)'/>
         <text x='28' y='360' fill='white' font-family='system-ui' font-size='28'>REEL ${index + 1}</text>
       </svg>`,
    )
  );
}

function mockMediaFixture(kind: DownloadMediaKind): string {
  return kind === "video"
    ? MOCK_VIDEO
    : libraryPreview("PHOTO", "#14532d", "#0f766e");
}

const MOCK_STORIES = [
  {
    pk: "s1",
    taken_at: 1_776_787_455,
    kind: "photo" as const,
    media_url: "",
    thumb_url: libraryPreview("STORY 1", "#7c3aed", "#db2777"),
  },
  {
    pk: "s2",
    taken_at: 1_776_787_500,
    kind: "video" as const,
    media_url: "",
    thumb_url: libraryPreview("STORY 2", "#0f766e", "#2563eb"),
  },
  {
    pk: "s3",
    taken_at: 1_776_787_600,
    kind: "photo" as const,
    media_url: "",
    thumb_url: libraryPreview("STORY 3", "#b45309", "#dc2626"),
  },
];
const MOCK_STORY_KINDS = new Map(MOCK_STORIES.map((story) => [story.pk, story.kind]));

function mockRelationshipUsers(kind: RelationshipKind): SearchUser[] {
  const names = kind === "following"
    ? ["meta", "metaglasses", ...Array.from({ length: 22 }, (_, index) => `following_${index + 3}`)]
    : Array.from({ length: 24 }, (_, index) => `follower_${index + 1}`);
  return names.map((username, index) => ({
    pk: `${(kind === "followers" ? 8_000_000 : 8_100_000) + index}`,
    username,
    full_name: username.replace(/_/g, " ").replace(/^./, (letter: string) => letter.toUpperCase()),
    is_verified: index === 0,
    is_private: index % 7 === 0,
    avatar_url: AVATAR,
  }));
}

const MOCK_REMOTE_MEDIA_FIXTURES = new Set([
  AVATAR,
  MOCK_VIDEO,
  ...MOCK_STORIES.map((story) => story.thumb_url),
  ...Array.from({ length: 24 }, (_, index) => profilePreview(index)),
  ...Array.from({ length: 22 }, (_, index) => reelPreview(index)),
]);

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
    preview_file_kind: "video",
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
  allocateFileId: FileIdAllocator,
  stem: string,
  kind: "photo" | "video",
  ordinal: number,
): JobOutputFile {
  return {
    file_id: allocateFileId(kind),
    basename: `${safeBasenameSegment(stem, "media")}_${ordinal + 1}.${kind === "video" ? "mp4" : "jpg"}`,
    kind,
    byte_size: kind === "video" ? 2_000_000 : 1_500_000,
    ordinal,
  };
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function normalizeUsername(value: unknown): string {
  if (typeof value !== "string") throw new Error("Instagram username must be a string");
  const trimmed = value.trim();
  const username = (trimmed.startsWith("@") ? trimmed.slice(1) : trimmed).toLowerCase();
  if (
    username.length === 0 ||
    username.length > 30 ||
    username === "." ||
    username === ".." ||
    !/^[A-Za-z0-9._]+$/.test(username)
  ) {
    throw new Error(
      "Instagram username must use 1 to 30 ASCII letters, digits, periods, or underscores",
    );
  }
  return username;
}

function isOwnMockFetchedResource(
  post: Record<string, unknown>,
  resource: Record<string, unknown>,
): boolean {
  const numeric = Number(post.pk);
  const profileIndex = numeric - MOCK_PROFILE_PK_START;
  const reelIndex = numeric - MOCK_REEL_PK_START;
  const matchesFixture =
    (Number.isInteger(profileIndex) &&
      profileIndex >= 0 &&
      profileIndex < 100 &&
      post.code === `DEMO${profileIndex}`) ||
    (Number.isInteger(reelIndex) &&
      reelIndex >= 0 &&
      reelIndex < 100 &&
      post.code === `REEL${reelIndex}`);
  return (
    matchesFixture &&
    typeof post.thumbnail_url === "string" &&
    post.thumbnail_url.startsWith("data:image/svg+xml,") &&
    resource.url === ""
  );
}

function isAllowedRemoteUrl(value: unknown): value is string {
  if (typeof value !== "string") return false;
  try {
    const parsed = new URL(value);
    return (
      parsed.protocol === "https:" &&
      ALLOWED_CDN_HOSTS.some(
        (host) => parsed.hostname === host || parsed.hostname.endsWith(`.${host}`),
      )
    );
  } catch {
    return false;
  }
}

function validateFetchedPosts(value: unknown): Post[] {
  if (!Array.isArray(value) || value.length === 0) {
    throw new Error("Fetched post batch must not be empty");
  }
  if (value.length > MAX_DOWNLOAD_ITEMS) {
    throw new Error("Fetched post batch exceeds maximum of 500 posts");
  }
  const seen = new Map<string, string>();
  const validated: Post[] = [];
  for (const candidate of value) {
    if (!isPlainObject(candidate)) throw new Error("Fetched post is malformed");
    if (typeof candidate.pk !== "string" || !/^\d+$/.test(candidate.pk)) {
      throw new Error("Fetched post PK must contain only ASCII digits");
    }
    if (typeof candidate.code !== "string" || candidate.code.length === 0) {
      throw new Error("Fetched post shortcode must not be empty");
    }
    if (new TextEncoder().encode(candidate.code).length > MAX_SHORTCODE_BYTES) {
      throw new Error("Fetched post shortcode exceeds maximum of 256 bytes");
    }
    if (
      !Array.isArray(candidate.resources) ||
      candidate.resources.length === 0 ||
      candidate.resources.length > MAX_RESOURCES_PER_POST
    ) {
      throw new Error("Fetched post must contain between 1 and 20 resources");
    }
    for (const resource of candidate.resources) {
      if (
        !isPlainObject(resource) ||
        (resource.kind !== "photo" && resource.kind !== "video") ||
        (!isAllowedRemoteUrl(resource.url) && !isOwnMockFetchedResource(candidate, resource))
      ) {
        throw new Error("Fetched post contains an invalid media resource");
      }
    }
    const post = candidate as unknown as Post;
    const canonical = JSON.stringify(post);
    const existing = seen.get(post.pk);
    if (existing !== undefined && existing !== canonical) {
      throw new Error("Fetched post batch contains conflicting posts with the same PK");
    }
    if (existing === undefined) {
      seen.set(post.pk, canonical);
      validated.push(post);
    }
  }
  return validated;
}

function validateFetchedArgs(args: CmdArgs | undefined): CmdArgs {
  const username = normalizeUsername(args?.username);
  if (args?.category !== "posts" && args?.category !== "reels") {
    throw new Error("Fetched post category must be posts or reels");
  }
  if (args?.scope !== "shown" && args?.scope !== "selected") {
    throw new Error("Fetched post scope must be shown or selected");
  }
  return { ...args, username, posts: validateFetchedPosts(args?.posts) };
}

function validateDirectArgs(args: CmdArgs | undefined): CmdArgs {
  if (typeof args?.label !== "string" || args.label.trim().length === 0) {
    throw new Error("Download label must not be empty");
  }
  if (typeof args?.subfolder !== "string" || args.subfolder.trim().length === 0) {
    throw new Error("Download subfolder must not be empty");
  }
  if (!Array.isArray(args.items) || args.items.length === 0) {
    throw new Error("Nothing to download");
  }
  if (args.items.length > MAX_DOWNLOAD_ITEMS) {
    throw new Error("Direct download exceeds maximum of 500 items");
  }
  const storyContext = args.subfolder.trim().toLowerCase() === "stories";
  for (const candidate of args.items) {
    if (
      !isPlainObject(candidate) ||
      typeof candidate.pk !== "string" ||
      candidate.pk.trim().length === 0 ||
      typeof candidate.url !== "string" ||
      (candidate.taken_at !== undefined &&
        candidate.taken_at !== null &&
        !Number.isSafeInteger(candidate.taken_at))
    ) {
      throw new Error("Direct download item is malformed");
    }
    const ownStoryFixture = storyContext && MOCK_STORY_KINDS.has(candidate.pk) && candidate.url === "";
    const ownDataFixture = candidate.url === AVATAR;
    if (!ownStoryFixture && !ownDataFixture && !isAllowedRemoteUrl(candidate.url)) {
      throw new Error("Direct download item contains an invalid media URL");
    }
  }
  return { ...args, label: args.label.trim(), subfolder: args.subfolder.trim() };
}

function validateStandaloneArgs(args: CmdArgs | undefined): CmdArgs {
  if (
    typeof args?.code !== "string" ||
    args.code.trim().length === 0 ||
    new TextEncoder().encode(args.code.trim()).length > MAX_SHORTCODE_BYTES
  ) {
    throw new Error("Post shortcode must contain between 1 and 256 bytes");
  }
  return { ...args, code: args.code.trim() };
}

function validateProfileArgs(args: CmdArgs | undefined): CmdArgs {
  const username = normalizeUsername(args?.username);
  if (!isPlainObject(args?.opts)) throw new Error("Profile options are malformed");
  const opts = args.opts;
  const optionKeys = ["posts", "reels", "stories", "highlights", "avatar"] as const;
  if (optionKeys.some((key) => typeof opts[key] !== "boolean")) {
    throw new Error("Profile options are malformed");
  }
  if (!optionKeys.some((key) => opts[key] === true)) {
    throw new Error("Select at least one profile category");
  }
  if (
    opts.max_posts !== undefined &&
    opts.max_posts !== null &&
    (!Number.isSafeInteger(opts.max_posts) || Number(opts.max_posts) <= 0)
  ) {
    throw new Error("Profile max_posts must be a positive integer or null");
  }
  return { ...args, username, opts };
}

function validateDownloadArgs(cmd: string, args: CmdArgs | undefined): CmdArgs {
  switch (cmd) {
    case "enqueue_fetched_post_download":
      return validateFetchedArgs(args);
    case "download_direct":
      return validateDirectArgs(args);
    case "download_post":
      return validateStandaloneArgs(args);
    case "enqueue_profile_download":
      return validateProfileArgs(args);
    default:
      throw new Error(`mock download: unhandled command "${cmd}"`);
  }
}

function fetchedPostManifest(
  args: CmdArgs | undefined,
  allocateFileId: FileIdAllocator,
): MockDownloadManifest {
  const username = String(args?.username);
  const category = args?.category as "posts" | "reels";
  const scope = args?.scope as "shown" | "selected";
  const posts = args?.posts as Post[];
  const outputs = posts.flatMap((post, postIndex) => {
    const stem = safeBasenameSegment(post.code, `post-${postIndex + 1}`);
    return post.resources.map((resource, ordinal) =>
      mockOutput(allocateFileId, stem, resource.kind, ordinal),
    );
  });
  return {
    label: `@${username} ${category} · ${scope} · ${posts.length}`,
    dir: `/mock/instagram-archive/${username}/${category}`,
    requestedItems: posts.length,
    outputs,
  };
}

function directManifest(
  args: CmdArgs | undefined,
  allocateFileId: FileIdAllocator,
): MockDownloadManifest {
  const label = safeBasenameSegment(args?.label, "instagram");
  const subfolder = safeBasenameSegment(args?.subfolder, "media");
  const items = args?.items as DirectItem[];
  const outputs = items.map((item, outputIndex) => {
    const fixtureKind = subfolder.toLowerCase() === "stories" ? MOCK_STORY_KINDS.get(item.pk) : undefined;
    const kind = fixtureKind ?? (/\.(?:mp4|mov|webm)(?:[?#]|$)/i.test(item.url) ? "video" : "photo");
    return mockOutput(
      allocateFileId,
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

function standalonePostManifest(
  args: CmdArgs | undefined,
  allocateFileId: FileIdAllocator,
): MockDownloadManifest {
  const code = safeBasenameSegment(args?.code, "post");
  return {
    label: `Post ${code}`,
    dir: "/mock/instagram-archive/posts",
    requestedItems: 1,
    outputs: [mockOutput(allocateFileId, code, "photo", 0)],
  };
}

function profileManifest(
  args: CmdArgs | undefined,
  allocateFileId: FileIdAllocator,
): MockDownloadManifest {
  const username = safeBasenameSegment(args?.username, "instagram");
  const opts = (args?.opts ?? {}) as Partial<ProfileOptions>;
  const kinds: Array<{ suffix: string; kind: "photo" | "video" }> = [];
  if (opts.posts) kinds.push({ suffix: "post", kind: "photo" });
  if (opts.reels) kinds.push({ suffix: "reel", kind: "video" });
  if (opts.stories) kinds.push({ suffix: "story", kind: "photo" });
  if (opts.highlights) kinds.push({ suffix: "highlight", kind: "video" });
  if (opts.avatar) kinds.push({ suffix: "avatar", kind: "photo" });
  return {
    label: `@${username} archive`,
    dir: `/mock/instagram-archive/${username}`,
    outputs: kinds.map(({ suffix, kind }) =>
      mockOutput(allocateFileId, `${username}_${suffix}`, kind, 0),
    ),
  };
}

function downloadManifest(
  cmd: string,
  args: CmdArgs | undefined,
  allocateFileId: FileIdAllocator,
): MockDownloadManifest {
  switch (cmd) {
    case "enqueue_fetched_post_download":
      return fetchedPostManifest(args, allocateFileId);
    case "download_direct":
      return directManifest(args, allocateFileId);
    case "download_post":
      return standalonePostManifest(args, allocateFileId);
    case "enqueue_profile_download":
      return profileManifest(args, allocateFileId);
    default:
      throw new Error(`mock download: unhandled command "${cmd}"`);
  }
}

function reply(
  cmd: string,
  args?: CmdArgs,
  registeredMedia?: ReadonlyMap<number, string>,
): unknown {
  switch (cmd) {
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
      return Number.isInteger(args?.fileId) && registeredMedia?.has(Number(args?.fileId)) === true;
    case "open_library_file":
    case "reveal_library_file":
      if (
        !Number.isInteger(args?.fileId) ||
        registeredMedia?.has(Number(args?.fileId)) !== true
      ) {
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
          following_count: 234,
          is_private: false,
          is_verified: true,
          avatar_url: AVATAR,
        },
        recent_posts: Array.from({ length: 12 }, (_, i) => {
          const index = pageStart + i;
          const thumb = profilePreview(index);
          const isVideo = index % 3 === 0;
          return {
            pk: String(MOCK_PROFILE_PK_START + index),
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
    case "fetch_profile_summary":
      return {
        pk: "25025320",
        username: String(args?.username ?? "instagram"),
        full_name: "Instagram",
        media_count: 7421,
        follower_count: 713_000_000,
        following_count: 234,
        is_private: false,
        is_verified: true,
        avatar_url: AVATAR,
      };
    case "fetch_relationships": {
      const kind = args?.kind === "following" ? "following" : "followers";
      const pageStart = args?.maxId ? 12 : 0;
      return {
        users: mockRelationshipUsers(kind).slice(pageStart, pageStart + 12),
        next_cursor: args?.maxId ? null : `${kind}-cursor`,
      };
    }
    case "search_relationships": {
      const kind = args?.kind === "following" ? "following" : "followers";
      const query = String(args?.query ?? "").trim().toLowerCase();
      return mockRelationshipUsers(kind).filter((user) =>
        `${user.username} ${user.full_name ?? ""}`.toLowerCase().includes(query),
      );
    }
    case "fetch_reels": {
      const pageStart = args?.endCursor ? 11 : 0;
      return {
        posts: Array.from({ length: 11 }, (_, i) => {
          const index = pageStart + i;
          const thumbnail = reelPreview(index);
          return {
            pk: String(MOCK_REEL_PK_START + index),
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
      return MOCK_STORIES.map((story) => ({ ...story }));
    case "validate_token":
      return reply("__balance", undefined, registeredMedia);
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

export function uninstallTauriMock(): void {
  const w = window as unknown as MockWindow;
  const dispose = w[MOCK_DISPOSER];
  if (typeof dispose === "function") dispose();
  delete w[MOCK_DISPOSER];
  delete w[MOCK_LIBRARY_MEDIA_URL_RESOLVER];
  delete w[MOCK_REMOTE_MEDIA_URL_RESOLVER];
  delete w.__TAURI_INTERNALS__;
  delete w.__TAURI_EVENT_PLUGIN_INTERNALS__;
}

export function installTauriMock(): void {
  uninstallTauriMock();
  const w = window as unknown as MockWindow;
  const callbacks = new Map<number, (data: unknown) => void>();
  const listeners = new Map<string, number[]>();
  const activeJobs = new Map<
    string,
    { label: string; timers: Array<ReturnType<typeof setTimeout>> }
  >();
  let nextCallbackId = 1;
  let nextJobNumber = 1;
  let nextFileId = 10_101;
  let disposed = false;
  const registeredMedia = new Map<number, string>();
  const config: ConfigState = {
    has_token: true,
    token_hint: "***9f3a",
    has_proxy: false,
    proxy_hint: null,
    dest_dir: "~/Downloads/insta-dl",
    sidecar: true,
  };

  function configState(): ConfigState {
    return { ...config };
  }

  function setMockProxy(proxyUrl: unknown): ConfigState {
    if (proxyUrl === null || (typeof proxyUrl === "string" && !proxyUrl.trim())) {
      config.has_proxy = false;
      config.proxy_hint = null;
      return configState();
    }
    if (typeof proxyUrl !== "string") throw new Error(PROXY_VALIDATION_ERROR);
    const trimmedProxy = proxyUrl.trim();
    let parsed: URL;
    try {
      parsed = new URL(trimmedProxy);
    } catch {
      throw new Error(PROXY_VALIDATION_ERROR);
    }
    const scheme = parsed.protocol.slice(0, -1).toLowerCase();
    const hasQueryOrFragmentMarker = trimmedProxy.includes("?") || trimmedProxy.includes("#");
    if (
      !["http", "https", "socks5", "socks5h"].includes(scheme) ||
      !parsed.hostname ||
      (parsed.pathname !== "/" && parsed.pathname !== "") ||
      hasQueryOrFragmentMarker ||
      parsed.search ||
      parsed.hash ||
      parsed.port === "0" ||
      ((scheme === "socks5" || scheme === "socks5h") && !parsed.port)
    ) {
      throw new Error(PROXY_VALIDATION_ERROR);
    }
    const userInfo = parsed.username || parsed.password ? "***@" : "";
    config.has_proxy = true;
    config.proxy_hint = `${scheme}://${userInfo}${parsed.host}/`;
    return configState();
  }

  function saveMockSettings(args?: CmdArgs): ConfigState {
    if (typeof args?.destDir === "string") config.dest_dir = args.destDir;
    if (typeof args?.sidecar === "boolean") config.sidecar = args.sidecar;
    return configState();
  }

  for (const card of mockLibraryCards()) {
    for (const file of mockFiles(card)) {
      if (file.exists_on_disk && (file.kind === "photo" || file.kind === "video")) {
        registeredMedia.set(file.id, mockMediaFixture(file.kind));
      }
    }
  }

  function allocateFileId(kind: DownloadMediaKind): number {
    const fileId = nextFileId++;
    registeredMedia.set(fileId, mockMediaFixture(kind));
    return fileId;
  }

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
    const validatedArgs = validateDownloadArgs(cmd, args);
    const jobNumber = nextJobNumber++;
    const jobId = `mock-job-${jobNumber}`;
    const manifest = downloadManifest(cmd, validatedArgs, allocateFileId);
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

  const tauriInternals = {
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
        if (cmd === "config_state") return Promise.resolve(configState());
        if (cmd === "save_settings") return Promise.resolve(saveMockSettings(args));
        if (cmd === "set_proxy") return Promise.resolve(setMockProxy(args?.proxyUrl));
        const response = Promise.resolve(reply(cmd, args, registeredMedia));
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
  const eventPluginInternals = {
    unregisterListener: (event: string, eventId: number) => {
      unregisterCallback(eventId);
      listeners.set(
        event,
        (listeners.get(event) ?? []).filter((id) => id !== eventId),
      );
    },
  };
  w.__TAURI_INTERNALS__ = tauriInternals;
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = eventPluginInternals;
  const mediaUrlResolver = (fileId: number) => registeredMedia.get(fileId);
  w[MOCK_LIBRARY_MEDIA_URL_RESOLVER] = mediaUrlResolver;
  const remoteMediaUrlResolver = (url: string) =>
    MOCK_REMOTE_MEDIA_FIXTURES.has(url) ? url : undefined;
  w[MOCK_REMOTE_MEDIA_URL_RESOLVER] = remoteMediaUrlResolver;

  const dispose = () => {
    if (disposed) return;
    disposed = true;
    for (const active of activeJobs.values()) {
      for (const timer of active.timers) clearTimeout(timer);
    }
    activeJobs.clear();
    registeredMedia.clear();
    callbacks.clear();
    listeners.clear();
    if (w.__TAURI_INTERNALS__ === tauriInternals) delete w.__TAURI_INTERNALS__;
    if (w.__TAURI_EVENT_PLUGIN_INTERNALS__ === eventPluginInternals) {
      delete w.__TAURI_EVENT_PLUGIN_INTERNALS__;
    }
    if (w[MOCK_LIBRARY_MEDIA_URL_RESOLVER] === mediaUrlResolver) {
      delete w[MOCK_LIBRARY_MEDIA_URL_RESOLVER];
    }
    if (w[MOCK_REMOTE_MEDIA_URL_RESOLVER] === remoteMediaUrlResolver) {
      delete w[MOCK_REMOTE_MEDIA_URL_RESOLVER];
    }
    if (w[MOCK_DISPOSER] === dispose) delete w[MOCK_DISPOSER];
  };
  w[MOCK_DISPOSER] = dispose;
}

export function isMockMode(): boolean {
  return new URLSearchParams(window.location.search).has("mock");
}

export function isDemoProfile(): boolean {
  return new URLSearchParams(window.location.search).get("demo") === "profile";
}
