# Changelog

All notable changes to insta-dl-gui are documented in this file.

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
