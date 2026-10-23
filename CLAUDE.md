# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A Rust implementation of "Tigers and Sheep" (a Bagh-Chal–style hunt game) on a 5x5 point board. The crate/binary is named `tigers-and-sheep-rs`. The UI uses ratatui (with its re-exported `ratatui::crossterm`).

## Environment & commands

The toolchain comes from a Nix flake (`flake.nix`, loaded via direnv `.envrc` → `use flake`); `shell.nix`/`default.nix` are flake-compat shims. Enter with `nix develop` if direnv isn't active.

- Build: `cargo build`
- Run: `cargo run`
- Test all: `cargo test`
- Single test: `cargo test move_tiger_diagonal_jump_kills_sheep` (any substring of the test name filters)
- Lint / format: `cargo clippy`, `cargo fmt`
- Nix package build (what CI runs in `.github/workflows/build_nix.yml`): `nix build`

### Benchmarking and profiling the search

- Bench: `cargo run --profile profiling -- bench [depth]` (default depth 6; 9 takes about 0.6 s) searches six fixed positions to a fixed depth without the TUI and prints the move, score and time for each. The positions come from `ClaudePlayer` playing itself to depth 4, so they are deterministic. An optimisation that only makes the search faster must leave every move and score unchanged.
- Counters: add `--features search-stats` to print nodes, nodes/s, cache hit/overwrite rates and the share of cutoffs on the first move. They are thread-local, bumped with the `count!` macro in `player.rs` (also used in `board.rs`), and compile to nothing without the feature. Run `cargo test` with and without the feature.
- The `profiling` Cargo profile is release plus debug info. The dev shell has `perf` and `samply`. As a non-root user, `perf record` needs `-m 16` (a small ring buffer) with `--call-graph dwarf`, otherwise mmap fails on the `perf_event_mlock_kb` limit: `perf record -e cycles:u -m 16 --call-graph dwarf target/profiling/tigers-and-sheep-rs bench 9`.
- Before comparing timings, check `/sys/devices/system/clocksource/clocksource0/current_clocksource`. On this machine the kernel has fallen back from `tsc` to `hpet`, which turns every `Instant::now()` (one per interior search node) into a syscall and nearly halves the speed. Results and history are in `plans.org`.

## Architecture

- `src/board.rs` — all game logic and its unit tests (`#[cfg(test)] mod tests` at the bottom).
- `src/player.rs` — the `Player` trait. Players are polled, not blocking: the coordinator calls `update(board, turn)` before each draw, `pick_move(board, turn) -> Option<Move>` once per loop (computer players search and return `Some`), and passes clicks/keys to `handle_input(Input, board, turn) -> Option<Move>`. `highlights()` gives points for the board to highlight, `reset()` is called on both players after a revert (drop any selection and last-move comments; it is *not* called after a normal move, so players clear their own selection when they return a move), `render(area, buf)` draws the current player's panel (default: its name), and `render_last_turn(area, buf)` draws end-of-turn comments on the move it just made (default: nothing). `impl Widget for &dyn Player` makes every player a ratatui widget. Also the private-ish `MinimaxPlayer` trait: implementors supply `evaluate` (positive favours tigers) and optionally `depth()` (at most `MAX_DEPTH` = 64)/`time_limit()`/`name()`, plus `set_last_search`/`last_search` to keep a `SearchReport` (move, depth reached, score) that the blanket impl's `render_last_turn` shows; a blanket impl makes every `MinimaxPlayer` a `Player`.
  - Search: alpha-beta (`minimax` checks the cache, `minimax_uncached` searches) with iterative deepening in `pick_move` until half the time limit has passed. Moves are tried in this order: the cache's best move (the `hint`), then tiger jumps (`get_moves` lists them first), then killer moves, then the rest. This is done by rotating moves to the front, because a `sort_by_key` here cost more than it saved.
  - `cache()` returns an optional `ScoreCache`, keyed by board, side to move and remaining depth, storing `CacheEntry` bounds. `TranspositionTable` is the fixed-size one (`new(bits)`: `1 << bits` slots of ~24 bytes, in two-slot buckets where one slot keeps the deepest entry; `clear()` bumps a generation number instead of wiping). The `HashMap` impl is kept as a simple reference for tests.
  - `killers()` returns optional `Killers`: two quiet moves per remaining depth that last caused a cutoff. `pick_move` clears both the cache and the killers.
  - Tests compare the search with plain minimax (`reference`) for every cache kind (`CacheKind::None`/`Map`/`TinyTable`, a table small enough to collide constantly) with and without killers.
