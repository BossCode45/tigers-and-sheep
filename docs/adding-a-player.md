# Adding a player

There are two kinds of player to add:

- **A minimax player** — a new evaluation function on top of the existing search. This is the
  easiest and usually what you want for a new computer opponent. Implement `MinimaxPlayer`.
- **Any other player** — a different algorithm (Monte Carlo tree search, a random mover, a player
  over the network, a keyboard-driven human…). Implement `Player` directly.

Then, either way, make it selectable on the command line.

Read [Players](players.md) first for what each trait method is for.

## Option A: a minimax player

Say we want a player that only counts dead sheep and tiger mobility. Create
`src/greedy-player.rs`:

```rust
use std::time::Duration;

use crate::{board::{GameBoard, Turn}, player::{Killers, MinimaxPlayer, ScoreCache, SearchReport, TranspositionTable}};

/// A computer player that searches like `ClaudePlayer` but only counts dead sheep and tiger moves.
pub struct GreedyPlayer
{
	pub time_limit: Duration,
	cache: TranspositionTable,
	killers: Killers,
	last_search: Option<SearchReport>,
}

impl GreedyPlayer
{
	pub fn new(time_limit: Duration) -> Self
	{
		Self { time_limit, cache: TranspositionTable::new(20), killers: Killers::default(), last_search: None }
	}
}

impl MinimaxPlayer for GreedyPlayer
{
	/// Positive favours tigers, whichever side is to move.
	fn evaluate(&mut self, board: &GameBoard, _turn: Turn) -> f32
	{
		let dead = 20 - board.sheep_count() - board.sheep_left();
		100.0 * dead as f32 + board.get_moves(Turn::Tiger).len() as f32
	}

	fn name(&self) -> String { "Greedy".to_string() }

	fn time_limit(&self) -> Duration { self.time_limit }

	fn cache(&mut self) -> Option<&mut dyn ScoreCache> { Some(&mut self.cache) }

	fn killers(&mut self) -> Option<&mut Killers> { Some(&mut self.killers) }

	fn set_last_search(&mut self, report: Option<SearchReport>) { self.last_search = report; }

	fn last_search(&self) -> Option<SearchReport> { self.last_search }
}
```

That's all: the blanket `impl<P: MinimaxPlayer> Player for P` gives it `pick_move` with iterative
deepening, the "Thinking…" panel and the last-turn report. **Don't also write `impl Player for
GreedyPlayer`**; it would conflict with the blanket impl.

Things to get right in `evaluate`:

- **Sign.** Positive is good for the tigers, negative good for the sheep, regardless of `turn`.
- **Finite values.** `±f32::INFINITY` means a won game to the search; an evaluation that returns it
  would be read as a forced win and stop iterative deepening.
- **Symmetry.** With a cache, the search shares one entry between the 8 symmetric boards, so
  `evaluate` should give them the same score. Anything built from counts (pieces, moves, jumps) is
  symmetric automatically; something like "sheep on the left half" is not.
- **Speed.** `evaluate` runs at every leaf, which is most of the nodes. `get_moves` allocates a
  `Vec`; the bitmask accessors (`tiger_points`, `sheep_points`, `sheep_count`) are much cheaper.

`cache`, `killers` and the `last_search` pair are optional. Leave them out and the player still
works, just searching more slowly and with an empty last-turn panel.

## Option B: any other player

Implement `Player`. A computer player does its work in `pick_move` and returns `Some`. Here is a
player that moves at random, using the crate's own `Rng` from `network.rs`
(`src/random-player.rs`):

```rust
use crate::{board::{GameBoard, Move, Turn}, network::Rng, player::Player};

/// Plays a uniformly random legal move.
pub struct RandomPlayer
{
	rng: Rng,
}

impl RandomPlayer
{
	pub fn new(seed: u64) -> Self
	{
		Self { rng: Rng::new(seed) }
	}
}

impl Player for RandomPlayer
{
	fn name(&self) -> String { "Random".to_string() }

