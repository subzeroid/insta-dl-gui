# Usage

## Download a single post or reel

Paste any of these into the input and press **Fetch**:

```
https://www.instagram.com/p/DXZlTiKEpxw/
https://www.instagram.com/reel/DXZlTiKEpxw/
```

All media of the post (including carousels) downloads immediately. Live progress stays visible in the footer; click it to open **Queue** and cancel a job.

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
| **Max** | cap on posts or reels considered (empty = all) | — |

3. Press **Download**. Progress appears in the global footer; click it to open **Queue** for every active and completed job.

For a private profile, only **Avatar** is available. Posts, reels, stories and highlights remain hidden because the public API cannot access them.

## Explore and choose what to download

The app opens on **Explore** after setup. Type at least two characters of a username, pick an autocomplete result with the mouse or arrow keys and **Enter**, then switch between **Posts**, **Reels** and **Stories**. The selected profile, active tab, loaded pages and selections stay available when you visit another app section and return during the same app session.

Current Stories load automatically after a public profile opens and cost 2 HikerAPI requests. A Stories failure does not block Posts or Reels; open **Stories** and use **Retry stories** when needed.

Each tab has the same **Download** control:

| Action | What it downloads | API and Queue behavior |
|---|---|---|
| **All** | The complete Posts or Reels archive, including pages not shown | Fetches through HikerAPI and may use additional requests |
| **All** on Stories | A refreshed set of all currently active Stories | Uses additional HikerAPI requests |
| **Shown N** | The exact items currently loaded in that tab | Fetches no additional pages and creates one Queue job |
| **Selected N** | Only the cards whose checkboxes are selected | Fetches no additional pages and creates one Queue job |

Use **Load more** on Posts or Reels before **Shown** if you want more pages in that exact snapshot. Shown and Selected accept at most 500 items. At 501 or more, the action is disabled and nothing is silently truncated or split; use **All** for the complete archive instead.

Click a card to preview it, or use its checkbox to add it to **Selected**. After Queue accepts a Selected snapshot, only the submitted selections are cleared; a failed enqueue keeps them selected, and a card reselected while the request is pending stays selected. Press **Escape** to close autocomplete or an open preview.

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

## Browse the local media Library

The **Library** is a local index of media in your download folder. It does not upload archive contents or spend HikerAPI requests.

### First scan

1. Open **Library**. The current download folder is registered automatically.
2. Press **Scan library** to import media already on disk.
3. Keep the page open to watch progress, or cancel the scan and start it again later.

Downloads completed by the app are cataloged automatically. The initial scan is still needed for files that existed before the Library was enabled, including archives shared with `insta-dl` CLI.

### Search and filters

- Search matches owner username, Instagram shortcode and caption text.
- Media-kind filters cover posts, reels, stories and avatars.
- Availability separates files still on disk from entries marked **Missing**.
- The captured-date range uses local calendar dates.
- Sort by **Publication date** or **Import date**.

Photo and video previews load directly from the local archive. On macOS, allow the single folder-access request when Library first loads previews. If access is denied, previews stay as placeholders until you enable **insta-dl-gui** under **System Settings → Privacy & Security → Files and Folders** and press **Retry previews**. See [troubleshooting](troubleshooting.md#macos-keeps-asking-for-access-to-downloads-in-library).

Open an item to inspect its metadata and files. **Open file** launches an available file with the system default app; **Show in folder** reveals it in the system file manager. These actions are disabled for missing files.

### Rescans and missing files

A successful rescan updates entries found at their existing archive paths. Catalog entries not seen during that completed scan are retained and marked **Missing**, not deleted. If a file reappears at the same path, another completed rescan marks it available again. A cancelled or failed scan does not complete the missing-file pass.

Library scans are read-only for the archive. They never move, rename, edit or delete downloaded media or JSON sidecars. Only the rebuildable catalog database is updated.

The database is `insta-dl-gui/catalog.sqlite3` inside your platform app-data directory:

| OS | Catalog directory |
|---|---|
| macOS | `~/Library/Application Support/insta-dl-gui/` |
| Windows | `%APPDATA%\insta-dl-gui\` |
| Linux | `$XDG_DATA_HOME/insta-dl-gui/` or `~/.local/share/insta-dl-gui/` |

Changing the download folder in **Settings** registers it as another Library root; existing roots and their history remain in the catalog. If registration fails after the setting is saved, Settings shows a warning with a link back to Library so you can fix the folder and scan again.

## Incremental archives

Re-running the same profile skips everything already on disk — file stems are compared before each download. Only new content costs API requests. Use this to keep an archive in sync: run daily, pay only for new posts.

## Queue

The footer keeps download activity visible from Download, Explore, Library and Settings. It shows the active job count, current file and downloaded bytes; click anywhere on it to open **Queue**.

The **Queue** screen lists all jobs with per-file progress and byte counters. Failed jobs show the reason (private profile, quota exhausted, deleted post). Cancelled and finished jobs can be cleared.

If a batch saves some files before a later item fails, it finishes with the exact number saved. Re-run the same target after fixing the cause; existing files are skipped.
