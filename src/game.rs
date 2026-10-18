use crate::{board::{GameBoard, Turn}, player::Player};

pub struct GameHandler
{
	board: GameBoard,
}

impl Default for GameHandler
{
	fn default() -> Self
	{
		Self
		{
			board: GameBoard::default()
		}
	}
}

impl GameHandler
{
	// fn play_move<T: Player>(&mut self, player: &mut T)
	// {
	// 	let m = player.pick_move(self.board);
	// 	self.board.make_move(m).unwrap();
	// }

	/// Play until someone wins, with `p1` as the sheep and `p2` as the tigers. Returns the winner.
	pub fn play_game<T1: Player, T2: Player>(&mut self, p1: &mut T1, p2: &mut T2) -> Turn
	{
		let mut turn = Turn::Sheep;
		while !self.board.check_win(turn)
		{
			let m = match turn
			{
				Turn::Sheep => p1.pick_move(self.board, turn),
				Turn::Tiger => p2.pick_move(self.board, turn),
			};
			self.board.make_move(m).unwrap();
			turn = turn.next_turn();
		}
		// check_win runs before `turn`'s move, so whoever moved last has won.
		turn.next_turn()
	}
}
