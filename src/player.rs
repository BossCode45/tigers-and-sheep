use std::{collections::HashMap, time::{Duration, Instant}};

use ratatui::{buffer::Buffer, crossterm::event::KeyEvent, layout::Rect, widgets::{Paragraph, Widget, Wrap}};

use crate::board::{is_jump, GameBoard, Move, Pos, Turn};

/// Input the game coordinator passes on to the player whose turn it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input
{
	/// A click on a board point.
	Click(Pos),
	Key(KeyEvent),
}

/// Points for the coordinator to highlight on the board.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Highlights
{
	pub selected: Option<Pos>,
	pub targets: Vec<Pos>,
}

/// A side in the game. The coordinator calls `pick_move` once per loop while it's this player's
/// turn, and passes on input with `handle_input`; whichever returns a move first plays it.
pub trait Player
{
	fn name(&self) -> String;

	/// Called before each draw while it's this player's turn, with the position to play.
	fn update(&mut self, _board: GameBoard, _turn: Turn) {}

	/// Computer players search here and return Some. Interactive players return None and
	/// wait for `handle_input`.
	fn pick_move(&mut self, _board: GameBoard, _turn: Turn) -> Option<Move> { None }

	/// A click on a board point, or a key press, while it's this player's turn.
	fn handle_input(&mut self, _input: Input, _board: GameBoard, _turn: Turn) -> Option<Move> { None }

	/// Points for the coordinator to highlight on the board.
	fn highlights(&self) -> Highlights { Highlights::default() }

	/// The game was reverted: drop any half-made selection, and anything said about the last move.
	fn reset(&mut self) {}

	/// Draw into the player-output panel while it's this player's turn. By default this shows the player's name.
	fn render(&self, area: Rect, buf: &mut Buffer)
	{
		Paragraph::new(self.name()).render(area, buf);
	}

	/// Draw some end-of-turn comments on the move this player just made, shown beside the other
	/// player's panel during their turn. By default this shows nothing.
	fn render_last_turn(&self, _area: Rect, _buf: &mut Buffer) {}
}

/// Every player is a ratatui widget through `Player::render`.
/// (A blanket `impl<P: Player> Widget for &P` would break the orphan rule.)
impl Widget for &dyn Player
{
	fn render(self, area: Rect, buf: &mut Buffer)
	{
		Player::render(self, area, buf);
	}
}

pub type ScoreType = f32;

/// What a `MinimaxPlayer`'s last `pick_move` found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SearchReport
{
	pub mv: Move,
	/// The deepest search that finished, in plies counting the move itself.
	pub depth: usize,
	/// The score of `mv` at `depth`; positive favours tigers, and ±`WIN` is a forced win.
	pub score: ScoreType,
}

/// What a search has proved about a board's true score: it lies in `lower..=upper`.
/// An alpha-beta cutoff only proves one side, so the other is left infinite.
/// `best` is the move that gave the score, tried first when the position is searched again deeper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CacheEntry
{
	pub lower: ScoreType,
	pub upper: ScoreType,
	pub best: Option<Move>,
}

/// Score cache for a `MinimaxPlayer`. No defaults, so an implementor must supply all of it.
/// Entries are keyed by the side to move and the remaining search depth as well as the board,
/// since the same board can be reached with either side to move, or at different depths.
pub trait ScoreCache
{
	fn lookup(&mut self, board: &GameBoard, turn: Turn, depth: usize) -> Option<CacheEntry>;
	fn insert(&mut self, board: &GameBoard, turn: Turn, depth: usize, entry: CacheEntry);
	fn clear(&mut self);
}

impl ScoreCache for HashMap<(GameBoard, Turn, usize), CacheEntry>
{
    fn lookup(&mut self, board: &GameBoard, turn: Turn, depth: usize) -> Option<CacheEntry> {
        self.get(&(*board, turn, depth)).copied()
    }

    fn insert(&mut self, board: &GameBoard, turn: Turn, depth: usize, entry: CacheEntry) {
        self.insert((*board, turn, depth), entry);
    }

    fn clear(&mut self) {
        self.clear();
    }
}

