# Tigers and Sheep

A terminal version of Tigers and Sheep, a hunt game in the style of Bagh-Chal, written in Rust with
[ratatui](https://ratatui.rs). Play against a friend, a minimax computer player, or a neural
network that learnt the game by playing itself.

## Rules

The board is a 5×5 grid of points joined by lines. Every point has horizontal and vertical lines,
and diagonals run through every other point (those where x + y is even).

- **Tigers** start in the four corners. A tiger steps one point along a line, or jumps two points
  along a line over a sheep, which kills that sheep.
- **Sheep** start off the board, 20 of them. Each sheep turn places one sheep on an empty point,
  until all 20 are placed. After that, a sheep turn moves one sheep a step along a line.
- Sheep move first, then the sides take turns.
- **Tigers win** by killing 5 sheep. **Either side wins** if the other has no legal move on its
  turn, so sheep win by trapping every tiger.

## Running

The toolchain comes from a Nix flake. With direnv it loads by itself; otherwise:

```bash
nix develop
```

Then start a game:

```bash
cargo run --release
```

With plain Rust (no Nix), `cargo` from a recent stable toolchain works too.

### Controls

- **Click** an empty point to place a sheep, or click a piece and then one of its highlighted destinations.
- **Click a move in the history** to go back to just before it.
- **q**, **Esc** or **Ctrl+C** quits.

By default the computer plays itself. Pick the players with `--sheep` and `--tiger`: `human`
(you, with the mouse), `claude` (the minimax computer player), or a weights file for a neural
player. `--time` sets how many seconds a computer player may think per move (default 2).

```bash
cargo run --release -- --sheep human --tiger claude --time 5
```

`--help` lists every command and option.

## Computer players

- **`ClaudePlayer`** searches with alpha-beta minimax and iterative deepening. It uses a
  transposition table shared between symmetric boards, and killer moves. It scores positions with
  a hand-written evaluation: dead sheep, sheep left open to a jump, and trapped tigers.
- **`NeuralPlayer`** uses the same search, but scores positions with a small neural network.
  The network only sees the raw position (where the pieces are, sheep left to place, and whose
  move it is) and learns everything else by self-play.

### Training the neural player

```bash
cargo run --release -- train [generations] [games per generation] [out dir] [--from <weights>]
```

The defaults are 50 generations of 500 games, saved to `weights/`. Each generation, the network
plays itself, learns from how those games went, then plays matches against the previous generation
and against `ClaudePlayer`. The score against `ClaudePlayer` is the best guide to progress.
`ClaudePlayer` is only a yardstick, never something the network learns from.

Each generation is saved as `gen-NNN.bin`, and the newest as `latest.bin`. Running `train` again
carries on from the newest generation in the folder. `--from` starts from any weights file instead.
The other training settings (search depth, ε, λ, epochs and so on) are options too; see
`train --help`.

Then play against it, as sheep (the default) or as tigers:

```bash
cargo run --release -- neural weights/latest.bin tiger
```

## Development

```bash
cargo test
cargo clippy
cargo run --profile profiling -- bench [depth]
```

`bench` searches six fixed positions to a fixed depth and prints each move, score and time. Add
`--features search-stats` to also count nodes, cache hits and cutoffs. `plans.org` keeps a history
of the optimisations and training experiments, with their results. `CLAUDE.md` describes the code's
structure in more depth.
