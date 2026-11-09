# The neural network and training

`NeuralPlayer` uses the same [minimax search](search.md) as `ClaudePlayer`, but scores positions with
a small neural network instead of hand-written rules. The network starts out random and learns only
by playing against itself. No human idea of a good position goes into it, and `ClaudePlayer` is
only ever used to measure progress, never as something to learn from.

The network is in [`src/network.rs`](../src/network.rs) and the training loop in
[`src/train.rs`](../src/train.rs). Neither uses a machine-learning library: the forward pass, the
backpropagation and the optimiser are all written by hand.

## The network

A multilayer perceptron:

```text
52 inputs ──▶ 64 ReLU ──▶ 32 ReLU ──▶ 1 tanh
```

The output is in `-1..1` and, like every score, positive favours the tigers.

### Inputs

Only the raw position, nothing derived from it:

| Inputs | Meaning |
|--------|---------|
| 0..25 | 1 if a tiger is on that point (bit `x + 5y`) |
| 25..50 | 1 if a sheep is on that point |
| 50 (`HAND`) | Sheep in hand / 20 |
| 51 (`TIGER_TO_MOVE`) | 1 if tigers are to move |

The network has to work out for itself that dead sheep are bad for the sheep, that a sheep with an
empty point behind it can be jumped, and so on.

### Parameters

All the weights and biases live in one flat `Vec<f32>` (`params`), at fixed offsets `W1`, `B1`,
`W2`, `B2`, `W3`, `B3`, `PARAMS` in all (about 5,500). That lets the optimiser, the gradient and the
file format treat them as one list of numbers.

- `W1` is **input-major**: input `i`'s weights to the 64 hidden units are contiguous, at
  `W1 + i*64 .. W1 + (i+1)*64`.
- `W2` is **unit-major**: hidden-2 unit `k`'s 64 incoming weights are contiguous.

### Forward pass

Most inputs are 0: at most 4 tiger bits and 20 sheep bits are set out of 50. So instead of a full
matrix multiply, `forward_full` starts layer 1 at its biases and **adds the weight row of each set
input**, then adds `hand × W1[HAND]` and applies ReLU. That's why `W1` is input-major. `for_active`
walks the set bits of the board masks with `trailing_zeros`. Layers 2 and 3 are ordinary dot
products, then `tanh`.

`forward_matches_a_plain_matrix_multiply` checks this against the textbook calculation.

### Initialisation

`Network::random(seed)` uses He initialisation: normal weights scaled by `sqrt(2 / fan_in)`, with
zero biases. For layer 1 the fan-in is taken as 20 rather than 52, since only about a third of the
inputs are set at once.

### Backpropagation

The loss for one sample is the squared error `(out - target)² / 2`. `add_gradient` runs the forward
pass, keeping each layer's activations, then works backwards:

1. Through `tanh`: `d_out = (out - target) × (1 - out²)`.
2. Layer 3's weight gradients are `d_out × h2`. The gradient flowing into each hidden-2 unit is
   `d_out × W3[k]`, or 0 where that ReLU was off.
3. The same again through layer 2 into hidden layer 1, skipping units with no gradient.
4. Layer 1: only the set inputs have non-zero gradients, so again only their rows are touched.

`gradient_matches_finite_differences` checks every layer's gradients against numerical estimates.

### Parallel and deterministic

`gradient(batch)` splits a batch into chunks of `GRAD_CHUNK` = 32 samples, computes each chunk's
gradient on rayon's thread pool, then **adds the chunks up in order**. `loss` does the same in chunks
of `LOSS_CHUNK` = 1024. Floating-point addition isn't associative, so summing in a fixed order is
what makes the result identical whatever the number of threads
(`training_is_the_same_on_any_number_of_threads`). Training is therefore reproducible: the same
seed gives the same network on any machine with any `RAYON_NUM_THREADS`.

### The optimiser: Adam

`train_batch` averages the gradient over the batch and takes one `Adam` step. Adam keeps running
averages of each parameter's gradient (`m`, decay `β1` = 0.9) and squared gradient (`v`, decay
`β2` = 0.999), corrects their bias towards their zero start, and moves each parameter by
`learning_rate × m̂ / (√v̂ + ε)`. In effect every parameter gets its own step size, which suits a
network where some inputs (a sheep on a rarely used point) are seen far less often than others.

### File format

`save` and `load` use a small binary format, all little-endian:

| Bytes | Contents |
|-------|----------|
| 0..4 | `TSNN` |
| 4..8 | version (`u32`, currently 1) |
| 8..20 | inputs, hidden 1, hidden 2 (`u32` each) |
| 20.. | every parameter as `f32`, in `params` order |

`load` rejects files of the wrong size, magic, version or layer sizes with `InvalidData`. So
changing the layer sizes makes old weights fail to load, and changing what an input *means* without
changing the sizes calls for bumping `VERSION`.

## Self-play training

```bash
cargo run --release -- train [generations] [games] [out-dir] [--from <weights>] [options]
```

Training runs in **generations**. Each one plays a batch of games, turns them into training samples,
trains the network on them, and saves it.

### 1. Starting point

`run` starts from `--from` if given, otherwise the newest `gen-NNN.bin` in the out dir, otherwise
random weights (seed `0x5EED`). Generation numbers carry on from the newest file in the out dir
either way, so a run can be stopped and restarted. The Adam state and the sample buffer aren't saved,
so they start empty on a restart.

### 2. Self-play

