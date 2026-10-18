# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A Rust implementation of "Tigers and Sheep" (a Bagh-Chal–style hunt game) on a 5x5 point board. The crate/binary is still named `rust-hello` (leftover from the template), not `tigers-and-sheep`.

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
- `src/player.rs` — the `Player` trait (`pick_move(board, turn) -> Move`). Also the private `MinimaxPlayer` trait: implementors supply `evaluate` (positive favours tigers) and optionally `depth()`, and a blanket impl makes every `MinimaxPlayer` a `Player`.
- `src/game.rs` — `GameHandler::play_game(sheep_player, tiger_player)` runs the game loop. Sheep move first, and it returns the winning `Turn`.
- `src/local-player.rs` (module `local_player`, pulled in via `#[path]` because of the hyphen) — `LocalPlayer`, a human at the terminal: renders the board, lists legal moves, and parses a move number or coordinates from stdin.
- `src/main.rs` — plays a game between two `LocalPlayer`s and prints the winner.

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
