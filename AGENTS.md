# Agent guidelines

These rules bind every automated change made in this repository. They take precedence over an
agent's defaults, including any default commit attribution.

## Comments

- A comment is a single line, written only where runtime intent or non-obvious logic needs it.
- No multi-line commentary, no narration of what changed, no section banners.
- No references to external brands, competitors or third-party infrastructure providers.
- Before creating a commit, list the comment lines being added:

  ```sh
  git diff --cached | grep -E '^\+.*//|^\+.*/\*'
  ```

  Remove every hit that breaks the rules above, and keep a hit only after confirming it is needed.

## Commits

- Commit small, incremental changes straight onto `main`. Do not create feature branches.
- Never run `git push`. Pushing starts the remote pipelines; the owner decides when that happens.
- Pause and ask for confirmation before committing a sweeping architectural refactor.
- A commit message is one line: a Conventional Commit subject. No body, no co-author trailer, no
  other trailers.

## Interface work

- When adapting an element from an external reference, take its functional anatomy and interaction
  mechanics only. Do not carry over its artwork, copy or branding.

## Identities

- Keep founder, executive and team member identities out of page copy, metadata and layouts.
- When an audit or a convention recommends showing who is behind the project (authors, maintainers,
  a team section), do not add it; report it as a skipped requirement.

## Working here

- `ARCHITECTURE.md` maps the crates and the path a frame takes through them.
- `cargo test` runs the unit tests and the corpus regression; `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` must pass.
- `cd app && npm run build` type-checks and builds the page with the WebAssembly reader.
- The reader must never return a wrong answer: a frame it cannot read with confidence is rejected.
  A change that trades this for fewer rejections needs the owner's confirmation.
