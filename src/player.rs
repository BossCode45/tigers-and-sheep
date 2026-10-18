use std::{collections::HashMap, time::{Duration, Instant}};

use crate::board::{GameBoard, Move, Turn};

pub trait Player
{
	fn pick_move(&mut self, board: GameBoard, turn: Turn) -> Move;
}

pub type ScoreType = f32;

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

pub trait MinimaxPlayer
{
	const WIN: f32 = f32::INFINITY;

	/// Scores outside `alpha..beta` are only bounds: at most `alpha`, or at least `beta`.
	/// Returns None if `deadline` passed before the search finished.
	fn minimax(&mut self, board: GameBoard, turn: Turn, depth: usize,
			   mut alpha: ScoreType, mut beta: ScoreType, deadline: Option<Instant>) -> Option<ScoreType> {
		let known = self.cache().and_then(|c| c.lookup(&board, turn, depth));
		let mut hint = None;
		if let Some(k) = known
		{
			if k.lower >= beta
			{
				return Some(k.lower);
			}
			if k.upper <= alpha || k.lower == k.upper
			{
				return Some(k.upper);
			}
			alpha = alpha.max(k.lower);
			beta = beta.min(k.upper);
			hint = k.best;
		}
		// Otherwise try the best move from the previous, one-ply-shallower iteration first
		if hint.is_none() && depth > 0
		{
			hint = self.cache().and_then(|c| c.lookup(&board, turn, depth - 1)).and_then(|e| e.best);
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
		if board.check_win(turn)
		{
			let score = match turn {
				Turn::Tiger => -Self::WIN,
				Turn::Sheep => Self::WIN,
			};
			return Some((score, None));
		}
		if depth == 0
		{
			return Some((self.evaluate(&board), None));
		}
		if deadline.is_some_and(|d| Instant::now() >= d)
		{
			return None;
		}

		let mut moves = board.get_moves(turn);
		if let Some(i) = hint.and_then(|h| moves.iter().position(|&m| m == h))
		{
			moves[..=i].rotate_right(1);
		}
		let next = turn.next_turn();
		// Keep the first move on ties, so a line of all-losing (-inf) moves still picks one
		let mut best: Option<(ScoreType, Move)> = None;
		for m in moves
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
	
	/// More positive for tiger winning, less positive for sheep winning
	fn evaluate(&mut self, board: &GameBoard) -> ScoreType;

	/// The most plies `pick_move` searches, counting its own move. Normally the time limit stops it first.
	fn depth(&self) -> usize { 64 }

	/// How long `pick_move` may spend searching.
	fn time_limit(&self) -> Duration { Duration::from_secs(1) }
}

impl<P: MinimaxPlayer> Player for P
{
	
	/// Iterative deepening: searches depth 1, 2, … until time runs out, each depth trying the
	/// previous one's best moves first, and plays the best move from the deepest finished search.
	fn pick_move(&mut self, board: GameBoard, turn: Turn) -> Move {
		let start = Instant::now();
		let time_limit = self.time_limit();
		let deadline = start + time_limit;
		// Entries would stay valid across moves (the key includes depth), but clear to bound memory
		if let Some(c) = self.cache()
		{
			c.clear();
		}
		let mut best = None;
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
			// A forced win or loss won't change with more depth,
			// and past half the budget the next depth probably won't finish
			if score.abs() >= Self::WIN || start.elapsed() > time_limit / 2
			{
				break;
			}
		}
		best.expect("pick_move called with no legal moves")
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
		cache: Option<HashMap<(GameBoard, Turn, usize), CacheEntry>>,
	}

	impl TestPlayer
	{
		fn new(depth: usize, cached: bool) -> Self
		{
			// Long enough that only the depth cap stops a search, so results are deterministic
			Self { depth, time_limit: Duration::from_secs(3600), cache: cached.then(HashMap::new) }
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
			self.cache.as_mut().map(|c| c as &mut dyn ScoreCache)
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
				let mut player = TestPlayer::new(depth, false);
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
				let mut player = TestPlayer::new(depth, true);
				let expected = reference(&mut player, board, turn, depth);
				let score = player.minimax(board, turn, depth, f32::NEG_INFINITY, f32::INFINITY, None).unwrap();
				assert_eq!(score, expected, "depth {depth}, {turn:?} to move");
			}
		}
	}

	#[test]
	fn any_hint_gives_the_same_score()
	{
		for (board, turn) in test_positions()
		{
			let mut player = TestPlayer::new(3, true);
			let expected = reference(&mut player, board, turn, 3);
			for hint in board.get_moves(turn)
			{
				let mut player = TestPlayer::new(3, true);
				let (score, _) = player.minimax_uncached(board, turn, 3, f32::NEG_INFINITY, f32::INFINITY, Some(hint), None).unwrap();
				assert_eq!(score, expected, "{turn:?} to move, hint {hint:?}");
			}
		}
	}

	#[test]
	fn aborted_search_is_not_cached()
	{
		let mut player = TestPlayer::new(3, true);
		let past = Instant::now();
		assert_eq!(player.minimax(GameBoard::default(), Turn::Sheep, 3, f32::NEG_INFINITY, f32::INFINITY, Some(past)), None);
		assert!(player.cache.unwrap().is_empty());
	}

	#[test]
	fn pick_move_respects_time_limit()
	{
		let mut player = TestPlayer::new(64, true);
		player.time_limit = Duration::from_millis(50);
		let board = GameBoard::default();
		let start = Instant::now();
		let m = player.pick_move(board, Turn::Sheep);
		assert!(start.elapsed() < Duration::from_millis(500), "took {:?}", start.elapsed());
		assert_eq!(board.check_move(m), Ok(()));
	}

	#[test]
	fn pick_move_picks_a_best_move()
	{
		for (board, turn) in test_positions()
		{
			for cached in [false, true]
			{
				let mut player = TestPlayer::new(4, cached);
				let best = reference(&mut player, board, turn, 4);
				let m = player.pick_move(board, turn);
				let score = reference(&mut player, board.after_move_unchecked(m), turn.next_turn(), 3);
				assert_eq!(score, best, "{turn:?} picked {m:?} (cached: {cached})");
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

		for cached in [false, true]
		{
			let m = TestPlayer::new(3, cached).pick_move(board, Turn::Tiger);
			assert!(board.after_move_unchecked(m).check_win(Turn::Sheep), "picked {m:?} (cached: {cached})");
		}
	}
}
