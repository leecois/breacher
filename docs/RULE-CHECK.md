# Settling the wrong-pick rule

A daemon uploads when the buffer matches its sequence. What is not documented anywhere is what
happens to a daemon's progress when a pick does **not** continue its sequence. Three rules are
plausible, and they give different answers:

| Rule | On a wrong pick | `A A B` with buffer `A A A B` | `A B` with buffer `A A B` |
|---|---|---|---|
| `substring` | nothing is lost: the sequence only has to appear somewhere in the buffer | uploads | uploads |
| `retest` | progress resets, then the same pick is checked against the first code | does not upload | uploads |
| `strict` | progress resets and that pick is dropped | does not upload | does not upload |

Neither reset rule contains the other: `A A B` with buffer `A A A A A B` uploads under `strict` and
not under `retest`. The tests in `crates/breach-core/src/solver.rs` pin these cases down.

In generated game-like puzzles the best achievable reward differs between rules in up to 9% of
three-daemon puzzles and up to 44% of seven-daemon ones (`breach bench-solver`, column `sens%`).

## What the tool does until the rule is known

It plans a route that uploads the same daemons under all three rules. When plain substring matching
would upload more, that route is offered separately and labelled as depending on an unconfirmed rule.

## Finding out by playing

Whenever Breacher follows a breach to its end, in the desktop program or in a browser with the
game shared, it reads the codes left in the buffer from the screen that follows and the bar the
game has laid over each daemon, green for installed and red for failed. Each rule still in play
predicts which daemons those codes upload; a rule that predicts something else than the game did
is dropped. Nothing is concluded unless the buffer reads the same twice, starts with the picks that
were seen, and every daemon is settled, and a result that no rule predicts changes nothing.

Most breaches tell nothing, because a route that works under every rule gives the same result
under every rule. A breach with a wrong pick in it often does. The check below is the quick way.

## Finding out in the app

1. In the Plan panel, under "What a wrong pick does", choose **Check the rule with a breach**.
2. Open a breach you can spare and capture it as usual. The route drawn, in the window and over the
   game, is now a rule check: it is chosen to tell the rules apart, not to upload the most, and the
   line over the game says so.
3. Play exactly those picks. Breacher reads the result from the game; where it cannot, it asks you
   what the game uploaded.

Most screens separate one rule from the other two, so the first answer either settles the rule or
rules one out; in the second case the next breach you capture goes on with the check, and one more
answer settles it. Meanwhile routes are planned to work under the rules still in play. If what
uploaded matches none of the choices, say so: nothing is changed, and a saved report with a note of
what uploaded is worth sending.

## Finding out on the command line

1. Open a breach at an access point and take a screenshot before the first pick.
2. Run

   ```sh
   breach rule-probe screenshot.png
   ```

   It prints a route and, for each rule, which daemons that route uploads:

   ```
   picks: r1c1 1C -> r2c1 BD -> r2c3 BD -> r3c3 BD -> r3c2 55 -> r6c2 55 -> r6c7 1C -> r2c7 7A

   Follow these picks exactly, then note which daemons the game marks as uploaded:
     #1, #2     -> the game matches plain substrings   (--matcher substring)
     #1         -> progress resets, the pick is re-read (--matcher retest)
     nothing    -> progress resets, the pick is dropped (--matcher strict)
   ```

3. Follow the picks in game and compare what uploaded with the three lines.
4. If the screen separates only two of the rules, or has no probe at all, back out before picking,
   reopen the breach for a new matrix and try again.

The probe route is chosen to tell the rules apart, not to earn the best reward, so use an access point
you do not mind spending.

## Once you know

- Command line: `breach read screenshot.png --matcher substring` (or `retest`, `strict`).
- Library: `PlanOptions { matcher: Some(Matcher::Substring), .. }`.

With the rule fixed the planner returns the single best route under it and stops offering a second one.
Please also record the result here, with the game version, so nobody has to repeat it.
