## New in 0.2.0

The interface has been redrawn to sit beside the game without imitating it: black glass, signal
yellow, cyan for anything you can touch, panels with two corners cut, Rajdhani and Chakra Petch
type. The route stays magenta so it cannot be confused with the game's yellow-green and cyan
when it is drawn over the game. The pick order is now shown as the buffer, one slot per position.

Nothing in the reader, the solver or the capture code changed in this version.

This is a pre-release. The desktop app has had little testing on Windows so far, so expect rough
edges and please report what you see.

## Install

1. Download `Breacher_…_x64-setup.exe` and run it. Windows SmartScreen will warn that the publisher
   is unknown, because the installer is not code-signed: choose **More info**, then **Run anyway**.
2. Start Breacher. Set the game to **Windowed Borderless** (the overlay cannot draw over exclusive
   fullscreen).

## Use

- Open a breach in the game and, **before the first pick**, press **Ctrl Shift B**. The route appears
  in the Breacher window and, if "Draw the route over the game" is on, over the game for 45 seconds.
- No luck with the hotkey? Two fallbacks need no capture at all:
  - **Watch a screenshot folder**: point it at the folder where the game, its launcher or Windows
    saves screenshots; each new screenshot is read as it appears.
  - Drop or paste (Ctrl V) a screenshot into the window.
- The "On the screen" tab shows the frame that was captured with the route drawn on it. If that
  picture is right but the overlay over the game is off, that is a placement bug worth reporting.

## What to check, and what to send back

These have not been confirmed against the running game yet:

- Does Ctrl Shift B capture the game window? (A yellow capture border may flash on Windows 10.)
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
