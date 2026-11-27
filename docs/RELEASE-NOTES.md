## New in 0.3.0

- **Breacher in a browser.** <https://leecois.github.io/breacher/> needs no download. Press
  **Share the game screen**, choose the game's window or its screen, and the page reads each
  breach as it opens and counts off your picks, planning again after a pick off the route. The
  picture is read inside the page and goes nowhere else.
- **A route that floats.** A page cannot draw over the game, so in Chrome and Edge **Float the
  route over the game** opens a small window that stays above a borderless game. Elsewhere the
  route is on the page, for a second screen.
- **Choose what is captured.** **Capture** in the desktop window lists the open windows and the
  displays. Leave it on "The game, found by its name" unless Breacher does not find your game, as
  can happen when it runs through a streaming service: then pick its window. A whole display works
  too, but its route stays for a fixed time, because only a window is followed pick by pick.
- Screen sharing was tested by playing recorded looks at a real breach into the page, not yet
  with a live game. If it misses a breach, **Read now** reads the screen as it is.

## New in 0.2.9

- **Keeps up with fast play.** Breacher now looks at the game every 0.4 seconds while a route is
  drawn, where it was 0.7, so the ring moves to the next pick sooner.
- **Leaves at the end.** The route is taken down the moment the game has settled every daemon. It
  used to linger for up to five seconds over the screen that follows a breach.
- The log now records what the game did with each daemon and, at the end, the result next to what
  the route promised. A saved report keeps the looks at which something happened.

## New in 0.2.8

- **Follows your picks.** While a route is drawn over the game, Breacher keeps looking at the
  game window and sees each pick you make. Played steps disappear from the route over the game and
  fade in the window, the next pick is ringed in white, and the line under the matrix counts
  ("Pick 5 of 7").
- **Recovers from a wrong pick.** A pick off the route is not the end: the rest is planned again
  from where the breach stands, and the new route replaces the old one over the game.
- This was built from frames of a breach played on the current game (2.5, 2560×1440) and passes on
  those; it has not yet been watched working live. When a look does not fit what Breacher knows it
  changes nothing, so the worst case is the route as it was before. On Windows 10 picks are not
  followed, for the same reason the route stays 45 seconds there.
- **Fuller reports.** A saved report now holds every look Breacher took at the last breach it
  followed, about one every 0.7 seconds for the last 17 seconds, including how the breach ended.

## New in 0.2.7

- **Check the rule with a breach.** What the game does to a sequence when a pick does not continue
  it is not documented, so Breacher has been planning routes that work whatever it does, and
  sometimes giving up a daemon for it. In the Plan panel choose **Check the rule with a breach**,
  capture a breach you can spare, play exactly the picks drawn (the line over the game says "Rule
  check") and tell Breacher what uploaded. One or two breaches settle it.
- **Reports show the breach being played.** While a route is up, Breacher keeps the last few frames
  of the game window in memory; **Save a report** writes them next to the capture. Nothing is
  written or sent unless you save a report and pass it on.

## New in 0.2.6

- **Told in the game.** When a capture cannot be read, a line over the game says why for a few
  seconds: no breach screen, a breach already under way, a code that is covered.
- **Save a report.** At the bottom right of the window. It writes the last frame, what was read
  from it, the log and the versions into `Documents\Breacher reports` and opens the folder. Attach
  those files when something is wrong; nothing is sent anywhere by Breacher itself.
- **Start when you sign in.** Tick it and Breacher is in the tray, hotkey ready, every time you log
  on.

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
