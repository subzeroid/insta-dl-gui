# Getting a HikerAPI token

insta-dl-gui talks to Instagram exclusively through [HikerAPI](https://hikerapi.com/p/uk064a1b) — a paid managed API. Your Instagram account is never involved: no login, no session, nothing to ban.

1. Sign up at [hikerapi.com](https://hikerapi.com/p/uk064a1b) — **the first 100 requests are free**, no credit card required.
2. Open your [HikerAPI dashboard](https://hikerapi.com/p/uk064a1b) and copy your token.
3. In insta-dl-gui, paste the token into the first-launch screen and press **Connect**.

The token is stored locally in the app config (`0600` permissions on macOS/Linux) and never leaves your machine except in requests to `api.hikerapi.com`.

## How much does a download cost?

One request ≈ one API call. Typical costs:

| Action | Requests |
|---|---|
| Fetch a single post/reel | 1 |
| Profile preview (what you see before downloading) | 2 |
| Feed pagination | 1 per page (~12–18 posts each) |
| Stories | 2 |
| Highlights listing | 2 |
| Each highlight reel's items | 1 per reel |

The remaining balance is always visible in the app header; click it to refresh. If quota runs out before any file is saved, the job fails with a link to top up. If some files were already saved, the job finishes with that exact count; top up and re-run to fetch the rest. Media CDN downloads themselves are free and unlimited.

!!! tip
    Use the **Max** field to cap how many posts a profile job considers — handy for testing a big profile before committing quota to the whole archive.

## Rotating or removing a token

Re-run the flow above with a new token — it replaces the old one. To wipe all app data, delete the config directory:

- macOS/Linux: `~/.config/insta-dl-gui/`
- Windows: `%APPDATA%\insta-dl-gui\`