- `src/game.rs` — `GameHandler::play_game(sheep_player, tiger_player)` runs a ratatui TUI (mouse capture on) until the user quits, returning the winner if there was one. Layout: board | move history on top; below, the current player's panel (70%) | the previous player's last-turn panel (30%). Panes overlap by one cell (`Layout::spacing(-1)`) and are built with `panel()` (`MergeStrategy::Exact`) so shared borders join up. The bottom row is drawn first and keeps its titles on its bottom border, because its top border is the same line as the board's bottom border, which lists the keybinds (keep them in step with `handle_event`). History is `Vec<(Move, GameBoard)>` (board *after* each move); the side to move comes from its length's parity (sheep first). Clicking a history row calls `revert(i)`, which goes back to the board *before* move `i`. The loop only waits for input (`event::read`) when `pick_move` returns `None`. After a computer's search, `pending_action` handles any input that came in meanwhile, so quit and revert still work in computer-vs-computer games. They take effect once the current search finishes.
- `src/board-view.rs` (module `board_view`) — `BoardView` widget (board with rank/file labels and highlights) and `pos_at`, which maps a mouse cell to the nearest point. Both pick the scale from the area (`scale_to_fit`: the largest whole-number scale whose `size(scale)` fits), so the coordinator sizes `board_area` to exactly `size(scale)` and the two agree. Pieces grow with the scale (`reach`): `T` at 1, `[T]` at 2, a 3x3 box from 3, which `size`, `cell` and `pos_at` all allow for. The history pane is a fixed 20 columns; the board pane gets the rest.
- `src/local-player.rs` (module `local_player`, pulled in via `#[path]` because of the hyphen) — `LocalPlayer`, a human using the mouse: click a point to place a sheep, or click a piece then one of its highlighted destinations.
- `src/claude-player.rs` — `ClaudePlayer`, the minimax computer player: `time_limit` and `max_depth` are public fields, with a 2^20-slot `TranspositionTable` and killer moves.
- `src/bench.rs` — the `bench` mode (see above).
- `src/main.rs` — `bench [depth]` runs the benchmark; otherwise it plays a game in the TUI (the sheep player first, then the tigers) and prints the winner. Which players it uses changes: `LocalPlayer` or `ClaudePlayer` on either side.

Notation (`pos_name`, `impl Display for Move` in board.rs): files `a`–`e` are x 0–4, ranks are `5 - y` (y = 0 is drawn at the top as rank 5). Moves are `sc3` (place), `sc3c2` (sheep move), `ta5b4` (tiger step), `ta5xc3` (tiger jump).

Win check: `check_win(turn)` is called with the side about to move, and true means the side that *just* moved has won. Tigers win once 5 sheep are dead (`sheep_count() + sheep_left() <= 15`); otherwise the side that just moved wins if the side to move has no legal moves. That check uses `has_moves(turn)`, which tests the same bitmasks as `get_moves` (shared through `sheep_steps`/`tiger_targets`) without building a list.

### Board representation (`GameBoard`)

The board is two `u32` bitmasks, `tiger_mask` and `sheep_mask`. This encoding is non-obvious:

- Point `(x, y)` maps to index `x + y*5` (0..24), and its bit is `1 << idx`. So board bits occupy the **low 25 bits** (`BOARD_MASK`), with `(0,0)` at bit 0.
- The **high 7 bits of `sheep_mask`** (from `SHEEP_COUNT_SHIFT = 25`) are not board points — they hold the count of sheep still in hand (starts at 20). `sheep_left()` reads it; `place_sheep` subtracts `ONE_SHEEP` (`1 << 25`) from `sheep_mask` directly. Killing a sheep does not change this count (it counts unplaced sheep, not living ones).
- `Pos = (u8, u8)` is `(x, y)`; out-of-range coordinates panic in `bit_idx`.
- Default layout: tigers in the four corners, 20 sheep in hand. `GameBoard::EMPTY` has no pieces and none in hand.
- `key()` packs the whole board into one `u64` (used for the transposition table's hash).

### Rules as implemented

- Movement follows board lines: orthogonal lines everywhere; diagonals only through points where `x + y` is even (`on_line`).
- Sheep: placed one at a time until none remain in hand; only then may they move one step along a line.
- Tigers: step one along a line, or jump two along a line over a sheep, which kills it.
- `check_move` returns `Err(MoveError::…)` for an illegal move; `make_move` runs it first and returns the error without touching the board. `make_move_unchecked` skips the checks (for moves from `get_moves`). `MoveError`'s `Display` gives a human-readable message. Tests assert on the specific `MoveError` variant.
- `bit_idx` still panics on out-of-range coordinates.

## Code style

Mostly tabs for indentation, with braces on their own line (Allman style). A few trait-impl blocks use 4-space indentation, and `cargo fmt` would reformat everything, so don't run it across the whole crate unless asked.
