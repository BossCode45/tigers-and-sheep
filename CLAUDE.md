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

## Architecture

- `src/board.rs` — all game logic and its unit tests (`#[cfg(test)] mod tests` at the bottom).
- `src/player.rs` — the `Player` trait. Players are polled, not blocking: the coordinator calls `update(board, turn)` before each draw, `pick_move(board, turn) -> Option<Move>` once per loop (computer players search and return `Some`), and passes clicks/keys to `handle_input(Input, board, turn) -> Option<Move>`. `highlights()` gives points for the board to highlight, `reset()` drops a half-made selection, and `render(area, buf)` draws the player's panel (default: its name). `impl Widget for &dyn Player` makes every player a ratatui widget. Also the private-ish `MinimaxPlayer` trait: implementors supply `evaluate` (positive favours tigers) and optionally `depth()`/`name()`, and a blanket impl makes every `MinimaxPlayer` a `Player`.
- `src/game.rs` — `GameHandler::play_game(sheep_player, tiger_player)` runs a ratatui TUI (mouse capture on) until the user quits, returning the winner if there was one. Layout: board | move history on top, the current player's panel below. The board's bottom border lists the keybinds; keep it in step with `handle_event`. History is `Vec<(Move, GameBoard)>` (board *after* each move); the side to move comes from its length's parity (sheep first). Clicking a history row calls `revert(i)`, which goes back to the board *before* move `i`.
- `src/board-view.rs` (module `board_view`) — `BoardView` widget (board with rank/file labels and highlights) and `pos_at`, which maps a mouse cell to the nearest point. Both pick the scale from the area (`scale_to_fit`: the largest whole-number scale whose `size(scale)` fits), so the coordinator sizes `board_area` to exactly `size(scale)` and the two agree. Pieces grow with the scale (`reach`): `T` at 1, `[T]` at 2, a 3x3 box from 3, which `size`, `cell` and `pos_at` all allow for. The history pane is a fixed 20 columns; the board pane gets the rest.
- `src/local-player.rs` (module `local_player`, pulled in via `#[path]` because of the hyphen) — `LocalPlayer`, a human using the mouse: click a point to place a sheep, or click a piece then one of its highlighted destinations.
- `src/claude-player.rs` — `ClaudePlayer`, the minimax computer player.
- `src/main.rs` — plays `LocalPlayer` (sheep) against `ClaudePlayer` (tigers) and prints the winner.

Notation (`pos_name`, `impl Display for Move` in board.rs): files `a`–`e` are x 0–4, ranks are `5 - y` (y = 0 is drawn at the top as rank 5). Moves are `sc3` (place), `sc3c2` (sheep move), `ta5b4` (tiger step), `ta5xc3` (tiger jump).

Win check: `check_win(turn)` is called with the side about to move, and true means the side that *just* moved has won. Tigers win once 5 sheep are dead (`sheep_count() + sheep_left() <= 15`); otherwise the side that just moved wins if the side to move has no legal moves.

### Board representation (`GameBoard`)

The board is two `u32` bitmasks, `tiger_mask` and `sheep_mask`. This encoding is non-obvious:

- Point `(x, y)` maps to index `x + y*5` (0..24), and its bit is `1 << idx`. So board bits occupy the **low 25 bits** (`BOARD_MASK`), with `(0,0)` at bit 0.
- The **high 7 bits of `sheep_mask`** (from `SHEEP_COUNT_SHIFT = 25`) are not board points — they hold the count of sheep still in hand (starts at 20). `sheep_left()` reads it; `place_sheep` subtracts `ONE_SHEEP` (`1 << 25`) from `sheep_mask` directly. Killing a sheep does not change this count (it counts unplaced sheep, not living ones).
- `Pos = (u8, u8)` is `(x, y)`; out-of-range coordinates panic in `bit_idx`.
- Default layout: tigers in the four corners, 20 sheep in hand.

### Rules as implemented

- Movement follows board lines: orthogonal lines everywhere; diagonals only through points where `x + y` is even (`on_line`).
- Sheep: placed one at a time until none remain in hand; only then may they move one step along a line.
- Tigers: step one along a line, or jump two along a line over a sheep, which kills it.
- `check_move` returns `Err(MoveError::…)` for an illegal move; `make_move` runs it first and returns the error without touching the board. `make_move_unchecked` skips the checks (for moves from `get_moves`). `MoveError`'s `Display` gives a human-readable message. Tests assert on the specific `MoveError` variant.
- `bit_idx` still panics on out-of-range coordinates.

## Code style

Mostly tabs for indentation, with braces on their own line (Allman style). A few trait-impl blocks use 4-space indentation, and `cargo fmt` would reformat everything, so don't run it across the whole crate unless asked.