Each generation plays `games_per_generation` (500) games of the network against itself, in parallel
across all cores (`parallel`, on rayon's pool). Each game:

- uses a `NeuralPlayer` that searches to exactly `depth` (3) plies. A depth limit rather than a time
  limit keeps runs reproducible, and the game's seed is derived from the generation and game number;
- plays random moves for the first `random_plies` (10) plies, so games start from different places;
- after that, plays a random move instead of the searched one with probability `epsilon` (0.05),
  to explore positions the network would never choose;
- is a draw on a threefold repetition (`play` passes the earlier boards to the search, so the
  search sees repetitions too), or after `ply_cap` (200) plies, a backstop for games that wander
  without repeating.

Every position is recorded as a `Ply` with the search's score for it (clamped to `-1..1`, so forced
wins become ±1), and whether a random move was played there.

### 3. Labelling positions: TD(λ)

Each position needs a target, the score the network should learn to give it. Two obvious choices
both have problems:

- **The game's result** (+1, -1 or 0) is the truth, but noisy: a position early in the game had many
  moves, some of them random, between it and the result.
- **The search score** of the next position is less noisy but biased, since it comes from the
  network itself.

TD(λ) blends them, working backwards from the end of the game:

```text
target[last] = result
target[t]    = (1 - λ) × score[t+1] + λ × target[t+1]
```

With `λ = 1` every position is labelled with the result; with `λ = 0` each is labelled with the next
position's search score. The default `lambda` is 0.7.

**Random moves cut the chain.** A position where a random move was played is labelled with its own
search score instead, and the positions before it then build on that, so they don't learn from the
random move's consequences. This matters because random moves aren't equally bad for both sides: a
random sheep move usually leaves a sheep to be jumped, while a random tiger move costs little.
Without the cut, the network learnt that tigers nearly always win.

### 4. The sample buffer

The samples of each generation are kept, split by outcome (tiger win, sheep win, draw), and the
network trains on the last `buffer_generations` (5) generations together. Training on more than the
latest games smooths out the network's swings from one generation to the next.

### 5. Balancing outcomes

Self-play ends mostly in tiger wins, largely because of those random sheep moves. Trained on
everything, the network would mostly learn what tiger wins look like. So with `balance_outcomes`
(on by default; `--no-balance` turns it off), `balanced` builds each epoch from:

- as many positions from sheep-win games as from tiger-win games: their mean, so the epoch keeps the
  same size. The bigger pool gives a random subset (every position at most once), and the smaller one
  gives every position plus random repeats;
- every position from drawn games, once.

If either kind of win has no positions at all there is nothing to balance, and everything is used.

### 6. Training

For each of `epochs` (2) epochs: build the epoch's samples, shuffle them, show each in a **random one
of the 8 symmetric orientations** (`GameBoard::transformed`), and train in batches of `batch_size`
(256) with Adam at `learning_rate` (1e-3). The random orientations are data augmentation: they teach
the network that symmetric positions are equally good, and multiply the variety of positions it sees.

### 7. Matches

On every generation whose number is a multiple of `match_every` (10), the network plays two matches
of `match_games` (100) games each, alternating sides, at the same depth:

- against the generation that last played matches (N − 10), loaded from its `gen-NNN.bin`, or the
  network the run started from if that file is missing;
- against `ClaudePlayer`.

Each pair of games (one as each side) shares a seed, so both sides get the same 4 random opening
moves. The score is the share of points won, with a draw worth half. Matches are slow and only
measure progress, so other generations skip them.

### 8. Saving

Each generation is saved as `out_dir/gen-NNN.bin` and copied to `out_dir/latest.bin`.

## Reading the output

```text
 gen trained tigers  sheep  draws T before S before  T after  S after vs prev vs Claude train s  time s
 499  128657    460     22     18   0.0161   0.0211   0.0194   0.0165       -         -    0.26     1.3
 500  128335    461     17     22   0.0198   0.0185   0.0161   0.0229     45%       82%    0.28     2.2
```

| Column | Meaning |
|--------|---------|
| `gen` | Generation number |
| `trained` | Positions in each epoch |
| `tigers`, `sheep`, `draws` | How this generation's self-play games ended |
| `T before`, `S before` | Mean error on positions from tiger-win and sheep-win games in the buffer, before training |
| `T after`, `S after` | The same after training |
| `vs prev`, `vs Claude` | Match scores (only every `match_every` generations; `-` otherwise) |
| `train s`, `time s` | Seconds spent training, and for the whole generation |

**Judge progress by `vs Claude`.** Self-play results say little, since the random moves skew them
towards tigers, and the errors measure how well the network fits its own targets, not how well it
plays. `vs prev` above 50% means the network beats its older self.

## Settings

Every `train::Config` field is a command-line option (`train --help`):

| Option | Default | Meaning |
|--------|---------|---------|
| `generations` (positional) | 50 | Generations to train |
| `games` (positional) | 500 | Self-play games per generation |
| `out_dir` (positional) | `weights` | Where weights are saved and resumed from |
| `--from` | – | Start from this weights file |
| `--depth` | 3 | Search depth in self-play and matches |
| `--ply-cap` | 200 | A game this long is a draw |
| `--random-plies` | 10 | Random opening plies in self-play |
| `--epsilon` | 0.05 | Chance of a random move after that |
| `--lambda` | 0.7 | TD(λ) blend |
| `--buffer-generations` | 5 | Generations of samples to train on |
| `--no-balance` | off | Train on all positions without balancing outcomes |
| `--epochs` | 2 | Passes over the samples per generation |
| `--batch-size` | 256 | Positions per optimiser step |
| `--learning-rate` | 1e-3 | Adam's learning rate |
| `--match-games` | 100 | Games per match |
| `--match-every` | 10 | Play matches every this many generations |

`RAYON_NUM_THREADS` limits the number of threads; the results don't depend on it.

Then play the result:

```bash
cargo run --release -- -S human -T weights/latest.bin
```
