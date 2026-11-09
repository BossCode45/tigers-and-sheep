# The minimax search

Both computer players, `ClaudePlayer` and `NeuralPlayer`, use the same search. It lives in the
default methods of the `MinimaxPlayer` trait in [`src/player.rs`](../src/player.rs), so a player only
supplies an evaluation function and some storage, and gets the whole search for free. (See
[Players](players.md) for the trait itself.)

The search is alpha-beta minimax with iterative deepening, a transposition table shared between
symmetric positions, killer moves, and draw detection by repetition.

## Scores

- Scores are `f32` (`ScoreType`), from the **tigers' point of view**: tigers maximise, sheep
  minimise. (This is plain minimax, not negamax, so there is a `match turn` wherever the two sides
  differ.)
- `evaluate(board, turn)` scores a position where the search stops. It should return a finite
  number.
- A finished game scores `±WIN`, where `WIN = f32::INFINITY`. When `check_win(turn)` is true the
  side that just moved has won, so with tigers to move the score is `-WIN` (sheep won) and with
  sheep to move it is `+WIN`.
- A position that repeats an earlier one scores `DRAW` = 0.

Because every forced win is the same `±∞`, the search itself can't tell a win in 3 from a win in 9.
Iterative deepening makes up for it: it stops at the first depth that finds a forced win, so the
move it plays is one that wins in the fewest plies.

## Iterative deepening: `pick_move`

The blanket `impl<P: MinimaxPlayer> Player for P` implements `pick_move`:

1. Clear the cache and the killers. Cache entries would still be valid for the next move (the key
   includes the depth), but clearing bounds the memory used and with `TranspositionTable` it is
   nearly free.
2. Start `path` as the game's earlier boards (`previous`), for repetition checks.
3. Search the root to depth 1, 2, 3, … up to `depth()` (at most `MAX_DEPTH` = 64). Each pass calls
   `minimax_uncached` on the root with the previous pass's best move as the `hint`, so it's tried
   first.
4. Stop when:
   - a pass runs out of time (it returns `None`): its partial result is thrown away and the last
     finished pass's move is kept;
   - a pass finds a forced win or loss (`|score| >= WIN`): more depth can't change it;
   - more than **half** the time limit has gone: the next pass takes several times longer than the
     last, so it probably wouldn't finish anyway.
5. Store a `SearchReport { mv, depth, score }` with `set_last_search`, and return the move.

Depth 1 ignores the deadline, so there is always a move to play however short the time limit is.

Why deepen at all, instead of searching to one depth? Because the time a search takes is hard to
predict, and because each pass makes the next one faster: the cache remembers the best move found in
every position, and trying that first gives far more alpha-beta cutoffs.

## Alpha-beta: `minimax_uncached`

`minimax_uncached(board, turn, depth, alpha, beta, hint, deadline, path)` returns the position's
score and the move that gave it, or `None` if the deadline passed.

1. If `check_win(turn)`: return `±WIN`.
2. If `depth == 0`: return `evaluate(board, turn)`.
3. If the deadline has passed: return `None`. This is checked only at interior nodes, after the two
   cheap cases, since reading the clock isn't free.
4. Generate and order the moves (below).
5. For each move: push `board` onto `path`, call `minimax` on the child with `depth - 1`, pop it.
   Tigers raise `alpha` to the best score so far, sheep lower `beta`. Once `alpha >= beta` the rest
   of the moves can't matter to the parent, so stop (a **cutoff**). A quiet move that caused a cutoff
   becomes a killer.

The window `alpha..beta` is what the parent can still use. A score at or below `alpha` only means
"no better than this", and one at or above `beta` only "at least this". The search is *fail-soft*:
it returns the real best score it found even when that falls outside the window, which gives the
cache tighter bounds.

Ties keep the first move, so even a position where every move loses still returns one.

## The cache: `minimax`

`minimax` wraps `minimax_uncached` with the transposition table. Many move orders reach the same
position, especially while sheep are being placed, so remembering results saves a great deal of work.

### What an entry holds

A `CacheEntry { lower, upper, best }` records what a search proved: the true score lies in
`lower..=upper`, and `best` is the move that gave it. Entries are keyed by the board, the side to
move, and the remaining depth, since a deeper search of the same position gives a better answer.

After searching, the entry depends on how the score compares with the window it was searched with:

| Result | Entry | Why |
|--------|-------|-----|
| `score <= alpha` (fail low) | `lower = -∞, upper = score, best = None` | Every move was only bounded from above, so none is known to be best |
| `score >= beta` (fail high) | `lower = score, upper = +∞, best` | The search stopped at a cutoff, so the score is only a lower bound |
| inside the window | `lower = upper = score, best` | The exact score |

If there was already an entry, the two are intersected (the larger `lower`, the smaller `upper`).

### Using an entry

Before searching, `minimax` looks the position up:

- `lower >= beta`: the position is already good enough to cut off. Return `lower`.
- `upper <= alpha`, or `lower == upper`: return `upper`.
- Otherwise narrow the window to `max(alpha, lower)..min(beta, upper)` and search, trying the entry's
  `best` move first.

