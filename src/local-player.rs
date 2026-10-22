use ratatui::{buffer::Buffer, layout::Rect, style::{Color, Style}, text::{Line, Text}, widgets::{Paragraph, Widget, Wrap}};

use crate::{board::{GameBoard, Move, PieceType, Pos, Turn}, player::{Highlights, Input, Player}};

/// Sheep that start the game: on the board plus in hand.
const TOTAL_SHEEP: u32 = 20;

/// A human player at the terminal, using the mouse: click a piece to select it, then click one
/// of its highlighted destinations. While sheep are in hand, click an empty point to place one.
#[derive(Default)]
pub struct LocalPlayer
{
	/// The piece whose moves are highlighted.
	selected: Option<Pos>,
	/// The legal moves of `selected`, kept for `highlights`.
	selected_moves: Vec<Move>,
	/// Why the last click didn't do anything, if it didn't.
	message: String,
	/// The position to play, for the player panel; set by `update`.
	status: Option<(GameBoard, Turn)>,
}

/// The source and destination of a move. Placements have no source.
fn ends(mv: Move) -> (Option<Pos>, Pos)
{
	match mv
	{
		Move::PlaceSheep { pos } => (None, pos),
		Move::MoveSheep { from, to } | Move::MoveTiger { from, to } => (Some(from), to),
	}
}

fn is_mine(board: &GameBoard, pos: Pos, turn: Turn) -> bool
{
	match turn
	{
		Turn::Tiger => matches!(board.get(pos), PieceType::Tiger),
		Turn::Sheep => matches!(board.get(pos), PieceType::Sheep),
	}
}

impl LocalPlayer
{
	fn click(&mut self, pos: Pos, board: GameBoard, turn: Turn) -> Option<Move>
	{
		self.message.clear();
		let moves = board.get_moves(turn);

		if turn == Turn::Sheep && board.sheep_left() > 0
		{
			let mv = Move::PlaceSheep { pos };
			if moves.contains(&mv)
			{
				return Some(mv);
			}
			if let Err(err) = board.check_move(mv)
			{
				self.message = err.to_string();
			}
			return None;
		}

		if let Some(mv) = self.selected_moves.iter().copied().find(|&m| ends(m).1 == pos)
		{
			return Some(mv);
		}
		self.reset();
		if is_mine(&board, pos, turn)
		{
			let from_here: Vec<Move> = moves.into_iter().filter(|&m| ends(m).0 == Some(pos)).collect();
			if from_here.is_empty()
			{
				self.message = "That piece can't move".to_string();
			}
			else
			{
				self.selected = Some(pos);
				self.selected_moves = from_here;
			}
		}
		None
	}
}

impl Player for LocalPlayer
{
	fn name(&self) -> String { "You".to_string() }

	fn update(&mut self, board: GameBoard, turn: Turn)
	{
		self.status = Some((board, turn));
	}

	fn handle_input(&mut self, input: Input, board: GameBoard, turn: Turn) -> Option<Move>
	{
		match input
		{
			Input::Click(pos) => self.click(pos, board, turn),
			Input::Key(_) => None,
		}
	}

	fn highlights(&self) -> Highlights
	{
		Highlights
		{
			selected: self.selected,
			targets: self.selected_moves.iter().map(|&m| ends(m).1).collect(),
		}
	}

	fn reset(&mut self)
	{
		self.selected = None;
		self.selected_moves.clear();
	}

