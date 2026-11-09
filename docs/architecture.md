# Architecture

The crate is a single binary, `tigers-and-sheep-rs`. It has three modes: play a game in the
terminal, benchmark the search, or train a neural network.

## Modules

```text
main.rs ─── command line (clap) ──┬── game.rs ─────── GameHandler: the TUI and game loop
                                  │      └── board-view.rs  BoardView widget, pos_at
                                  ├── bench.rs ────── fixed-depth search timings
                                  └── train.rs ────── self-play training

player.rs ───── Player trait, MinimaxPlayer trait + search, ScoreCache, TranspositionTable, Killers
  ├── local-player.rs   LocalPlayer     a human with a mouse
  ├── claude-player.rs  ClaudePlayer    minimax + hand-written evaluation
  └── neural-player.rs  NeuralPlayer    minimax + network evaluation
network.rs ──── Network (MLP), Adam, Rng, Sample
board.rs ────── GameBoard, Move, Turn, Outcome, rules, move generation, symmetries, repetitions
```

| File | Module | Contents |
|------|--------|----------|
| [`src/main.rs`](../src/main.rs) | crate root | `Cli` and its subcommands, `parse_player`, `make_player`, `play` |
| [`src/board.rs`](../src/board.rs) | `board` | All the game logic and its unit tests. See [The board](board.md). |
| [`src/player.rs`](../src/player.rs) | `player` | The `Player` trait, and the `MinimaxPlayer` trait with the whole search. See [Players](players.md) and [The minimax search](search.md). |
| [`src/game.rs`](../src/game.rs) | `game` | `GameHandler`, which owns the move history and runs the TUI loop |
| [`src/board-view.rs`](../src/board-view.rs) | `board_view` | `BoardView`, the board widget, and `pos_at`, which maps a mouse click back to a point |
| [`src/local-player.rs`](../src/local-player.rs) | `local_player` | `LocalPlayer` |
| [`src/claude-player.rs`](../src/claude-player.rs) | `claude_player` | `ClaudePlayer` |
| [`src/neural-player.rs`](../src/neural-player.rs) | `neural_player` | `NeuralPlayer` |
| [`src/network.rs`](../src/network.rs) | `network` | The neural network, its optimiser and file format. See [Training](training.md). |
| [`src/train.rs`](../src/train.rs) | `train` | Self-play training (`Config`, `run`) |
| [`src/bench.rs`](../src/bench.rs) | `bench` | The `bench` command. See [Benchmarking](benchmarking.md). |

Files with a hyphen in their name are pulled in with `#[path = "local-player.rs"] mod local_player;`
in `main.rs`, since a module name can't contain a hyphen.

## The command line

`main.rs` parses arguments with clap's derive API:

- `Cli` has an optional subcommand and, flattened in, the `PlayArgs`, so `tigers-and-sheep-rs -S human`
  works without writing `play`. `args_conflicts_with_subcommands` stops the two being mixed.
- `Command::Play(PlayArgs)` — `--sheep`/`-S` and `--tiger`/`-T` each take a `PlayerKind`, parsed
  by `parse_player`: `human`, `claude`, or the path of an existing file (taken as network weights).
  `--time`/`-t` is each computer player's time limit in seconds (default 2).
- `Command::Bench { depth }` — runs `bench::run(depth)`.
- `Command::Train(TrainArgs)` — every field of `train::Config` is an argument, and each default is
  read from `Config::default()`, so the defaults live in one place. `impl From<TrainArgs> for Config`
  converts them. Keep the two in step when adding a setting.

`make_player` turns a `PlayerKind` into a `Box<dyn Player>`, and `play` hands the two boxes to
`GameHandler::play_game` and prints the result.

## How a game runs

`GameHandler` ([`game.rs`](../src/game.rs)) holds the start board and the history, a
`Vec<(Move, GameBoard)>` with the board *after* each move. Everything else is derived from it:

