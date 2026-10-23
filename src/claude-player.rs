use std::time::Duration;

use crate::{board::{is_jump, GameBoard, Move, PieceType, Turn}, player::{Killers, MinimaxPlayer, SearchReport, TranspositionTable}};

/// Sheep that start the game: on the board plus in hand.
const TOTAL_SHEEP: u32 = 20;

/// A computer player that searches with minimax for `time_limit` per move and scores positions with `evaluate`.
pub struct ClaudePlayer
{
	pub time_limit: Duration,
	/// The most plies to search, counting the move itself; normally `time_limit` stops the search first.
	pub max_depth: usize,
	cache: TranspositionTable,
	killers: Killers,
	last_search: Option<SearchReport>,
}

impl Default for ClaudePlayer
{
	fn default() -> Self
	{
		Self::new(Duration::from_secs(1))
	}
}

impl ClaudePlayer
{
	pub fn new(time_limit: Duration) -> Self
	{
		Self { time_limit, max_depth: 64, cache: TranspositionTable::new(20), killers: Killers::default(), last_search: None }
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

	fn set_last_search(&mut self, report: Option<SearchReport>) { self.last_search = report; }

	fn last_search(&self) -> Option<SearchReport> { self.last_search }

	fn time_limit(&self) -> Duration { self.time_limit }

	fn depth(&self) -> usize { self.max_depth }

	fn cache(&mut self) -> Option<&mut dyn crate::player::ScoreCache> {
		Some(&mut self.cache)
	}

	fn killers(&mut self) -> Option<&mut Killers> { Some(&mut self.killers) }
}

#[cfg(test)]
mod tests
{
	use super::*;
	use crate::player::Player;

	#[test]
	fn pick_move_reports_its_search()
	{
		let mut player = ClaudePlayer::new(Duration::from_millis(50));
		let m = player.pick_move(GameBoard::default(), Turn::Tiger).unwrap();
		let report = player.last_search.expect("a report");
		assert_eq!(report.mv, m);
		assert!(report.depth >= 1);
		assert_eq!(report.score, player.minimax(GameBoard::default(), Turn::Tiger, report.depth,
			f32::NEG_INFINITY, f32::INFINITY, None).unwrap());

		player.reset();
		assert_eq!(player.last_search, None);
	}
}
