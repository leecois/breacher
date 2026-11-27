# Adding captures to the corpus

The reader's accuracy claims are only as good as the captures behind them. Every new capture is both a
regression test and, through `learn-templates`, more evidence for the glyph templates.

## What is most needed

- Captures from the **current game patch**.
- **5×5** matrices, and buffers of 4, 5, 7, 10 and more.
- **HDR** on, captured the way the app would capture it.
- Ultrawide (21:9, 32:9) and 4K.
- Languages other than English and Simplified Chinese.
- A breach with one daemon already installed, and one with a quest daemon.

## How to add one

1. Take the screenshot before the first pick, with the cursor off the matrix. Keep the original PNG.
2. Put it in `corpus/cpah/` (or a new folder beside it with its own `labels.json`).
3. Add an entry to `labels.json`. Codes are indices into `1C 55 7A BD E9 FF`:

   ```json
   "12.png": {
     "negative": false,
     "matrix": [[3, 2, 1, 1, 1, 1], "... one row per matrix row ..."],
     "buffer": 6,
     "sequences": [[1, 1, 1], [4, 0, 1], [2, 3, 4, 1]],
     "style": "latin"
   }
   ```

   `breach read 12.png` prints what the reader sees, which is a quick way to draft the entry. Check
   every code against the image before saving it: a wrong label is worse than no label.
   Use `"negative": true` (and nothing else) for frames that must be rejected. Use a new `style` name
   for a language whose codes are drawn in a different font.

4. Run

   ```sh
   breach eval corpus/cpah --loo           # is it read with templates learned from the others?
   breach eval corpus/cpah --loo --full    # and under the perturbation sweep?
   breach calibrate corpus/cpah            # do its layout offsets match the rest?
   ```

5. If it reads, rebuild the embedded templates so they include it, and re-run the tests:

   ```sh
   breach learn-templates corpus/cpah
   cargo test
   ```

If `calibrate` shows offsets that differ from the other captures (a moved sequence column, a different
row pitch), the layout has changed: adjust `Layout` in `crates/breach-core/src/reader.rs`, or make the
constant a range, and say which patch needed it.