	fn render(&self, area: Rect, buf: &mut Buffer)
	{
		let mut text = Text::from(self.name());
		if let Some((board, turn)) = self.status
		{
			let dead = TOTAL_SHEEP - board.sheep_count() - board.sheep_left();
			text.push_line(format!("Sheep in hand: {}, on board: {}, killed: {dead}",
				board.sheep_left(), board.sheep_count()));
			let instruction = if turn == Turn::Sheep && board.sheep_left() > 0
			{
				"Click an empty point to place a sheep".to_string()
			}
			else if self.selected.is_some()
			{
				"Click a highlighted point to move there, or another piece to select it".to_string()
			}
			else
			{
				format!("Click a {} to move", if turn == Turn::Tiger { "tiger" } else { "sheep" })
			};
			text.push_line(instruction);
		}
		if !self.message.is_empty()
		{
			text.push_line(Line::styled(self.message.clone(), Style::new().fg(Color::Red)));
		}
		Paragraph::new(text).wrap(Wrap { trim: false }).render(area, buf);
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	/// The default board with all 20 sheep placed, on every point but (2, 1).
	fn moving_board() -> GameBoard
	{
		let mut board = GameBoard::default();
		for y in 0..5
		{
			for x in 0..5
			{
				if (x, y) != (2, 1) && matches!(board.get((x, y)), PieceType::Empty)
				{
					board.make_move(Move::PlaceSheep { pos: (x, y) }).unwrap();
				}
			}
		}
		assert_eq!(board.sheep_left(), 0);
		board
	}

	#[test]
	fn click_places_a_sheep()
	{
		let mut player = LocalPlayer::default();
		let board = GameBoard::default();
		assert_eq!(player.handle_input(Input::Click((2, 2)), board, Turn::Sheep), Some(Move::PlaceSheep { pos: (2, 2) }));
	}

	#[test]
	fn click_on_occupied_point_explains_why()
	{
		let mut player = LocalPlayer::default();
		assert_eq!(player.handle_input(Input::Click((0, 0)), GameBoard::default(), Turn::Sheep), None);
		assert_eq!(player.message, "Can't place a sheep on an occupied point!");
	}

	#[test]
	fn select_highlight_then_move_tiger()
	{
		let mut player = LocalPlayer::default();
		let board = GameBoard::default();
		assert_eq!(player.handle_input(Input::Click((0, 0)), board, Turn::Tiger), None);
		let mut targets = player.highlights().targets;
		targets.sort();
		assert_eq!(player.highlights().selected, Some((0, 0)));
		assert_eq!(targets, vec![(0, 1), (1, 0), (1, 1)]);
		assert_eq!(player.handle_input(Input::Click((1, 1)), board, Turn::Tiger),
			Some(Move::MoveTiger { from: (0, 0), to: (1, 1) }));
	}

	#[test]
	fn clicking_another_piece_reselects()
	{
		let mut player = LocalPlayer::default();
		let board = GameBoard::default();
		player.handle_input(Input::Click((0, 0)), board, Turn::Tiger);
		player.handle_input(Input::Click((4, 4)), board, Turn::Tiger);
		assert_eq!(player.highlights().selected, Some((4, 4)));
		// (1, 1) isn't reachable from (4, 4), so it just deselects
		assert_eq!(player.handle_input(Input::Click((1, 1)), board, Turn::Tiger), None);
		assert_eq!(player.highlights(), Highlights::default());
	}

	#[test]
	fn select_and_move_sheep()
	{
		let mut player = LocalPlayer::default();
		let board = moving_board();
		assert_eq!(player.handle_input(Input::Click((2, 2)), board, Turn::Sheep), None);
		assert_eq!(player.highlights().targets, vec![(2, 1)]);
		assert_eq!(player.handle_input(Input::Click((2, 1)), board, Turn::Sheep),
			Some(Move::MoveSheep { from: (2, 2), to: (2, 1) }));
	}

	#[test]
	fn stuck_piece_is_not_selected()
	{
		let mut player = LocalPlayer::default();
		let board = moving_board();
		// (0, 4) is a tiger boxed in by sheep with nothing empty behind them
		assert_eq!(player.handle_input(Input::Click((0, 4)), board, Turn::Tiger), None);
		assert_eq!(player.highlights().selected, None);
		assert_eq!(player.message, "That piece can't move");
	}
}
