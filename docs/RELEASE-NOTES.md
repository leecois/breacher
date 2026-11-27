## New in 0.2.5

- **Stays in the tray.** Closing the window no longer ends Breacher: it stays in the tray (the
  icons by the clock, possibly behind the arrow), so the hotkey keeps working while you play. Click
  the icon to bring the window back; right-click it and choose **Quit Breacher** to end it. Untick
  "Stay in the tray when closed" to have the close button quit as before.
- **Versions on show.** The bottom of the window names the Breacher that is running and, after a
  capture of the game, the version of the game it looked at. Quote both when you report a problem.

## New in 0.2.4

- **The route leaves when the breach does.** After a hotkey capture Breacher keeps an eye on the
  game window: the route stays over the game for as long as the breach screen is there, and goes
  about two seconds after it closes or as soon as the game is minimised. It used to stay for 45
  seconds whatever happened.
- On Windows 10 the route still stays for 45 seconds: looking at the game again would flash a
  yellow capture border round it every time.

## New in 0.2.3

- **Updates.** An installed copy looks for a newer version when it starts and installs it when you
  say so, after checking that it carries the project's signature. "Look for updates at start" turns
  the check off.
- **Runs without installing.** `Breacher_…_x64-portable.exe` starts from wherever you put it. It
  tells you about a newer version and opens the download page, since it has no installation to
  replace.

## Fixed in 0.2.2

Fixes for the first reports from a gaming PC:

- **"The hotkey could not be registered (Hotkey already registered)".** Closing the window did not
  end Breacher: it kept running unseen and kept the hotkey, so the next start could not have it.
  Closing the window now ends the program, and starting it twice brings the running window forward.
- **The hotkey can be changed.** The window shows the hotkey in use, and **Change** sets another.
  If another program already holds the hotkey, Breacher takes the next free combination
  (Ctrl Alt B, then Alt Shift B) and says which one.
- **A rejected capture shows what was captured**, where it used to show a broken picture. It also
  says when the game window was not found or is minimised.
- **Back to start** returns from a result or a rejected frame to the start screen.
- The "On the screen" view now works after a capture.

This is an early version. The desktop app has had little testing on Windows so far, so expect rough
edges and please report what you see.

## Run without installing

Download `Breacher_…_x64-portable.exe`, put it anywhere and double-click it. Nothing is installed;
deleting the file removes the program. Settings and the log are still kept in
`%APPDATA%\dev.breacher.mk2`.

It needs the WebView2 runtime, which Windows 11 and an up-to-date Windows 10 already have. If the
window does not open, use the installer instead: it fetches the runtime when it is missing.

## Install

1. Download `Breacher_…_x64-setup.exe` and run it.
2. If Breacher is running, in the tray or (before 0.2.2) unseen, the installer says so: choose
   **OK** to let it close the running copy.

## First start

Neither file is code-signed, so Windows SmartScreen warns that the publisher is unknown: choose
**More info**, then **Run anyway**. Set the game to **Windowed Borderless** (the overlay cannot draw
over exclusive fullscreen).

## Use

- Open a breach in the game and, **before the first pick**, press **Ctrl Shift B** (or the hotkey
  the window shows). The route appears in the Breacher window and, if "Draw the route over the game"
  is on, over the game until the breach screen closes.
- **Capture game** in the window does the same without the hotkey. It needs the game visible on a
  screen, so it suits a second monitor; a minimised game cannot be captured.
- No luck with capture? Two fallbacks need none at all:
  - **Watch a screenshot folder**: point it at the folder where the game, its launcher or Windows
    saves screenshots; each new screenshot is read as it appears.
  - Drop or paste (Ctrl V) a screenshot into the window.
- The "On the screen" tab shows the frame that was captured with the route drawn on it. If that
  picture is right but the overlay over the game is off, that is a placement bug worth reporting.

## What to check, and what to send back

These have not been confirmed against the running game yet:

- Does the hotkey capture the game window? (A yellow capture border may flash on Windows 10.) If the
  screen is not read, the window now shows the frame it captured: is it the game?
- Is the matrix read correctly? Compare the "Plan" tab with the game.
- Does the overlay line up with the cells?
- Does following the route upload what it says?

If something is wrong, the two most useful things are:

- a screenshot of the breach screen (before the first pick), and
- the log file `%APPDATA%\dev.breacher.mk2\breacher.log`.

## Also in this release

`breach-cli.exe` is the command-line tool:

```
breach-cli.exe read screenshot.png
breach-cli.exe rule-probe screenshot.png
```

`rule-probe` prints a route whose result shows how the game treats a wrong pick in the middle of a
sequence, which is not yet confirmed (see `docs/RULE-CHECK.md`). Until it is, Breacher only suggests
routes that work under every candidate rule.

## Known limits

- Tested on 11 captures from 2020–21 and on composed screens; not yet on the current game patch.
- HDR is handled by tolerance to over-bright frames, tested in simulation only.
- Exclusive fullscreen is not supported by the overlay; the Breacher window still shows the route.
