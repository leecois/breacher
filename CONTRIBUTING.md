# Contributing

## What helps most

Labelled captures: from the current game patch, at 5×5, in HDR, on ultrawide screens and in other
languages. [docs/CORPUS.md](docs/CORPUS.md) explains how to add one. A capture that the reader gets
wrong or refuses is as useful as one it reads.

Contributions are accepted under the project's dual licence, MIT or Apache-2.0 at the user's
option, as the README describes.

## Setting up

You need a Rust toolchain (1.90 or newer) with the `wasm32-unknown-unknown` target, and Node.js 22
or newer.

```sh
cargo test                                   # unit tests and the corpus regression
cargo run --release -p breach-cli -- read corpus/cpah/5.png

cd app && npm install
npm run dev                                  # the page
npm run tauri dev                            # the desktop app
```

[ARCHITECTURE.md](ARCHITECTURE.md) maps the crates and explains the reader and the solver.

## Before you send a change

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
cd app && npm run build
```

If the change touches the reader, also run the sweep and quote its summary line:

```sh
cargo run --release -p breach-cli -- eval corpus/cpah --loo --full
```

The reader must never return a wrong reading. A change that turns a refusal into a wrong answer is
not an improvement, whatever it does to the other numbers.

## Conventions

- **Commits** have a one-line [Conventional Commits](https://www.conventionalcommits.org) subject,
  such as `fix(reader): keep column 0 when the title rule is faint`.
- **Comments** are single lines, written only where intent or logic is not obvious from the code.
  Longer reasoning belongs in `ARCHITECTURE.md`.
- **Formatting** follows `rustfmt.toml` and `.editorconfig`.
- **The core stays portable.** `breach-core` has no platform code and must keep building for
  WebAssembly.
