# Players

A side in the game is a `Player`. The game coordinator doesn't know or care whether it's a human
clicking, a search, or a network: it asks the player for a move, passes on input, and draws whatever
the player wants to show. Both traits are in [`src/player.rs`](../src/player.rs).

## The `Player` trait

```rust
pub trait Player
{
	fn name(&self) -> String;
	fn update(&mut self, _board: GameBoard, _turn: Turn) {}
	fn pick_move(&mut self, _board: GameBoard, _turn: Turn, _previous: &[GameBoard]) -> Option<Move> { None }
	fn handle_input(&mut self, _input: Input, _board: GameBoard, _turn: Turn) -> Option<Move> { None }
	fn highlights(&self) -> Highlights { Highlights::default() }
	fn reset(&mut self) {}
	fn render(&self, area: Rect, buf: &mut Buffer) { /* shows name() */ }
	fn render_last_turn(&self, _area: Rect, _buf: &mut Buffer) {}
}
```

Only `name` is required. Players are **polled, not blocking**: no method waits for anything, so the
coordinator can keep redrawing and handling quit keys whatever kind of player is on move.

| Method | When it's called | What to do |
|--------|------------------|------------|
| `name` | For panel titles and the result | A short name, e.g. `"Claude"` |
| `update(board, turn)` | Before every redraw, while it's this player's turn | Remember the position if `render` needs it |
| `pick_move(board, turn, previous)` | Once per loop, while it's this player's turn and the game isn't over | Computer players: search and return `Some(move)`. Interactive players: return `None`. `previous` is every earlier board in the game, oldest first, for spotting repetitions. |
| `handle_input(input, board, turn)` | When the user clicks a board point (`Input::Click(pos)`) or presses a key (`Input::Key(event)`) during this player's turn | Return `Some(move)` once the input completes a move |
| `highlights()` | When drawing the board during this player's turn | `Highlights { selected, targets }`: a selected piece and the points it can move to |
| `reset()` | On **both** players after the user reverts to an earlier move | Drop any half-made selection and any comments about the last move. It is *not* called after a normal move, so a player clears its own selection when it returns a move. |
| `render(area, buf)` | Every redraw, while it's this player's turn | Draw the player's panel. The default shows `name()`. |
| `render_last_turn(area, buf)` | Every redraw during the *other* player's turn | Draw comments on the move this player just made. The default draws nothing. |

Rules a player must follow:

- **Only return legal moves.** The coordinator plays them with `make_move(..).unwrap()`, so an
  illegal move panics. Moves from `board.get_moves(turn)` are always legal; `board.check_move(mv)`
  checks any other.
- `pick_move` is never called once the game is over, so `get_moves(turn)` is never empty there.
- Input that arrives while a computer player is searching is handled after the search, so a long
  `pick_move` delays quitting until it returns. Keep a time limit.

`impl Widget for &dyn Player` makes every player a ratatui widget by forwarding to
`Player::render`, which is how the coordinator draws the panel. (A blanket
`impl<P: Player> Widget for &P` would break the orphan rule.)

### Where each method shows up on screen

```text
┌ Board ───────────────────────────┬ Moves ─────┐
│   highlights() of the player     │            │
│   to move are drawn here         │            │
├──────────────────────────────────┼────────────┤
│ render() of the player to move   │ render_last│
│                                  │ _turn() of │
│                                  │ the other  │
└────────── Sheep: name() ─────────┴────────────┘
```

## `LocalPlayer`: a human with a mouse

[`src/local-player.rs`](../src/local-player.rs). It's the example of an interactive player:

- `pick_move` keeps the default `None`, so the coordinator waits for input.
- `handle_input` takes clicks. While sheep are in hand, a click on an empty point places a sheep.
  Otherwise a click on one of its own pieces selects it and keeps its legal moves; a click on one of
  their destinations plays that move; any other click clears the selection. Illegal clicks leave a
  message in red, from `MoveError`'s `Display`.
- `highlights` returns the selected piece and its destinations.
- `update` stores the position so `render` can show the sheep counts and an instruction.
- `render_last_turn` shows `Played <move>`.
- `reset` clears the selection, message and last move.

## The `MinimaxPlayer` trait

Every computer player so far is a search with a different evaluation. `MinimaxPlayer` holds that
search, and **a blanket impl makes every `MinimaxPlayer` a `Player`**:

