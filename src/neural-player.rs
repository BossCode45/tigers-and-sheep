use std::time::Duration;

use crate::{board::{GameBoard, Turn}, network::Network, player::{Killers, MinimaxPlayer, ScoreCache, SearchReport, TranspositionTable}};

/// A computer player that searches like `ClaudePlayer` but scores positions with a neural network
/// trained by self-play (see `train.rs`), so it has no human idea of what a good position is.
pub struct NeuralPlayer
{
	pub net: Network,
	pub time_limit: Duration,
	/// The most plies to search, counting the move itself; normally `time_limit` stops the search first.
	pub max_depth: usize,
	cache: TranspositionTable,
	killers: Killers,
	last_search: Option<SearchReport>,
}

impl NeuralPlayer
{
	/// A player with a `1 << table_bits`-slot transposition table.
	pub fn new(net: Network, time_limit: Duration, table_bits: u32) -> Self
	{
		Self { net, time_limit, max_depth: 64, cache: TranspositionTable::new(table_bits), killers: Killers::default(), last_search: None }
	}
}

impl MinimaxPlayer for NeuralPlayer
{
	/// The network's output, in -1..1; forced wins are still ±`WIN` from the search itself.
	fn evaluate(&mut self, board: &GameBoard, turn: Turn) -> f32
	{
		self.net.forward(board, turn)
	}

	fn name(&self) -> String { "Neural".to_string() }

	fn set_last_search(&mut self, report: Option<SearchReport>) { self.last_search = report; }

	fn last_search(&self) -> Option<SearchReport> { self.last_search }

	fn time_limit(&self) -> Duration { self.time_limit }

	fn depth(&self) -> usize { self.max_depth }

	fn cache(&mut self) -> Option<&mut dyn ScoreCache> { Some(&mut self.cache) }

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
		let mut player = NeuralPlayer::new(Network::random(2), Duration::from_secs(3600), 12);
		player.max_depth = 3;
		let board = GameBoard::default();
		let m = player.pick_move(board, Turn::Sheep, &[]).unwrap();
		assert_eq!(board.check_move(m), Ok(()));
		let report = player.last_search.expect("a report");
		assert_eq!((report.mv, report.depth), (m, 3));
		assert_eq!(report.score, player.minimax(board, Turn::Sheep, 3, f32::NEG_INFINITY, f32::INFINITY, None, &mut Vec::new()).unwrap());
	}
}
