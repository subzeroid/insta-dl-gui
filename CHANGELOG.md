# Changelog

All notable changes to insta-dl-gui are documented in this file.

## 0.4.0 - 2026-08-30

### Added

- Explore is now the first screen and keeps the current profile, tab, loaded pages, Stories, and selections while navigating through the app.
- Posts, Reels, and Stories can be selected individually and downloaded with one consistent **All / Shown / Selected** control.
- Stories load automatically alongside the profile without blocking Posts or Reels.

### Changed

- **Shown** and **Selected** download the exact media currently in Explore, including carousel resources, while **All** remains the complete archive action.
- Exact snapshots are limited to 500 items with a clear fallback to **All**; accepted selections clear safely while failed or changed selections remain selected.
- Explore downloads now share Queue and footer reservations, cancellation, byte progress, and terminal results across navigation and remounts.
- Release smoke verification retries transient CDN delivery failures.

### Fixed

- Exact batch downloads now validate identifiers, media URLs, duplicate payloads, and resource limits before writing files.
- Partial failures, retries, recovered files, metadata sidecars, catalog entries, and cancellation keep accurate progress and concrete results across a batch.
- Stories reuse the loaded profile ID, survive navigation while pending, ignore stale responses, and expose a recoverable retry state.
- The browser-based Explore demo now supports scoped fetched-media downloads.

## 0.3.2 - 2026-08-29

### Added

- Explore now loads Reels from HikerAPI's dedicated clips endpoint, one cursor-paged API response at a time.
- Reels can be extended explicitly with **Load more** and downloaded with a count that matches the unique items shown.

### Changed

- Explore keeps the selected profile, tab, and loaded pages while navigating between app sections.

### Fixed

- Fixed Reels payloads with an empty `resources` array so their top-level video previews and downloads remain available.
- Fixed duplicate Reels and cyclic cursors from consuming the visible download limit or causing extra archive requests.
- Fixed the Stories download action appearing on an empty Reels tab.

## 0.3.1 - 2026-08-29

### Fixed

- Fixed repeated macOS Downloads-folder permission prompts in Library by requesting preview access once per configured folder per app session.
- Library now keeps safe placeholders after access is denied and offers a single explicit **Retry previews** action.
- macOS application bundles are now fully ad-hoc signed and signature-verified in CI so privacy permissions attach to a complete app identity within each release.

## 0.3.0 - 2026-08-28

### Added

- Added a global download activity footer with live job, file, and byte progress that opens Queue from every main screen.
- Added safe HikerAPI token replacement in Settings with validation before the active token changes.

### Fixed

- Fixed local Library preview URLs so downloaded photos and videos load through the protected media protocol.
- Fixed video stories being rendered as broken images instead of video previews.

### Changed

- Download and Explore now use the global footer for active progress; Queue remains the detailed job and cancellation screen.

## 0.2.0 - 2026-08-24

### Added

- Added a local, searchable Library for downloaded posts, reels, stories, highlights, and profile media.
- Added cancellable archive scans with progress reporting, stale-root handling, and first-scan guidance.
- Added safe media previews and operating-system actions for opening files and revealing their folders.

### Changed

- Downloads are now indexed in the local catalog as they complete.
- Settings saves are serialized and preserve focus while folder and sidecar options are updated.
- Navigation and Library layouts now adapt to narrow windows.
- Updated the README and usage guide for the Library workflow.
