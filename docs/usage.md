# Usage

## Download a single post or reel

Paste any of these into the input and press **Fetch**:

```
https://www.instagram.com/p/DXZlTiKEpxw/
https://www.instagram.com/reel/DXZlTiKEpxw/
```

All media of the post (including carousels) downloads immediately with live progress. Cancel anytime from the job card.

## Archive a whole profile

1. Paste `@username` (or a profile URL) and press **Fetch** — you get a preview card: avatar, name, post/follower counts.
2. Tick what you want:

| Option | What downloads | Lands in |
|---|---|---|
| **Posts** | full feed: photos, albums, videos | `<dest>/<username>/posts/` |
| **Reels** | only video posts | `<dest>/<username>/posts/` |
| **Stories** | active stories (24h) | `<dest>/<username>/stories/` |
| **Highlights** | every highlight reel | `<dest>/<username>/highlights/<id>_<title>/` |
| **Avatar** | HD profile picture | `<dest>/<username>/avatar_<pk>.<ext>` |
| **Max** | cap on posts considered (empty = all) | — |

3. Press **Download**. Progress appears live; switch to the **Queue** tab for all jobs.

## Where files end up

Default destination is `~/Downloads/insta-dl` (change it in **Settings**). Layout matches [insta-dl CLI](https://subzeroid.github.io/insta-dl/) exactly, so both tools can maintain the same archive:

```
~/Downloads/insta-dl/<username>/
    2026-04-21_16-04-15_DXZlTiKEpxw.mp4      # feed post, mtime = taken_at
    2026-04-21_16-04-15_DXZlTiKEpxw.json     # metadata sidecar
    avatar_25025320.jpg
    stories/
        2026-04-21_18-30-00_178290.jpg
    highlights/
        17991_Travel/
            2025-10-12_19-20-30_4011.jpg
```

Filenames start with the original post timestamp, so gallery apps sort chronologically. The JSON sidecar carries caption, like/comment counts and owner info — toggle it off in **Settings** if you only want media.

## Incremental archives

Re-running the same profile skips everything already on disk — file stems are compared before each download. Only new content costs API requests. Use this to keep an archive in sync: run daily, pay only for new posts.

## Queue

The **Queue** tab lists all jobs with per-file progress and byte counters. Failed jobs show the reason (private profile, quota exhausted, deleted post). Cancelled and finished jobs can be cleared.