- `board()` is the last history entry's board, or the start board.
- `turn()` comes from the history's length: even means sheep to move.
- `previous()` is every board before the current one, oldest first, used to detect repetitions.
- `outcome()` is a win if `board.check_win(turn)` (the side that just moved has won), otherwise a
  draw if the position has come up `REPETITION_DRAW` = 2 times before, otherwise `None`.

`play_game` enters the alternate screen with `ratatui::init()`, turns on mouse capture, runs the
loop, and restores the terminal however the loop ends. Each pass of the loop:

1. Calls `update(board, turn)` on the player to move, so it can show the position in its panel.
2. Draws the whole screen. This happens *before* asking for a move, so a computer player's
   "Thinking…" panel is on screen while it searches.
3. If the game isn't over, calls `pick_move(board, turn, &previous)`.
   - A computer player searches and returns `Some(move)`. Before playing it, `pending_action`
     handles any key presses and clicks that arrived during the search, without blocking. A quit or
     a revert there wins over the move. This is what makes quitting work in a computer-against-computer
     game, where nothing else ever waits for input.
   - A human player returns `None`, and the loop blocks on `event::read()` for the next event.
4. Handles the resulting `Action`:
   - **Quit** (`q`, `Esc`, `Ctrl+C`): returns the outcome.
   - **Play(move)**: `make_move` (which panics on an illegal move, so players must only return legal ones) and push onto the history.
   - **Revert(i)**: a click on history row `i` truncates the history to before move `i`, then calls
     `reset()` on both players so they drop selections and stale comments.
   - A click on the board goes to the player to move as `Input::Click(pos)`; other keys go as
     `Input::Key`. If `handle_input` returns a move, it is played.

Once the game is over the loop keeps running, showing the result, until the user quits or reverts.

### Screen layout

```text
┌ Board ──────────────────────────────┬ Moves ───────────┐
│                                     │  1. sc3   [↶]    │
│          (BoardView, scaled)        │  2. ta5b4 [↶]    │
│                                     │       …          │
├ click: select / move · q, Esc: quit ┼──── ↶ reverts ───┤
│ current player's panel (70%)        │ last turn (30%)  │
└───────────── Sheep: You ────────────┴ Last turn: … ────┘
```

- Neighbouring panes overlap by one cell (`Layout::spacing(-1)`) and are built with `panel()`,
  which uses `MergeStrategy::Exact`, so shared borders join into `┬ ┴ ┼`.
- The bottom row is drawn first and keeps its titles on its bottom border, because its top border
  is the same line as the board's bottom border, which lists the keybinds. If you change the keys in
  `handle_event`, change that title too.
- The current player's panel is drawn by `Player::render`; the other panel is the previous mover's
  `Player::render_last_turn` (for minimax players: the move, depth reached and score).
- The history pane is a fixed 20 columns and scrolls to keep the newest move visible. Sheep moves
  are white, tiger moves yellow.
- `BoardView` picks the largest whole-number scale that fits (`scale_to_fit`), and the coordinator
  sizes `board_area` to exactly `BoardView::size(scale)`. `pos_at` uses the same calculation to map
  a click back to the nearest point, so drawing and clicking always agree. Pieces grow with the
  scale: `T` at scale 1, `[T]` at 2, a 3×3 box from 3 on.

## Data flow between the pieces

```text
            GameHandler ──update/pick_move/handle_input──▶ dyn Player
                 │                                            │
                 │ make_move, check_win, repetitions          │ get_moves, after_move_unchecked,
                 ▼                                            ▼ check_win, evaluate
             GameBoard ◀──────────────────────────────── MinimaxPlayer search
                                                              │
                                                    ScoreCache / Killers / Network
```

The board is `Copy` and 8 bytes, so it is passed by value everywhere and nothing shares mutable
state. Players never see the `GameHandler`; they only get the board, the side to move and the
earlier boards.

Training (`train.rs`) doesn't use `GameHandler` at all. It has its own small game loop, `play`,
that calls players directly, so it runs without a terminal and in parallel. `bench.rs` likewise
calls `pick_move` directly.