/// A fixed-size score cache. Each position hashes to a bucket of two slots: the first keeps
/// whichever entry was searched deepest, so the many shallow entries near the leaves can't push
/// it out, and the second takes everything else.
/// Slots keep their full key, so a collision loses an entry but never returns the wrong one.
/// It is allocated once, so unlike a `HashMap` it never grows or rehashes during a search.
pub struct TranspositionTable
{
	slots: Vec<Slot>,
	/// Bits of the slot index, so there are `1 << bits` slots in `1 << (bits - 1)` buckets.
	bits: u32,
	/// Slots from an older generation count as empty, which makes `clear` cheap.
	/// 0 is never current, so `EMPTY_SLOT`s start out empty.
	generation: u8,
}

#[derive(Clone, Copy)]
struct Slot
{
	board: GameBoard,
	turn: Turn,
	depth: u8,
	generation: u8,
	entry: CacheEntry,
}

const EMPTY_SLOT: Slot = Slot
{
	board: GameBoard::EMPTY,
	turn: Turn::Sheep,
	depth: 0,
	generation: 0,
	entry: CacheEntry { lower: f32::NEG_INFINITY, upper: f32::INFINITY, best: None },
};

impl Slot
{
	fn holds(&self, board: &GameBoard, turn: Turn, depth: usize, generation: u8) -> bool
	{
		self.generation == generation && self.board == *board && self.turn == turn && self.depth as usize == depth
	}
}

impl TranspositionTable
{
	/// A table of `1 << bits` slots (about 24 bytes each).
	pub fn new(bits: u32) -> Self
	{
		assert!(bits >= 1, "the table needs at least one bucket of two slots");
		Self { slots: vec![EMPTY_SLOT; 1 << bits], bits, generation: 1 }
	}

	/// The index of the first slot of the position's bucket.
	fn bucket(&self, board: &GameBoard, turn: Turn, depth: usize) -> usize
	{
		let key = board.key() ^ (depth as u64) << 1 ^ matches!(turn, Turn::Sheep) as u64;
		((key.wrapping_mul(0x9E3779B97F4A7C15) >> (65 - self.bits)) as usize) << 1
	}
}

impl ScoreCache for TranspositionTable
{
	fn lookup(&mut self, board: &GameBoard, turn: Turn, depth: usize) -> Option<CacheEntry>
	{
		let b = self.bucket(board, turn, depth);
		self.slots[b..b + 2].iter().find(|s| s.holds(board, turn, depth, self.generation)).map(|s| s.entry)
	}

	fn insert(&mut self, board: &GameBoard, turn: Turn, depth: usize, entry: CacheEntry)
	{
		let generation = self.generation;
		let b = self.bucket(board, turn, depth);
		let [deep, any] = &mut self.slots[b..b + 2] else { unreachable!() };
		let slot = if deep.holds(board, turn, depth, generation) || any.holds(board, turn, depth, generation)
		{
			// Update the position's own entry, wherever it is
			if deep.holds(board, turn, depth, generation) { deep } else { any }
		}
		else if deep.generation != generation || depth >= deep.depth as usize
		{
			// The deep slot's entry moves down rather than being lost
			*any = *deep;
			deep
		}
		else
		{
			any
		};
		#[cfg(feature = "search-stats")]
		if slot.generation == generation && !slot.holds(board, turn, depth, generation)
		{
			count!(cache_overwrites);
		}
		let depth = u8::try_from(depth).expect("search depth fits in a u8");
		*slot = Slot { board: *board, turn, depth, generation, entry };
	}

	fn clear(&mut self)
	{
		self.generation = self.generation.wrapping_add(1);
		// After 255 clears the oldest slots' generation would come round again, so empty them for real
		if self.generation == 0
		{
			self.slots.fill(EMPTY_SLOT);
			self.generation = 1;
		}
	}
}

/// The deepest search a `MinimaxPlayer` may do, in plies.
pub const MAX_DEPTH: usize = 64;

/// Killer moves: for each remaining search depth, the last two quiet (non-jump) moves that cut a
/// search off there. Within one pass of iterative deepening a remaining depth is a ply from the
/// root, and a move that refuted one position at a ply often refutes its siblings too.
#[derive(Clone, Debug)]
pub struct Killers([[Option<Move>; 2]; MAX_DEPTH + 1]);

