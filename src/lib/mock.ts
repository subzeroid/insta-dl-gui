/**
 * Dev-only Tauri IPC mock so the UI can run in a plain browser
 * (`?mock=1`) for screenshots and UI work without the Rust backend.
 */

type CmdArgs = Record<string, unknown>;

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
    case "resolve_input": {
      const input = String(args?.input ?? "");
      if (/instagram\.com\/(p|reel|reels|tv)\//.test(input)) {
        return { kind: "post", code: "DXZlTiKEpxw" };
      }
      const username = input.replace(/^@/, "").trim() || "instagram";
      return { kind: "profile", username };
    }
    case "fetch_profile":
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
          const hue = Math.round((i * 137) % 360);
          const thumb =
            "data:image/svg+xml," +
            encodeURIComponent(
              `<svg xmlns='http://www.w3.org/2000/svg' width='400' height='400'>
                 <defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'>
                   <stop offset='0' stop-color='hsl(${hue},60%,30%)'/>
                   <stop offset='1' stop-color='hsl(${(hue + 60) % 360},60%,18%)'/>
                 </linearGradient></defs>
                 <rect width='400' height='400' fill='url(#g)'/>
                 <circle cx='${80 + ((i * 97) % 240)}' cy='${100 + ((i * 61) % 200)}' r='56' fill='#ffffff' opacity='.14'/>
               </svg>`,
            );
          const isVideo = i % 3 === 0;
          return {
            pk: `p${i}`,
            code: `DEMO${i}`,
            caption: `Demo post #${i} — golden hour somewhere far away`,
            like_count: 1200 * (12 - i),
            comment_count: 40 + i,
            taken_at: 1776000000 + i * 86400,
            owner_username: "natgeo",
            thumbnail_url: thumb,
            resources: [
              { url: "", kind: isVideo ? ("video" as const) : ("photo" as const) },
            ],
          };
        }),
        end_cursor: "cursor",
      };
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
        { pk: "s1", taken_at: 1776787455, kind: "photo", media_url: "", thumb_url: "" },
        { pk: "s2", taken_at: 1776787500, kind: "video", media_url: "", thumb_url: "" },
        { pk: "s3", taken_at: 1776787600, kind: "photo", media_url: "", thumb_url: "" },
      ];
    case "download_direct":
    case "download_post":
      return "mock-job-id";
    case "enqueue_profile_download":
      return "mock-job-id";
    case "cancel_job":
      return true;
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
    default:
      throw new Error(`mock ipc: unhandled command "${cmd}"`);
  }
}

export function installTauriMock(): void {
  const w = window as unknown as Record<string, unknown>;
  w.__TAURI_INTERNALS__ = {
    invoke: (cmd: string, args?: CmdArgs) => {
      try {
        return Promise.resolve(reply(cmd, args));
      } catch (e) {
        return Promise.reject(e);
      }
    },
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    plugins: {},
  };
}

export function isMockMode(): boolean {
  return new URLSearchParams(window.location.search).has("mock");
}

export function isDemoProfile(): boolean {
  return new URLSearchParams(window.location.search).get("demo") === "profile";
}