```rust
impl<P: MinimaxPlayer> Player for P { … }
```

So a computer player implements `MinimaxPlayer` only, never `Player` directly (the two impls would
conflict).

```rust
pub trait MinimaxPlayer
{
	const WIN: f32 = f32::INFINITY;
	const DRAW: f32 = 0.0;

	// Required
	fn evaluate(&mut self, board: &GameBoard, turn: Turn) -> ScoreType;

	// Optional settings
	fn depth(&self) -> usize { MAX_DEPTH }                     // at most 64
	fn time_limit(&self) -> Duration { Duration::from_secs(1) }
	fn name(&self) -> String { "Computer".to_string() }

	// Optional storage that speeds up the search
	fn cache(&mut self) -> Option<&mut dyn ScoreCache> { None }
	fn killers(&mut self) -> Option<&mut Killers> { None }

	// Optional storage for the last-turn panel
	fn set_last_search(&mut self, _report: Option<SearchReport>) {}
	fn last_search(&self) -> Option<SearchReport> { None }

	// The search itself; not meant to be overridden
	fn minimax(…) -> Option<ScoreType>;
	fn minimax_uncached(…) -> Option<(ScoreType, Option<Move>)>;
}
```

| Item | Purpose |
|------|---------|
| `evaluate(board, turn)` | The score of a position where the search stops. Positive favours tigers, whichever side is to move; `turn` is there for evaluations that care. Return finite values: `±WIN` (infinity) is reserved for won games. If the player has a cache, give symmetric boards the same score (see [the search](search.md#symmetric-positions-share-entries)). |
| `depth()` | The most plies to search, counting the move itself. Normally the time limit stops the search first. |
| `time_limit()` | How long `pick_move` may take. It stops deepening after half of it. |
| `cache()` | Return a `ScoreCache` (normally a `TranspositionTable`) to turn on the transposition table. |
| `killers()` | Return a `Killers` to turn on killer-move ordering. |
| `set_last_search` / `last_search` | Store and return the `SearchReport { mv, depth, score }` of the last `pick_move`, which the blanket impl's `render_last_turn` shows as "Played … / Searched to depth … / Score: …". |
| `WIN`, `DRAW` | Scores of a won game and of a repetition |

The blanket `Player` impl provides:

- `pick_move`: iterative deepening with the time limit (see [The minimax search](search.md)).
- `render`: the name and "Thinking…", which is on screen while the search runs.
- `render_last_turn`: the search report, with the score to 5 significant figures, or "forced win for
  tigers/sheep".
- `reset`: forgets the search report.

### Supporting types

| Type | Purpose |
|------|---------|
| `ScoreType` | `f32` |
| `SearchReport` | The move, depth and score of the last search |
| `CacheEntry` | Bounds `lower..=upper` on a position's score and its best move |
| `ScoreCache` | Trait for caches: `lookup`, `insert`, `clear`. Implemented by `TranspositionTable` and (for tests) `HashMap` |
| `TranspositionTable` | Fixed-size cache: `new(bits)` gives `1 << bits` slots of ~24 bytes |
| `Killers` | Two killer moves per remaining depth |
| `SearchStats` | Counters filled in with the `search-stats` feature |
| `MAX_DEPTH` | 64 |

## `ClaudePlayer`

[`src/claude-player.rs`](../src/claude-player.rs). A `MinimaxPlayer` with a 2^20-slot
`TranspositionTable`, killers, and a hand-written evaluation from the tigers' side:

```text
100 × dead sheep            five dead sheep win the game for the tigers
+20 × jumps available now   sheep left hanging
−15 × trapped tigers        tigers with no move; trapping all of them wins for the sheep
 +2 × tiger moves           mobility in general, which the sheep win by squeezing to zero
```

`time_limit` and `max_depth` are public fields. `ClaudePlayer::new(time_limit)` makes one; the
`bench` and `train` code set `max_depth` and a huge time limit to get fixed-depth, repeatable
searches.

## `NeuralPlayer`

[`src/neural-player.rs`](../src/neural-player.rs). The same as `ClaudePlayer`, except that
`evaluate` returns `self.net.forward(board, turn)`, the [network](training.md)'s output in `-1..1`.
`NeuralPlayer::new(net, time_limit, table_bits)` lets training use a smaller table (2^16 slots),
since it runs one player per thread. Forced wins are still `±WIN` from the search itself, far
outside the network's range.