impl Default for Killers
{
	fn default() -> Self { Self([[None; 2]; MAX_DEPTH + 1]) }
}

impl Killers
{
	pub fn clear(&mut self) { *self = Self::default(); }

	fn get(&self, depth: usize) -> [Option<Move>; 2]
	{
		self.0.get(depth).copied().unwrap_or_default()
	}

	fn add(&mut self, depth: usize, m: Move)
	{
		if let Some(k) = self.0.get_mut(depth)
		{
			if k[0] != Some(m)
			{
				*k = [Some(m), k[0]];
			}
		}
	}
}

/// Counts of what the search did, for profiling. They are only counted with the `search-stats`
/// feature; without it `count!` compiles to nothing and `SearchStats::take` returns zeros.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchStats
{
	/// Calls to `minimax_uncached`, i.e. positions searched rather than answered from the cache.
	pub nodes: u64,
	/// Positions scored with `evaluate`.
	pub leaves: u64,
	/// Positions where `check_win` ended the search.
	pub terminal: u64,
	pub cache_probes: u64,
	pub cache_hits: u64,
	/// Cache hits whose bounds settled the score without searching.
	pub cache_returns: u64,
	/// Cache entries lost because another position's entry took their slot.
	pub cache_overwrites: u64,
	/// Searches stopped early because alpha met beta.
	pub cutoffs: u64,
	/// Cutoffs on the first move tried; a high share means the moves are well ordered.
	pub first_move_cutoffs: u64,
	/// Calls to `GameBoard::get_moves`, from the search, `check_win` and `evaluate` alike.
	pub move_gens: u64,
}

#[cfg(feature = "search-stats")]
thread_local!
{
	pub(crate) static STATS: std::cell::RefCell<SearchStats> = std::cell::RefCell::new(SearchStats::default());
}

/// Add one to a `SearchStats` field, if the `search-stats` feature is on.
macro_rules! count
{
	($field:ident) =>
	{
		#[cfg(feature = "search-stats")]
		crate::player::STATS.with_borrow_mut(|s| s.$field += 1);
	};
}
pub(crate) use count;

impl SearchStats
{
	/// The counts so far on this thread, resetting them to zero.
	pub fn take() -> Self
	{
		#[cfg(feature = "search-stats")]
		return STATS.with_borrow_mut(std::mem::take);
		#[cfg(not(feature = "search-stats"))]
		Self::default()
	}
}

pub trait MinimaxPlayer
{
	const WIN: f32 = f32::INFINITY;

	/// Scores outside `alpha..beta` are only bounds: at most `alpha`, or at least `beta`.
	/// Returns None if `deadline` passed before the search finished.
	fn minimax(&mut self, board: GameBoard, turn: Turn, depth: usize,
			   mut alpha: ScoreType, mut beta: ScoreType, deadline: Option<Instant>) -> Option<ScoreType> {
		let known = self.cache().and_then(|c| { count!(cache_probes); c.lookup(&board, turn, depth) });
		let mut hint = None;
		if let Some(k) = known
		{
			count!(cache_hits);
			if k.lower >= beta
			{
				count!(cache_returns);
				return Some(k.lower);
			}
			if k.upper <= alpha || k.lower == k.upper
			{
				count!(cache_returns);
				return Some(k.upper);
			}
			alpha = alpha.max(k.lower);
			beta = beta.min(k.upper);
			hint = k.best;
		}
		// Otherwise try the best move from the previous, one-ply-shallower iteration first
		if hint.is_none() && depth > 0
		{
			hint = self.cache().and_then(|c| { count!(cache_probes); c.lookup(&board, turn, depth - 1) }).and_then(|e| e.best);
		}
		let (score, best) = self.minimax_uncached(board, turn, depth, alpha, beta, hint, deadline)?;
		if let Some(c) = self.cache()
		{
			// On a fail-low every move was only bounded, so none is known to be best
			let mut entry = if score <= alpha
			{
				CacheEntry { lower: f32::NEG_INFINITY, upper: score, best: None }
			}
			else if score >= beta
			{
				CacheEntry { lower: score, upper: f32::INFINITY, best }
			}
			else
			{
				CacheEntry { lower: score, upper: score, best }
			};
			if let Some(k) = known
			{
				entry.lower = entry.lower.max(k.lower);
				entry.upper = entry.upper.min(k.upper);
				entry.best = entry.best.or(k.best);
			}
			c.insert(&board, turn, depth, entry);
		}
		Some(score)
	}