	fn pick_move(&mut self, board: GameBoard, turn: Turn, _previous: &[GameBoard]) -> Option<Move>
	{
		// pick_move is only called while the game is on, so there is always a move
		let moves = board.get_moves(turn);
		Some(moves[self.rng.below(moves.len())])
	}
}
```

Things to keep in mind:

- **Return only legal moves.** The coordinator `unwrap`s `make_move`, so an illegal move panics.
  Pick from `board.get_moves(turn)`, or check with `board.check_move(mv)`.
- **Don't block forever.** The coordinator can't redraw or react to `q` while `pick_move` runs. Input
  that arrives meanwhile is handled as soon as it returns. Keep a time limit.
- **Repetitions.** `previous` holds every earlier board in the game. `board::repetitions(previous,
  &next)` says how often a position came up before; `REPETITION_DRAW` (2) earlier occurrences make a
  draw.
- **Interactive players** return `None` from `pick_move` and produce their move from
  `handle_input`, as `LocalPlayer` does. Use `update` to remember the position for `render`,
  `highlights` to show a selection, and clear your own selection when you return a move. `reset`
  is called after the user reverts, so drop any selection there too.
- **Panels.** Override `render` for the panel shown on your turn and `render_last_turn` for comments
  on your last move, shown during the opponent's turn. Both draw with ratatui into the given `area`
  of `buf`, e.g. `Paragraph::new(text).render(area, buf)` (with `ratatui::widgets::Widget` in scope).

## Wiring it into the command line

All in [`src/main.rs`](../src/main.rs).

**1. Declare the module.** File names with a hyphen need `#[path]`:

```rust
#[path = "greedy-player.rs"]
mod greedy_player;
```

and import the type: `use crate::greedy_player::GreedyPlayer;` (or add it to the existing
`use crate::{…}` list).

**2. Add a `PlayerKind` variant:**

```rust
enum PlayerKind
{
	Human,
	Claude,
	Greedy,
	Neural(PathBuf),
}
```

**3. Parse its name** in `parse_player`, before the file-path case, and mention it in the error:

```rust
"greedy" => Ok(PlayerKind::Greedy),
…
_ => Err(format!("expected `human`, `claude`, `greedy` or a weights file, but `{s}` is none of these")),
```

**4. Construct it** in `make_player`:

```rust
PlayerKind::Greedy => Box::new(GreedyPlayer::new(time_limit)),
```

**5. Update the help text** on the `sheep` and `tiger` fields of `PlayArgs` (their doc comments are
what `--help` shows), and the player list in the top-level `README.md`.

Then:

```bash
cargo run --release -- -S human -T greedy
cargo run --release -- -S greedy -T claude --time 1
```

If the new type is only used from `main.rs` through `make_player`, everything is reachable and there
are no dead-code warnings. A constructor or field that nothing uses yet will warn; that's a sign to
either use it or remove it.

## Testing it

Put tests in a `#[cfg(test)] mod tests` at the bottom of the player's file, as the other players do.
Useful checks:

- `pick_move` from a few positions returns a move with `board.check_move(m) == Ok(())`.
- For a minimax player, `last_search()` reports the move it returned.
- A player with a clear preference picks the obvious move: `player.rs` has a position where one
  tiger jump wins the game (`tiger_takes_the_winning_kill`) that is easy to copy.

To see how it plays, play it against `ClaudePlayer` in the TUI with both sides computer-controlled:

```bash
cargo run --release -- -S greedy -T claude
```

For a proper strength comparison over many games, `train.rs`'s `match_score` and `play` show how to
run games without the TUI: `play` takes a closure that returns `(move, score)` for a position, so
any `Player` can be plugged in.

## Using it in training

`train.rs` plays its matches against `ClaudePlayer` (`claude(depth)`). To measure the network
against your player instead, add a constructor like `claude` for it and pass it to `match_score`,
which accepts any `MinimaxPlayer + Player`. Keep the match opponents out of the training data:
they are a yardstick, and the network is meant to learn from self-play alone.