If there's no entry at this depth, it looks up the same position at `depth - 1` — the entry left by
the previous iterative-deepening pass — and uses its best move as the hint.

The root position is searched with `minimax_uncached` directly, so it never goes in the cache.

### Symmetric positions share entries

The board has 8 symmetries that preserve the rules (see [The board](board.md#symmetries)), and
symmetric positions have the same score. So `minimax` looks positions up by `board.canonical()`, and
all 8 orientations share one entry. The best move is stored as played on the canonical board and
turned back with `transform_move(m, inverse_symmetry(sym))` when it's read.

This assumes `evaluate` gives symmetric boards the same score. `ClaudePlayer`'s evaluation does.
A `Network` only does approximately, since it learns symmetry from training on transformed positions;
the small differences don't matter in practice.

In the fully symmetric opening this cut the number of nodes searched by more than four times.

### `TranspositionTable`

The `ScoreCache` trait has `lookup`, `insert` and `clear`. There are two implementations:

- `HashMap<(GameBoard, Turn, usize), CacheEntry>`: simple and exact, kept as a reference for tests.
- `TranspositionTable`: what the players use. `new(bits)` allocates `1 << bits` slots of about
  24 bytes once (2^20 slots, about 24 MB, for the players), so nothing grows or rehashes during a
  search.

How the table works:

- **Hashing.** The board's `key()` is mixed with the depth and turn, multiplied by the 64-bit golden
  ratio constant `0x9E3779B97F4A7C15` (Fibonacci hashing), and the top bits pick a **bucket of two
  slots**.
- **Exact keys.** Each slot keeps the full board, turn and depth, so a collision can lose an entry
  but never return the wrong one.
- **Replacement.** The first slot of a bucket keeps the deepest entry, and the second takes
  everything else. A new entry at least as deep as the first slot's moves that entry down to the
  second slot rather than losing it. Without this, the 1.5 million leaf-level entries of a deep
  search pushed out the few deep ones that save the most work: an always-replace table needed about
  35% more nodes.
- **Clearing by generation.** Every slot stores the generation it was written in. `clear()` just
  increments the generation, so all old slots count as empty. After 255 clears the counter would come
  back round to an old slot's generation, so then it really wipes the table.

## Move ordering

Alpha-beta cuts off most when the best move is tried first. In a perfectly ordered tree it searches
roughly the square root of the nodes plain minimax does. Moves are tried in this order:

1. **The hint**: the cache's best move for this position, or the previous pass's best move at the
   root.
2. **Tiger jumps.** `get_moves` lists them first. A kill is usually the best tiger move.
3. **Killer moves.** For each remaining depth, `Killers` keeps the last two quiet (non-jump) moves
   that caused a cutoff there. Within one pass of iterative deepening, a remaining depth is a fixed
   distance from the root, so killers are moves that refuted a *sibling* position, and often refute
   this one too. A new killer goes in front of the old one, and a move already in front isn't added
   twice.
4. **Everything else**, in generated order.

This is done by rotating each chosen move to the front of the list (`rotate_right(1)` on a slice),
not by sorting: a `sort_by_key` cost about a quarter of the search's time and made it slower overall.
With this ordering, about 97% of cutoffs come on the first move tried.

## Repetitions

`minimax` and `minimax_uncached` carry `path`: the game's earlier boards (from `previous`) followed
by the boards along the current search line, pushed and popped around each child.

At every non-root position, `minimax` first checks `repetitions(path, &board) > 0`, and if so
returns `DRAW` before looking at the cache. This is a **twofold** rule, stricter than the game's
threefold one. The reasoning: a side that can repeat a position once can keep repeating it until the
game is drawn, so the first repetition is already as good as a draw. This lets the side that is
behind find draws, and the side that is ahead avoid them, much earlier.

The cache doesn't know the path, so an entry whose score was influenced by a repetition can be
slightly wrong for the same board reached a different way. Chess engines accept the same
inaccuracy. Since every path includes the root, it can't happen below depth 5, which is why the
tests at depths 3 and 4 against the reference search (which applies the same rule) are exact.

## Correctness tests

The tests in `player.rs` compare the search against `reference`, a plain minimax with no pruning, no
cache and no ordering, on positions from a range of pseudo-random games:

- alpha-beta with no cache gives exactly the same score as `reference`;
- so does every cache kind (`CacheKind::Map`, and `CacheKind::TinyTable`, a table of 8 slots that
  collides constantly), with and without killers;
- any legal hint gives the same score;
- a search that runs out of time stores nothing in the cache;
- `pick_move` respects the time limit, and picks a move whose score equals the best;
- repetitions score as draws, a tiger who is ahead avoids one, a sheep who is behind takes one;
- a tiger takes a winning kill.

An optimisation that only makes the search faster must keep all of these passing and must leave
every move and score in `bench` unchanged (see [Benchmarking](benchmarking.md)).

## Search statistics

With `--features search-stats`, the `count!` macro increments thread-local `SearchStats` counters:
nodes, leaves, terminal positions, repetitions, cache probes, hits, returns and overwrites, cutoffs,
first-move cutoffs and move generations. `SearchStats::take()` reads and resets them. Without the
feature the macro expands to nothing.