	/// The score and the move that gave it (None at a leaf), trying `hint` first if it is legal.
	/// Returns None if `deadline` passed before the search finished.
	fn minimax_uncached(&mut self, board: GameBoard, turn: Turn, depth: usize,
						mut alpha: ScoreType, mut beta: ScoreType,
						hint: Option<Move>, deadline: Option<Instant>) -> Option<(ScoreType, Option<Move>)> {
		count!(nodes);
		if board.check_win(turn)
		{
			count!(terminal);
			let score = match turn {
				Turn::Tiger => -Self::WIN,
				Turn::Sheep => Self::WIN,
			};
			return Some((score, None));
		}
		if depth == 0
		{
			count!(leaves);
			return Some((self.evaluate(&board), None));
		}
		if deadline.is_some_and(|d| Instant::now() >= d)
		{
			return None;
		}

		// Try the cache's best move first, then jumps (which `get_moves` lists first), then killer
		// moves, then the rest in generated order. Rotating keeps that cheaper than sorting.
		let mut moves = board.get_moves(turn);
		if let Some(killers) = self.killers().map(|k| k.get(depth))
		{
			let quiet = moves.iter().take_while(|&&m| is_jump(m)).count();
			// The older killer goes first, so the newer one ends up in front of it
			for k in killers.into_iter().rev().flatten()
			{
				if let Some(i) = moves[quiet..].iter().position(|&m| m == k)
				{
					moves[quiet..=quiet + i].rotate_right(1);
				}
			}
		}
		if let Some(i) = hint.and_then(|h| moves.iter().position(|&m| m == h))
		{
			moves[..=i].rotate_right(1);
		}
		let next = turn.next_turn();
		// Keep the first move on ties, so a line of all-losing (-inf) moves still picks one
		let mut best: Option<(ScoreType, Move)> = None;
		for (_i, m) in moves.into_iter().enumerate()
		{
			let score = self.minimax(board.after_move_unchecked(m), next, depth - 1, alpha, beta, deadline)?;
			let better = match best
			{
				None => true,
				Some((best_score, _)) => match turn
				{
					Turn::Tiger => score > best_score,  // Maximising player
					Turn::Sheep => score < best_score,  // Minimising player
				},
			};
			if better
			{
				best = Some((score, m));
				match turn
				{
					Turn::Tiger => alpha = alpha.max(score),
					Turn::Sheep => beta = beta.min(score),
				}
				if alpha >= beta
				{
					count!(cutoffs);
					#[cfg(feature = "search-stats")]
					if _i == 0
					{
						count!(first_move_cutoffs);
					}
					if !is_jump(m)
					{
						if let Some(k) = self.killers()
						{
							k.add(depth, m);
						}
					}
					break;
				}
			}
		}
		let (score, m) = best.expect("check_win is false, so there is a legal move");
		Some((score, Some(m)))
	}

	/// Override to return a cache to enable caching; the default is no cache.
	fn cache(&mut self) -> Option<&mut dyn ScoreCache>
	{
		None
	}

	/// Override to return storage for killer moves, to try them earlier; the default is none.
	fn killers(&mut self) -> Option<&mut Killers>
	{
		None
	}
	
	/// More positive for tiger winning, less positive for sheep winning
	fn evaluate(&mut self, board: &GameBoard) -> ScoreType;

	/// The most plies `pick_move` searches, counting its own move. Normally the time limit stops it first.
	fn depth(&self) -> usize { MAX_DEPTH }

	/// How long `pick_move` may spend searching.
	fn time_limit(&self) -> Duration { Duration::from_secs(1) }

	fn name(&self) -> String { "Computer".to_string() }

	/// Override with `last_search` to keep each search's report for the last-turn panel.
	/// By default it is dropped.
	fn set_last_search(&mut self, _report: Option<SearchReport>) {}

	fn last_search(&self) -> Option<SearchReport> { None }
}

impl<P: MinimaxPlayer> Player for P
{
	fn name(&self) -> String { MinimaxPlayer::name(self) }

