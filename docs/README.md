# Tigers and Sheep documentation

These pages explain how the game is built and how its computer players think. The top-level
[README](../README.md) covers the rules and how to play; start here when you want to change the code.

| Page | What it covers |
|------|----------------|
| [Building](building.md) | Toolchain, dependencies and what each is for, Cargo profiles and features, the Nix flake, CI |
| [Architecture](architecture.md) | The modules, how a game runs from `main` to the TUI loop, and how the pieces talk to each other |
| [The board](board.md) | The bitboard representation, move generation, the rules as implemented, symmetries and repetitions |
| [Players](players.md) | The `Player` and `MinimaxPlayer` traits, and the players that implement them |
| [The minimax search](search.md) | Alpha-beta, iterative deepening, the transposition table, move ordering, repetitions |
| [The neural network and training](training.md) | The network, backprop and Adam, and the self-play training loop with TD(λ) targets |
| [Benchmarking and profiling](benchmarking.md) | The `bench` command, search counters, `perf`/`samply`, and pitfalls when timing |
| [Adding a player](adding-a-player.md) | Step by step: a simple player, a minimax player, and wiring either into the command line |

## Conventions used throughout

- **Scores favour tigers.** Every score in the code, from `evaluate` to the network's output, is
  positive when it's good for the tigers and negative when it's good for the sheep. Tigers maximise,
  sheep minimise.
- **Coordinates.** A point is `Pos = (x, y)`, with `x` and `y` in `0..5`. `y = 0` is drawn at the
  top. In notation, files `a`–`e` are `x` 0–4 and ranks are `5 - y`, so `(0, 0)` is `a5`.
- **Move notation.** `sc3` places a sheep on c3, `sc3c2` moves a sheep, `ta5b4` is a tiger step and
  `ta5xc3` a tiger jump (which kills the sheep on b4).
- **Turns.** Sheep move first. `Turn` is the side *about to move*, and most functions take it
  alongside the board, since the same board can come up with either side to move.
