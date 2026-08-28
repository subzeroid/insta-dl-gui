# Changelog

All notable changes to insta-dl-gui are documented in this file.

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