	/// Iterative deepening: searches depth 1, 2, … until time runs out, each depth trying the
	/// previous one's best moves first, and plays the best move from the deepest finished search.
	fn pick_move(&mut self, board: GameBoard, turn: Turn) -> Option<Move> {
		let start = Instant::now();
		let time_limit = self.time_limit();
		let deadline = start + time_limit;
		// Entries would stay valid across moves (the key includes depth), but clear to bound memory
		if let Some(c) = self.cache()
		{
			c.clear();
		}
		if let Some(k) = self.killers()
		{
			k.clear();
		}
		let mut best = None;
		let mut report = None;
		for depth in 1..=self.depth().max(1)
		{
			// Depth 1 ignores the deadline, so there is always a move to play
			let dl = (depth > 1).then_some(deadline);
			// Out of time: drop the partial search and keep the last finished one's move
			let Some((score, m)) = self.minimax_uncached(board, turn, depth, f32::NEG_INFINITY, f32::INFINITY, best, dl)
			else
			{
				break;
			};
			best = m;
			report = m.map(|mv| SearchReport { mv, depth, score });
			// A forced win or loss won't change with more depth,
			// and past half the budget the next depth probably won't finish
			if score.abs() >= Self::WIN || start.elapsed() > time_limit / 2
			{
				break;
			}
		}
		self.set_last_search(report);
		Some(best.expect("pick_move called with no legal moves"))
    }

	fn reset(&mut self)
	{
		self.set_last_search(None);
	}

	fn render(&self, area: Rect, buf: &mut Buffer)
	{
		Paragraph::new(format!("{}\n\nThinking…", MinimaxPlayer::name(self))).render(area, buf);
	}

	fn render_last_turn(&self, area: Rect, buf: &mut Buffer)
	{
		let Some(r) = self.last_search() else { return };
		let score = if r.score >= Self::WIN
		{
			"forced win for tigers".to_string()
		}
		else if r.score <= -Self::WIN
		{
			"forced win for sheep".to_string()
		}
		else
		{
			format!("{:+.1} (+ favours tigers)", r.score)
		};
		Paragraph::new(format!("Played {}\nSearched to depth {}\nScore: {score}", r.mv, r.depth))
			.wrap(Wrap { trim: true }).render(area, buf);
	}
    
}

#[cfg(test)]
mod tests
{
	use super::*;
	use crate::board::PieceType;

	/// A minimax player with a cheap evaluation that still varies a lot between positions.
	struct TestPlayer
	{
		depth: usize,
		time_limit: Duration,
		cache: Option<Box<dyn ScoreCache>>,
		killers: Option<Killers>,
	}

	/// Which cache a `TestPlayer` searches with.
	#[derive(Clone, Copy, Debug)]
	enum CacheKind
	{
		None,
		Map,
		/// A table so small that slots are overwritten all the time.
		TinyTable,
	}

	const CACHED: [CacheKind; 2] = [CacheKind::Map, CacheKind::TinyTable];
	const ALL_CACHES: [CacheKind; 3] = [CacheKind::None, CacheKind::Map, CacheKind::TinyTable];

	impl TestPlayer
	{
		fn new(depth: usize, cache: CacheKind) -> Self
		{
			let cache: Option<Box<dyn ScoreCache>> = match cache
			{
				CacheKind::None => None,
				CacheKind::Map => Some(Box::new(HashMap::<(GameBoard, Turn, usize), CacheEntry>::new())),
				CacheKind::TinyTable => Some(Box::new(TranspositionTable::new(3))),
			};
			// Long enough that only the depth cap stops a search, so results are deterministic
			Self { depth, time_limit: Duration::from_secs(3600), cache, killers: None }
		}

		/// Also order moves with killer moves if `on`.
		fn with_killers(mut self, on: bool) -> Self
		{
			self.killers = on.then(Killers::default);
			self
		}
	}

	impl MinimaxPlayer for TestPlayer
	{
		fn evaluate(&mut self, board: &GameBoard) -> ScoreType
		{
			let dead = 20 - board.sheep_count() - board.sheep_left();
			100.0 * dead as f32 + board.get_moves(Turn::Tiger).len() as f32
				- board.get_moves(Turn::Sheep).len() as f32 / 2.0
		}

