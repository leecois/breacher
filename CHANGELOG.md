# Changelog

Notable changes for people who use Breacher. The format follows
[Keep a Changelog](https://keepachangelog.com), and versions follow
[Semantic Versioning](https://semver.org).

## Unreleased

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
