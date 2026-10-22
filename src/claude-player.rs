use std::{collections::HashMap, time::Duration};

use crate::{board::{is_jump, GameBoard, Move, PieceType, Turn}, player::{CacheEntry, MinimaxPlayer}};

/// Sheep that start the game: on the board plus in hand.
const TOTAL_SHEEP: u32 = 20;

/// A computer player that searches with minimax for `time_limit` per move and scores positions with `evaluate`.
pub struct ClaudePlayer
{
	pub time_limit: Duration,
	cache: HashMap<(GameBoard, Turn, usize), CacheEntry>
}

impl Default for ClaudePlayer
{
	fn default() -> Self
	{
		Self { time_limit: Duration::from_secs(1), cache: HashMap::new() }
	}
}

impl ClaudePlayer
{
	pub fn new(time_limit: Duration) -> Self
	{
		Self { time_limit, cache: HashMap::new() }
	}
}

impl MinimaxPlayer for ClaudePlayer
{
	/// Scored from the tigers' side. In order of weight:
	/// - dead sheep, since five of them wins the game for the tigers;
	/// - jumps the tigers could make right now (sheep left hanging);
	/// - tigers boxed in with no move at all, since trapping every tiger wins for the sheep;
	/// - tiger mobility in general, since the sheep win by squeezing it to zero.
	fn evaluate(&mut self, board: &GameBoard) -> f32
	{
		let dead = TOTAL_SHEEP - board.sheep_count() - board.sheep_left();

		let tiger_moves = board.get_moves(Turn::Tiger);
		let jumps = tiger_moves.iter().filter(|m| is_jump(**m)).count();

		let mut trapped = 0;
		for y in 0..5
		{
			for x in 0..5
			{
				if matches!(board.get((x, y)), PieceType::Tiger)
					&& !tiger_moves.iter().any(|m| matches!(m, Move::MoveTiger { from, .. } if *from == (x, y)))
				{
					trapped += 1;
				}
			}
		}

		100.0 * dead as f32
			+ 20.0 * jumps as f32
			- 15.0 * trapped as f32
			+ 2.0 * tiger_moves.len() as f32
	}

	fn name(&self) -> String { "Claude".to_string() }

	fn time_limit(&self) -> Duration { self.time_limit }

	fn cache(&mut self) -> Option<&mut dyn crate::player::ScoreCache> {
		Some(&mut self.cache)
	}
}