		fn cache(&mut self) -> Option<&mut dyn ScoreCache>
		{
			match &mut self.cache
			{
				Some(c) => Some(c.as_mut()),
				None => None,
			}
		}

		fn killers(&mut self) -> Option<&mut Killers>
		{
			self.killers.as_mut()
		}

		fn depth(&self) -> usize { self.depth }

		fn time_limit(&self) -> Duration { self.time_limit }
	}

	/// Plain minimax with no pruning or cache, to check the real search against.
	fn reference(player: &mut TestPlayer, board: GameBoard, turn: Turn, depth: usize) -> ScoreType
	{
		if board.check_win(turn)
		{
			return match turn { Turn::Tiger => -TestPlayer::WIN, Turn::Sheep => TestPlayer::WIN };
		}
		if depth == 0
		{
			return player.evaluate(&board);
		}
		let scores = board.get_moves(turn).into_iter()
			.map(|m| reference(player, board.after_move_unchecked(m), turn.next_turn(), depth - 1));
		match turn
		{
			Turn::Tiger => scores.fold(f32::NEG_INFINITY, f32::max),
			Turn::Sheep => scores.fold(f32::INFINITY, f32::min),
		}
	}

	/// Positions reached by playing `plies` pseudo-random moves from the start, or None if the game ended.
	fn position(plies: usize, seed: u64) -> Option<(GameBoard, Turn)>
	{
		let mut board = GameBoard::default();
		let mut turn = Turn::Sheep;
		let mut rng = seed;
		for _ in 0..plies
		{
			if board.check_win(turn)
			{
				return None;
			}
			let moves = board.get_moves(turn);
			rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
			board.make_move_unchecked(moves[(rng >> 33) as usize % moves.len()]);
			turn = turn.next_turn();
		}
		(!board.check_win(turn)).then_some((board, turn))
	}

	fn test_positions() -> Vec<(GameBoard, Turn)>
	{
		[0, 1, 5, 12, 25, 41, 50, 60].iter()
			.flat_map(|&plies| (1..4).filter_map(move |seed| position(plies, seed)))
			.collect()
	}

	#[test]
	fn alpha_beta_matches_plain_minimax()
	{
		for (board, turn) in test_positions()
		{
			for depth in 1..=3
			{
				let mut player = TestPlayer::new(depth, CacheKind::None);
				let expected = reference(&mut player, board, turn, depth);
				let score = player.minimax(board, turn, depth, f32::NEG_INFINITY, f32::INFINITY, None).unwrap();
				assert_eq!(score, expected, "depth {depth}, {turn:?} to move");
			}
		}
	}

	#[test]
	fn cached_alpha_beta_matches_plain_minimax()
	{
		for (board, turn) in test_positions()
		{
			for depth in 1..=3
			{
				for kind in CACHED
				{
					for killers in [false, true]
					{
						let mut player = TestPlayer::new(depth, kind).with_killers(killers);
						let expected = reference(&mut player, board, turn, depth);
						let score = player.minimax(board, turn, depth, f32::NEG_INFINITY, f32::INFINITY, None).unwrap();
						assert_eq!(score, expected, "depth {depth}, {turn:?} to move, {kind:?}, killers {killers}");
					}
				}
			}
		}
	}

	#[test]
	fn any_hint_gives_the_same_score()
	{
		for (board, turn) in test_positions()
		{
			let mut player = TestPlayer::new(3, CacheKind::None);
			let expected = reference(&mut player, board, turn, 3);
			for hint in board.get_moves(turn)
			{
				for kind in CACHED
				{
					let mut player = TestPlayer::new(3, kind).with_killers(true);
					let (score, _) = player.minimax_uncached(board, turn, 3, f32::NEG_INFINITY, f32::INFINITY, Some(hint), None).unwrap();
					assert_eq!(score, expected, "{turn:?} to move, hint {hint:?}, {kind:?}");
				}
			}
		}
	}

	#[test]
	fn aborted_search_is_not_cached()
	{
		for kind in CACHED
		{
			let mut player = TestPlayer::new(3, kind);
			let past = Instant::now();
			assert_eq!(player.minimax(GameBoard::default(), Turn::Sheep, 3, f32::NEG_INFINITY, f32::INFINITY, Some(past)), None);
			// The root gives up before searching any move, so its entry is the only one there could be
			assert_eq!(player.cache.unwrap().lookup(&GameBoard::default(), Turn::Sheep, 3), None, "{kind:?}");
		}
	}

