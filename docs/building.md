# Building

## Quick start

```bash
nix develop            # or let direnv do it: the repo's .envrc says `use flake`
cargo run --release    # play a game (computer against computer by default)
cargo test
```

Without Nix, any stable Rust toolchain from **1.87** on works (the code uses
`unsigned::is_multiple_of`, stabilised in 1.87). The flake currently pins rustc 1.95.

```bash
rustup toolchain install stable
cargo run --release
```

Always use `--release` (or the `profiling` profile) for anything that searches or trains. A debug
build searches many times slower, so the computer players reach much shallower depths in the same
time limit and training takes hours longer.

## The toolchain: Nix

| File | Purpose |
|------|---------|
| `flake.nix` | The real definition: the package and the dev shell |
| `flake.lock` | Pins the exact revisions of the flake's inputs |
| `.envrc` | `use flake`, so [direnv](https://direnv.net) enters the dev shell when you `cd` in |
| `shell.nix`, `default.nix` | flake-compat shims, so `nix-shell` and `nix-build` work without flakes |

### Flake inputs

| Input | Used for |
|-------|----------|
| `nixpkgs` (26.05) | Everything: rustc, cargo and the dev tools |
| `naersk` | Builds the crate as a Nix package (`defaultPackage = naersk-lib.buildPackage ./.`), reading `Cargo.lock` so dependencies are fetched reproducibly |
| `flake-utils` | `eachDefaultSystem`, so the package and shell exist for every common platform |

### What the dev shell provides

| Tool | Why it's there |
|------|----------------|
| `cargo`, `rustc` | Building and testing |
| `rustfmt` | Formatting (but see the note on style below) |
| `clippy` | Lints: `cargo clippy` |
| `rust-analyzer` | Editor support; the shell sets `RUST_SRC_PATH` so it can find the standard library's source |
| `pre-commit` | Git hook runner, for anyone who sets up hooks |
| `perf`, `samply` | Profiling the search (see [Benchmarking and profiling](benchmarking.md)) |

`nix build` builds the release binary into `./result/bin/tigers-and-sheep-rs`. This is also what
the GitHub workflow in `.github/workflows/build_nix.yml` runs on every push. Note that `.github` is
currently listed in `.gitignore`, so that workflow only exists locally and isn't committed.

## Crate dependencies

The crate keeps its dependencies few. Everything else (the board, the search, the neural network,
its optimiser, the random number generator, the weights file format) is written by hand.

| Crate | Version | What it does here |
|-------|---------|-------------------|
| [`ratatui`](https://ratatui.rs) | 0.30 | The terminal UI: layout, bordered panes, paragraphs and the `Widget` trait that `BoardView` and every `Player` implement. It re-exports `crossterm`, which the game uses through `ratatui::crossterm` for raw mode, mouse capture and reading key and mouse events. Don't add `crossterm` as a direct dependency: the re-export keeps the two versions in step. |
| [`clap`](https://docs.rs/clap) | 4.6, feature `derive` | The command line (`Cli`, `Command`, `PlayArgs`, `TrainArgs` in `main.rs`), including `--help` text taken from the doc comments |
| [`rayon`](https://docs.rs/rayon) | 1.12 | Parallelism for training only: self-play games and matches run across all cores, and the network's gradient and loss are computed in parallel chunks. The game itself is single-threaded. |

## Cargo profiles

| Profile | Command | Use |
|---------|---------|-----|
| `dev` | `cargo build`, `cargo test` | Day-to-day development and tests |
| `release` | `cargo run --release` | Playing and training |
| `profiling` | `cargo build --profile profiling` | Release optimisations plus debug info, so profilers can show function names and source lines. The binary lands in `target/profiling/`. |

## Cargo features

| Feature | Effect |
|---------|--------|
| `search-stats` | Counts nodes, leaves, cache probes and hits, cutoffs and more during the search, and makes `bench` print them. Without it the `count!` macro compiles to nothing, so it costs nothing. |

Run the tests both with and without it when touching the search, since some code only exists in
one configuration:

```bash
cargo test
cargo test --features search-stats
```

## Running

```bash
cargo run --release -- --help
cargo run --release -- -S human -T claude --time 5      # you play sheep against ClaudePlayer
cargo run --release -- -S weights/latest.bin -T human   # a trained network plays sheep
cargo run --release -- train 50 500 weights             # train a network
cargo run --profile profiling -- bench 9                # time the search
```

The game needs a terminal that supports mouse reporting and Unicode box-drawing characters; any
modern terminal emulator does. The board grows to fill whatever space the window gives it.

## Generated files

| Path | What | In git? |
|------|------|---------|
| `target/` | Cargo's build output | No |
| `result` | Symlink to the `nix build` output | No |
| `weights/` | Trained networks, `gen-NNN.bin` and `latest.bin` | No |
| `.direnv/` | direnv's cache of the dev shell | No |

## Code style

Tabs for indentation and braces on their own line (Allman style). A few trait-impl blocks use
4-space indentation. `cargo fmt` would reformat the whole crate to a different style, so only run it
on a file you're happy to see restyled, never across the crate.
