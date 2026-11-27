# Security

## What the app can do

Breacher captures the screen only when you press its hotkey or its capture button, and reads image
files only from a folder you choose to watch or that you drop or paste in. It writes two files, its
settings and a plain-text log, in its own application data folder. **Save a report** copies the last
frame, up to eight frames of the game window from the last breach that was followed, the reading
and that log into a folder under your documents and nowhere else; sending it is up to you. "Start when you sign in" adds, and its opposite removes, one sign-in entry for Breacher.

The desktop app makes one kind of network request: at start it fetches a small file from the
project's release page to learn whether a newer version exists. The request carries nothing about
you, your screen or the game, and "Look for updates at start" turns it off. An update is downloaded
only when you ask for it, and it is installed only if it carries the project's signature for that
version. The command-line tool makes no network requests.

## Supported versions

Fixes are made on the latest release only.

## Reporting a vulnerability

Please report it privately through the repository's **Security** tab (Report a vulnerability), not
in a public issue. Include the version, the operating system and the steps to reproduce.
