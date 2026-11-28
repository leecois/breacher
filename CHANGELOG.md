# Changelog

Notable changes for people who use Breacher. The format follows
[Keep a Changelog](https://keepachangelog.com), and versions follow
[Semantic Versioning](https://semver.org).

## Unreleased

## 0.4.0 - 2026-11-28

### Added

- **Read each breach as it opens**: the desktop program can watch the game and draw the route
  without the hotkey. Off by default; offered where watching the game goes unseen.
- The wrong-pick rule is learnt from play. After a breach that was followed, the buffer and the
  installed daemons are read from the game and every rule that predicts another result is dropped.
  This also answers a rule check, in the desktop program and in the browser.
- Vietnamese, chosen from the list at the bottom of the window or taken from the browser.
- The reader knows the arrangement of the panels the game has used since its matrix grows from
  the left, and no longer reports those screens as unusual.

### Fixed

- The route over the game could end up under the game after switching windows, and showed only
  while Breacher's own window was in front. It is now put back on top at every capture and every
  look.
- Changing a choice in the window no longer draws a route over the game after that route was
  taken down.

## 0.3.0 - 2026-11-28

### Added

- Breacher runs in a browser at <https://leecois.github.io/breacher/>. Shown the game by screen
  sharing, the page reads each breach as it opens, counts off the picks and plans again after a
  pick off the route. Where the browser has one, a floating window keeps the route above the game.
- **Capture** in the desktop window chooses what the hotkey captures: the game found by its name, a
  window from the list of those that are open, or a whole display.

### Changed

- The follower that keeps a route in step with the picks is part of the core, so the desktop
  program and the page run the same code.

## 0.2.9 - 2026-11-28

### Changed

- Picks are looked for every 0.4 seconds, where it was 0.7, so the route keeps up with fast play.

### Fixed

- The route leaves the moment the game has settled every daemon. It could stay up to five seconds
  over the screen that follows a breach, which was sometimes taken for the breach itself.

## 0.2.8 - 2026-11-28

### Added

- Breacher follows your picks while a route is drawn: played steps fade, the next one stands out,
  and after a pick off the route the rest is planned again from where the breach stands.

### Changed

- A saved report holds every look at the last breach that was followed, up to the latest 24 and
  including how it ended, where it used to hold one look every two seconds. The log notes what
  each look saw.

## 0.2.7 - 2026-11-28

### Added

- **Check the rule with a breach.** Breacher draws a route chosen to tell the wrong-pick rules
  apart; you play it and say what uploaded. One or two breaches settle the rule, and routes are
  better from then on.
- A saved report also holds up to eight frames of the game window from the last breach that was
  followed.

### Changed

- Once one wrong-pick rule has been ruled out, routes only have to work under the two that remain.

## 0.2.6 - 2026-11-28

### Added

- When a capture cannot be read, a line over the game says why for a few seconds.
- **Save a report** writes the last frame, its reading, the log and the versions into a folder
  under your documents.
- "Start when you sign in" starts Breacher in the tray every time you log on.

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
