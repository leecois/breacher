## New in 0.2.2

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

This is a pre-release. The desktop app has had little testing on Windows so far, so expect rough
edges and please report what you see.

## Install

1. Download `Breacher_…_x64-setup.exe` and run it. Windows SmartScreen will warn that the publisher
   is unknown, because the installer is not code-signed: choose **More info**, then **Run anyway**.
2. If an earlier version is installed, the installer may say that Breacher is still running, even
   with no window open. That is the fault fixed in 0.2.2: choose **OK** to let it close the old copy.
3. Start Breacher. Set the game to **Windowed Borderless** (the overlay cannot draw over exclusive
   fullscreen).

## Use

- Open a breach in the game and, **before the first pick**, press **Ctrl Shift B** (or the hotkey
  the window shows). The route appears in the Breacher window and, if "Draw the route over the game"
  is on, over the game for 45 seconds.
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