	#[test]
	fn pick_move_respects_time_limit()
	{
		let mut player = TestPlayer::new(64, CacheKind::TinyTable);
		player.time_limit = Duration::from_millis(50);
		let board = GameBoard::default();
		let start = Instant::now();
		let m = player.pick_move(board, Turn::Sheep).unwrap();
		assert!(start.elapsed() < Duration::from_millis(500), "took {:?}", start.elapsed());
		assert_eq!(board.check_move(m), Ok(()));
	}

	#[test]
	fn pick_move_picks_a_best_move()
	{
		for (board, turn) in test_positions()
		{
			for kind in ALL_CACHES
			{
				for killers in [false, true]
				{
					let mut player = TestPlayer::new(4, kind).with_killers(killers);
					let best = reference(&mut player, board, turn, 4);
					let m = player.pick_move(board, turn).unwrap();
					let score = reference(&mut player, board.after_move_unchecked(m), turn.next_turn(), 3);
					assert_eq!(score, best, "{turn:?} picked {m:?} ({kind:?}, killers {killers})");
				}
			}
		}
	}

	#[test]
	fn tiger_takes_the_winning_kill()
	{
		// 16 sheep placed, 4 of them then removed as if killed, so one more kill wins.
		// (1, 0) is left next to the tiger at (0, 0) with (2, 0) empty behind it.
		let mut board = GameBoard::default();
		let mut placed = Vec::new();
		for y in 0..5
		{
			for x in 0..5
			{
				if placed.len() < 16 && (x, y) != (2, 0) && matches!(board.get((x, y)), PieceType::Empty)
				{
					board.make_move(Move::PlaceSheep { pos: (x, y) }).unwrap();
					placed.push((x, y));
				}
			}
		}
		for &pos in &placed[12..]
		{
			board.set(pos, PieceType::Empty);
		}
		assert_eq!(board.sheep_count() + board.sheep_left(), 16);

		for kind in ALL_CACHES
		{
			let m = TestPlayer::new(3, kind).pick_move(board, Turn::Tiger).unwrap();
			assert!(board.after_move_unchecked(m).check_win(Turn::Sheep), "picked {m:?} ({kind:?})");
		}
	}

	#[test]
	fn table_keeps_entries_apart_and_forgets_them_on_clear()
	{
		let entry = CacheEntry { lower: 1.0, upper: 2.0, best: None };
		let board = GameBoard::default();
		let mut table = TranspositionTable::new(3);
		table.insert(&board, Turn::Sheep, 3, entry);
		assert_eq!(table.lookup(&board, Turn::Sheep, 3), Some(entry));
		assert_eq!(table.lookup(&board, Turn::Tiger, 3), None);
		assert_eq!(table.lookup(&board, Turn::Sheep, 2), None);
		table.clear();
		assert_eq!(table.lookup(&board, Turn::Sheep, 3), None);
		// Enough clears to wrap the generation round to the one the entry was stored in
		table.insert(&board, Turn::Sheep, 3, entry);
		for _ in 0..256
		{
			table.clear();
		}
		assert_eq!(table.lookup(&board, Turn::Sheep, 3), None);
	}

	#[test]
	fn killers_keep_the_last_two_different_moves()
	{
		let a = Move::PlaceSheep { pos: (0, 1) };
		let b = Move::PlaceSheep { pos: (0, 2) };
		let c = Move::PlaceSheep { pos: (0, 3) };
		let mut killers = Killers::default();
		killers.add(2, a);
		killers.add(2, a);
		assert_eq!(killers.get(2), [Some(a), None]);
		killers.add(2, b);
		killers.add(2, c);
		assert_eq!(killers.get(2), [Some(c), Some(b)]);
		assert_eq!(killers.get(1), [None, None]);
		// Depths past the table are ignored rather than panicking
		killers.add(MAX_DEPTH + 1, a);
		assert_eq!(killers.get(MAX_DEPTH + 1), [None, None]);
		killers.clear();
		assert_eq!(killers.get(2), [None, None]);
	}
}
