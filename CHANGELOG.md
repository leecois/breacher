# Changelog

Notable changes for people who use Breacher. The format follows
[Keep a Changelog](https://keepachangelog.com), and versions follow
[Semantic Versioning](https://semver.org).

## Unreleased

## 0.2.5 - 2026-11-27

### Added

- The bottom of the window shows the version of Breacher and, after a capture of the game, the
  version of the game.
- A tray icon. Closing the window keeps Breacher running there, with the hotkey still working;
  a click brings the window back and the menu quits. "Stay in the tray when closed" turns it off.

## 0.2.4 - 2026-11-27

### Changed

- The route drawn over the game now stays until the breach screen closes, where it used to vanish
  after 45 seconds, and it is taken down as soon as the breach is over or the game is minimised.
  On Windows 10 it still stays for 45 seconds, because looking at the game again would flash a
  capture border round it.

## 0.2.3 - 2026-11-27

### Added

- An installed copy looks for a newer release when it starts and installs it on request, after
  checking its signature. "Look for updates at start" turns the check off.
- A portable build for Windows that runs without installing, next to the installer. It reports a
  newer release and opens the download page.

## 0.2.2 - 2026-11-27

### Added

- The capture hotkey can be changed in the window. When another program already holds it, Breacher
  takes the next free combination and says which one.
- **Back to start** returns from a result or a rejected frame to the start screen.

### Fixed

- Closing the window now ends Breacher. It used to keep running unseen and hold the hotkey, so the
  next start reported the hotkey as already registered.
- Starting Breacher a second time brings the running window forward instead of starting a copy.
- A rejected capture shows the frame that was captured, where it used to show a broken picture, and
  says when the game window was not found or is minimised.
- The "On the screen" view is available after a capture in the desktop app.
- The header no longer overflows a narrow window.

## 0.2.1 - 2026-11-27

### Added

- The project is licensed under MIT or Apache-2.0, at your option.

### Changed

- The reader and the page were reorganised internally. Reading, solving and capture behave as in
  0.2.0.

## 0.2.0 - 2026-11-27

### Changed

- The interface has been redrawn: black glass, signal yellow, cyan for controls, panels with two
  corners cut. The route stays magenta so it stands apart from the game's own colours in the overlay.
- The pick order is shown as the buffer, one slot per position.
- The plan settings share one panel.
- New application icon.

## 0.1.0 - 2026-11-27

First pre-release.

### Added

- Screen reader for the Breach Protocol screen, which refuses a frame it cannot read with confidence.
- Exact solver with routes that hold under every candidate wrong-pick rule.
- Desktop app for Windows: hotkey capture, route overlay over the game, watched screenshot folder.
- The `breach` command-line tool: `read` and `rule-probe`.
