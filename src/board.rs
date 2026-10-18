use std::{fmt::Display};

type Pos = (u8, u8);

pub enum PieceType
{
	Tiger,
	Sheep,
	Empty
}

impl Display for PieceType
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
			Self::Tiger => write!(f, "T"),
			Self::Sheep => write!(f, "S"),
			Self::Empty => write!(f, " "),
		}
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameBoard
{
	sheep_mask: u32,
	tiger_mask: u32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Turn
{
	Tiger,
	Sheep
}
impl Turn
{
	pub fn next_turn(&self) -> Turn
	{
		match self
		{
			Turn::Tiger => Self::Sheep,
			Turn::Sheep => Self::Tiger,
		}
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Move
{
	PlaceSheep{pos: Pos},
	MoveSheep{from: Pos, to: Pos},
	MoveTiger{from: Pos, to: Pos}
}

/// Why `check_move` rejected a move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveError
{
	NoSheepLeft,
	PlaceOnOccupied,
	SheepStillInHand,
	NoSheepToMove,
	SheepOntoOccupied,
	SheepNotOneStep,
	NoTigerToMove,
	TigerOntoOccupied,
	JumpOverNonSheep,
	TigerNotOnLine
}

impl Display for MoveError
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
			Self::NoSheepLeft => "No sheep left!",
			Self::PlaceOnOccupied => "Can't place a sheep on an occupied point!",
			Self::SheepStillInHand => "Can't move sheep until all sheep are placed!",
			Self::NoSheepToMove => "No sheep to move!",
			Self::SheepOntoOccupied => "Can't move a sheep onto an occupied point!",
			Self::SheepNotOneStep => "Sheep can only move one step along a line!",
			Self::NoTigerToMove => "No tiger to move!",
			Self::TigerOntoOccupied => "Can't move a tiger onto an occupied point!",
			Self::JumpOverNonSheep => "Tigers can only jump over a sheep!",
			Self::TigerNotOnLine => "Tigers can only step or jump along a line!",
		};
		write!(f, "{msg}")
    }
}

impl std::error::Error for MoveError {}

/// The 25 board bits (0 up to 24); the high 7 bits of `sheep_mask` are the sheep-in-hand count.
const BOARD_MASK: u32 = (1 << 25) - 1;
/// Bit offset of the sheep-in-hand count in `sheep_mask`.
const SHEEP_COUNT_SHIFT: u32 = 25;
/// One sheep in hand, as stored in `sheep_mask`.
const ONE_SHEEP: u32 = 1 << SHEEP_COUNT_SHIFT;

/// The 8 line directions as (dx, dy). Indices 0..4 are orthogonal, 4..8 diagonal.
const DIRS: [(isize, isize); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];

/// Change in bit index for one step in each direction.
const DELTAS: [isize; 8] = {
	let mut deltas = [0; 8];
	let mut d = 0;
	while d < 8
	{
		deltas[d] = DIRS[d].0 + DIRS[d].1 * 5;
		d += 1;
	}
	deltas
};

/// Mask of the points from which moving `dist` points in direction (dx, dy) stays on the board and on a line.
const fn line_sources((dx, dy): (isize, isize), dist: isize) -> u32
{
	let mut mask = 0;
	let mut y = 0;
	while y < 5
	{
		let mut x = 0;
		while x < 5
		{
			let tx = x + dx * dist;
			let ty = y + dy * dist;
			let on_board = tx >= 0 && tx < 5 && ty >= 0 && ty < 5;
			let on_line = dx == 0 || dy == 0 || (x + y) % 2 == 0;
			if on_board && on_line
			{
				mask |= 1 << (x + y * 5);
			}
			x += 1;
		}
		y += 1;
	}
	mask
}

const fn sources_table(dist: isize) -> [u32; 8]
{
	let mut table = [0; 8];
	let mut d = 0;
	while d < 8
	{
		table[d] = line_sources(DIRS[d], dist);
		d += 1;
	}
	table
}

/// For each direction, the points a piece can step from.
const STEP_SOURCES: [u32; 8] = sources_table(1);
/// For each direction, the points a tiger can jump from.
const JUMP_SOURCES: [u32; 8] = sources_table(2);

/// Move every point in `mask` by `delta` bit indices (positive = towards higher index = higher bit).
#[inline(always)]
fn shift(mask: u32, delta: isize) -> u32
{
	if delta >= 0 { mask << delta } else { mask >> -delta }
}

/// Iterate the bit indices (0..25) of the points set in `mask`.
#[inline(always)]
fn indices(mut mask: u32) -> impl Iterator<Item = usize>
{
	std::iter::from_fn(move || {
		if mask == 0
		{
			return None;
		}
		let idx = mask.trailing_zeros() as usize;
		mask &= mask - 1;
		Some(idx)
	})
}

#[inline(always)]
fn pos_of(idx: usize) -> Pos
{
	((idx % 5) as u8, (idx / 5) as u8)
}

impl GameBoard
{
	fn bit_idx((x, y): Pos) -> usize
	{
		if x > 4 || y > 4
		{
			panic!("Board is only 5x5!")
		}
		(x + y * 5) as usize
	}

	pub fn get(&self, pos: Pos) -> PieceType {
		let bit_idx = GameBoard::bit_idx(pos);
		let mask: u32 = 1 << bit_idx;
        if (self.tiger_mask & mask) != 0
		{
			PieceType::Tiger
		}
        else if (self.sheep_mask & mask) != 0
		{
			PieceType::Sheep
		}
		else
		{
			PieceType::Empty
		}
    }

	pub fn set(&mut self, pos: Pos, piece: PieceType) -> () {
		let bit_idx = GameBoard::bit_idx(pos);
		let mask: u32 = 1 << bit_idx;
		self.tiger_mask &= !mask;
		self.sheep_mask &= !mask;
		match piece
		{
			PieceType::Empty => (),
			PieceType::Tiger => self.tiger_mask |= mask,
			PieceType::Sheep => self.sheep_mask |= mask,
		}
    }
	
	pub fn sheep_left(&self) -> u32 { self.sheep_mask >> SHEEP_COUNT_SHIFT }
	pub fn sheep_count(&self) -> u32 { (self.sheep_mask & BOARD_MASK).count_ones() }

	/// All legal moves for `turn`. Sheep place while any are in hand, then move.
	pub fn get_moves(&self, turn: Turn) -> Vec<Move>
	{
		let tigers = self.tiger_mask;
		let sheep = self.sheep_mask & BOARD_MASK;
		let empty = !(tigers | sheep) & BOARD_MASK;

		match turn
		{
			Turn::Sheep if self.sheep_left() > 0 =>
			{
				let mut moves = Vec::with_capacity(empty.count_ones() as usize);
				moves.extend(indices(empty).map(|idx| Move::PlaceSheep { pos: pos_of(idx) }));
				moves
			}
			Turn::Sheep =>
			{
				let mut steps = [0; 8];
				for d in 0..8
				{
					steps[d] = shift(sheep & STEP_SOURCES[d], DELTAS[d]) & empty;
				}
				let mut moves = Vec::with_capacity(steps.iter().map(|m| m.count_ones() as usize).sum());
				for d in 0..8
				{
					for to in indices(steps[d])
					{
						let from = (to as isize - DELTAS[d]) as usize;
						moves.push(Move::MoveSheep { from: pos_of(from), to: pos_of(to) });
					}
				}
				moves
			}
			Turn::Tiger =>
			{
				let mut steps = [0; 8];
				let mut jumps = [0; 8];
				for d in 0..8
				{
					steps[d] = shift(tigers & STEP_SOURCES[d], DELTAS[d]) & empty;
					let over = shift(tigers & JUMP_SOURCES[d], DELTAS[d]) & sheep;
					jumps[d] = shift(over, DELTAS[d]) & empty;
				}
				let count = steps.iter().chain(jumps.iter()).map(|m| m.count_ones() as usize).sum();
				let mut moves = Vec::with_capacity(count);
				for d in 0..8
				{
					for to in indices(steps[d])
					{
						let from = (to as isize - DELTAS[d]) as usize;
						moves.push(Move::MoveTiger { from: pos_of(from), to: pos_of(to) });
					}
					for to in indices(jumps[d])
					{
						let from = (to as isize - 2 * DELTAS[d]) as usize;
						moves.push(Move::MoveTiger { from: pos_of(from), to: pos_of(to) });
					}
				}
				moves
			}
		}
	}

	/// Play `mv` if it is legal, otherwise leave the board unchanged and return why it is illegal.
	/// A tiger jump kills the sheep it jumps over.
	pub fn make_move(&mut self, mv: Move) -> Result<(), MoveError>
	{
		self.check_move(mv)?;
		self.make_move_unchecked(mv);
		Ok(())
	}

	/// Play `mv` without checking that it is legal, e.g. for moves straight from `get_moves`.
	/// An illegal move leaves the board in a state the rules can't reach.
	pub fn make_move_unchecked(&mut self, mv: Move) -> ()
	{
		match mv
		{
			Move::PlaceSheep { pos } => self.place_sheep(pos),
			Move::MoveSheep { from, to } => self.move_sheep(from, to),
			Move::MoveTiger { from, to } => self.move_tiger(from, to),
		}
	}

	/// `Ok` if `mv` is legal, otherwise the reason it is illegal.
	pub fn check_move(&self, mv: Move) -> Result<(), MoveError>
	{
		match mv
		{
			Move::PlaceSheep { pos } =>
			{
				if self.sheep_left() == 0
				{
					return Err(MoveError::NoSheepLeft);
				}
				if !matches!(self.get(pos), PieceType::Empty)
				{
					return Err(MoveError::PlaceOnOccupied);
				}
			}
			Move::MoveSheep { from, to } =>
			{
				if self.sheep_left() != 0
				{
					return Err(MoveError::SheepStillInHand);
				}
				if !matches!(self.get(from), PieceType::Sheep)
				{
					return Err(MoveError::NoSheepToMove);
				}
				if !matches!(self.get(to), PieceType::Empty)
				{
					return Err(MoveError::SheepOntoOccupied);
				}
				if !GameBoard::on_line(from, to, 1)
				{
					return Err(MoveError::SheepNotOneStep);
				}
			}
			Move::MoveTiger { from, to } =>
			{
				if !matches!(self.get(from), PieceType::Tiger)
				{
					return Err(MoveError::NoTigerToMove);
				}
				if !matches!(self.get(to), PieceType::Empty)
				{
					return Err(MoveError::TigerOntoOccupied);
				}
				if GameBoard::on_line(from, to, 2)
				{
					if !matches!(self.get(GameBoard::midpoint(from, to)), PieceType::Sheep)
					{
						return Err(MoveError::JumpOverNonSheep);
					}
				}
				else if !GameBoard::on_line(from, to, 1)
				{
					return Err(MoveError::TigerNotOnLine);
				}
			}
		}
		Ok(())
	}

	/// True if the board is in a winning state before the next turn.
	/// The side that just moved has won.
	pub fn check_win(&self, turn: Turn) -> bool
	{
		if self.sheep_count() + self.sheep_left() <= 15
		{
			true
		}
		else
		{
			self.get_moves(turn).len() == 0
		}
	}

	pub fn after_move_unchecked(&self, mv: Move) -> Self
	{
		let mut new = self.clone();
		new.make_move_unchecked(mv);
		new
	}
		

	/// True if `to` is `dist` points away from `from` along a board line.
	/// Orthogonal lines are everywhere; diagonals only run through points where x + y is even.
	fn on_line((fx, fy): Pos, (tx, ty): Pos, dist: u8) -> bool
	{
		let dx = fx.abs_diff(tx);
		let dy = fy.abs_diff(ty);
		let orthogonal = (dx == dist && dy == 0) || (dx == 0 && dy == dist);
		let diagonal = dx == dist && dy == dist && (fx + fy) % 2 == 0;
		orthogonal || diagonal
	}

	fn midpoint(from: Pos, to: Pos) -> Pos
	{
		((from.0 + to.0) / 2, (from.1 + to.1) / 2)
	}

	// The functions below apply a move without checking it; `check_move` does the checks.

	fn place_sheep(&mut self, pos: Pos) -> () {
		self.set(pos, PieceType::Sheep);
		self.sheep_mask -= ONE_SHEEP;
	}

	fn move_sheep(&mut self, from: Pos, to: Pos) -> () {
		self.set(from, PieceType::Empty);
		self.set(to, PieceType::Sheep);
	}

	fn kill_sheep(&mut self, pos: Pos) -> () {
		self.set(pos, PieceType::Empty);
	}

	/// A move of two points along a line is a jump, which kills the sheep in between.
	fn move_tiger(&mut self, from: Pos, to: Pos) -> () {
		if from.0.abs_diff(to.0) == 2 || from.1.abs_diff(to.1) == 2
		{
			self.kill_sheep(GameBoard::midpoint(from, to));
		}
		self.set(from, PieceType::Empty);
		self.set(to, PieceType::Tiger);
	}
}

impl Default for GameBoard
{
    fn default() -> Self {
        Self { tiger_mask: 0b1_0001_0000_0000_0000_0001_0001, sheep_mask: 20 * ONE_SHEEP }
    }
}

#[cfg(test)]
mod tests
{
	use super::*;

	/// A board with no tigers and the full 20 sheep in hand.
	fn empty_board() -> GameBoard
	{
		GameBoard { tiger_mask: 0, sheep_mask: 20 * ONE_SHEEP }
	}

	fn is_tiger(board: &GameBoard, pos: Pos) -> bool { matches!(board.get(pos), PieceType::Tiger) }
	fn is_sheep(board: &GameBoard, pos: Pos) -> bool { matches!(board.get(pos), PieceType::Sheep) }
	fn is_empty(board: &GameBoard, pos: Pos) -> bool { matches!(board.get(pos), PieceType::Empty) }

	// --- bit_idx / get / set ---

	#[test]
	fn bit_idx_maps_rows_and_columns()
	{
		assert_eq!(GameBoard::bit_idx((0, 0)), 0);
		assert_eq!(GameBoard::bit_idx((4, 0)), 4);
		assert_eq!(GameBoard::bit_idx((0, 1)), 5);
		assert_eq!(GameBoard::bit_idx((4, 4)), 24);
	}

	#[test]
	#[should_panic(expected = "Board is only 5x5!")]
	fn bit_idx_rejects_x_out_of_range()
	{
		GameBoard::bit_idx((5, 0));
	}

	#[test]
	#[should_panic(expected = "Board is only 5x5!")]
	fn bit_idx_rejects_y_out_of_range()
	{
		GameBoard::bit_idx((0, 5));
	}

	#[test]
	fn set_and_get_every_point()
	{
		for y in 0..5
		{
			for x in 0..5
			{
				let mut board = empty_board();
				board.set((x, y), PieceType::Tiger);
				assert!(is_tiger(&board, (x, y)));
				board.set((x, y), PieceType::Sheep);
				assert!(is_sheep(&board, (x, y)));
				board.set((x, y), PieceType::Empty);
				assert!(is_empty(&board, (x, y)));
			}
		}
	}

	#[test]
	fn set_does_not_touch_sheep_count()
	{
		let mut board = empty_board();
		board.set((4, 4), PieceType::Sheep);
		board.set((4, 4), PieceType::Empty);
		assert_eq!(board.sheep_left(), 20);
	}

	#[test]
	fn default_has_tigers_in_corners_and_20_sheep()
	{
		let board = GameBoard::default();
		for y in 0..5
		{
			for x in 0..5
			{
				let corner = (x == 0 || x == 4) && (y == 0 || y == 4);
				assert_eq!(is_tiger(&board, (x, y)), corner, "at ({x}, {y})");
				assert!(!is_sheep(&board, (x, y)));
			}
		}
		assert_eq!(board.sheep_left(), 20);
	}

	// --- on_line ---

	#[test]
	fn on_line_orthogonal_everywhere()
	{
		assert!(GameBoard::on_line((1, 0), (2, 0), 1));
		assert!(GameBoard::on_line((1, 0), (1, 1), 1));
		assert!(GameBoard::on_line((1, 0), (3, 0), 2));
		assert!(GameBoard::on_line((2, 3), (2, 1), 2));
	}

	#[test]
	fn on_line_diagonal_only_from_even_points()
	{
		assert!(GameBoard::on_line((0, 0), (1, 1), 1));
		assert!(GameBoard::on_line((2, 2), (1, 3), 1));
		assert!(GameBoard::on_line((0, 0), (2, 2), 2));
		assert!(!GameBoard::on_line((1, 0), (2, 1), 1));
		assert!(!GameBoard::on_line((1, 2), (2, 3), 1));
	}

	#[test]
	fn on_line_rejects_wrong_distance_and_knight_moves()
	{
		assert!(!GameBoard::on_line((0, 0), (0, 0), 1));
		assert!(!GameBoard::on_line((0, 0), (2, 0), 1));
		assert!(!GameBoard::on_line((0, 0), (1, 0), 2));
		assert!(!GameBoard::on_line((0, 0), (1, 2), 1));
		assert!(!GameBoard::on_line((0, 0), (1, 2), 2));
	}

	// --- place_sheep ---

	#[test]
	fn place_sheep_places_and_decrements()
	{
		let mut board = empty_board();
		board.make_move(Move::PlaceSheep { pos: (2, 2) }).unwrap();
		assert!(is_sheep(&board, (2, 2)));
		assert_eq!(board.sheep_left(), 19);
	}

	#[test]
	fn place_all_20_sheep()
	{
		let mut board = empty_board();
		for i in 0..20
		{
			board.make_move(Move::PlaceSheep { pos: (i % 5, i / 5) }).unwrap();
		}
		assert_eq!(board.sheep_left(), 0);
	}

	#[test]
	fn place_sheep_rejects_when_none_left()
	{
		let mut board = empty_board();
		for i in 0..20
		{
			board.make_move(Move::PlaceSheep { pos: (i % 5, i / 5) }).unwrap();
		}
		assert_eq!(board.make_move(Move::PlaceSheep { pos: (0, 4) }), Err(MoveError::NoSheepLeft));
	}

	#[test]
	fn place_sheep_rejects_on_sheep()
	{
		let mut board = empty_board();
		board.make_move(Move::PlaceSheep { pos: (2, 2) }).unwrap();
		assert_eq!(board.make_move(Move::PlaceSheep { pos: (2, 2) }), Err(MoveError::PlaceOnOccupied));
	}

	#[test]
	fn place_sheep_rejects_on_tiger()
	{
		let mut board = GameBoard::default();
		assert_eq!(board.make_move(Move::PlaceSheep { pos: (0, 0) }), Err(MoveError::PlaceOnOccupied));
	}

	// --- move_sheep ---

	/// A board where every sheep has been placed, with sheep on `sheep` and tigers on `tigers`.
	fn movement_board(sheep: &[Pos], tigers: &[Pos]) -> GameBoard
	{
		let mut board = GameBoard { tiger_mask: 0, sheep_mask: 0 };
		for &pos in sheep { board.set(pos, PieceType::Sheep); }
		for &pos in tigers { board.set(pos, PieceType::Tiger); }
		board
	}

	#[test]
	fn move_sheep_rejects_while_sheep_in_hand()
	{
		let mut board = empty_board();
		board.make_move(Move::PlaceSheep { pos: (1, 2) }).unwrap();
		assert_eq!(board.make_move(Move::MoveSheep { from: (1, 2), to: (1, 3) }), Err(MoveError::SheepStillInHand));
	}

	#[test]
	fn move_sheep_allowed_after_placing_all_20()
	{
		let mut board = empty_board();
		for i in 0..20
		{
			board.make_move(Move::PlaceSheep { pos: (i % 5, i / 5) }).unwrap();
		}
		board.make_move(Move::MoveSheep { from: (0, 3), to: (0, 4) }).unwrap();
		assert!(is_empty(&board, (0, 3)));
		assert!(is_sheep(&board, (0, 4)));
	}

	#[test]
	fn move_sheep_orthogonal()
	{
		let mut board = movement_board(&[(1, 2)], &[]);
		board.make_move(Move::MoveSheep { from: (1, 2), to: (1, 3) }).unwrap();
		assert!(is_empty(&board, (1, 2)));
		assert!(is_sheep(&board, (1, 3)));
	}

	#[test]
	fn move_sheep_diagonal_from_even_point()
	{
		let mut board = movement_board(&[(2, 2)], &[]);
		board.make_move(Move::MoveSheep { from: (2, 2), to: (3, 1) }).unwrap();
		assert!(is_empty(&board, (2, 2)));
		assert!(is_sheep(&board, (3, 1)));
	}

	#[test]
	fn move_sheep_rejects_diagonal_from_odd_point()
	{
		let mut board = movement_board(&[(1, 2)], &[]);
		assert_eq!(board.make_move(Move::MoveSheep { from: (1, 2), to: (2, 3) }), Err(MoveError::SheepNotOneStep));
	}

	#[test]
	fn move_sheep_rejects_two_steps()
	{
		let mut board = movement_board(&[(0, 2)], &[]);
		assert_eq!(board.make_move(Move::MoveSheep { from: (0, 2), to: (2, 2) }), Err(MoveError::SheepNotOneStep));
	}

	#[test]
	fn move_sheep_rejects_from_empty()
	{
		let mut board = movement_board(&[], &[]);
		assert_eq!(board.make_move(Move::MoveSheep { from: (2, 2), to: (2, 3) }), Err(MoveError::NoSheepToMove));
	}

	#[test]
	fn move_sheep_rejects_from_tiger()
	{
		let mut board = movement_board(&[], &[(0, 0)]);
		assert_eq!(board.make_move(Move::MoveSheep { from: (0, 0), to: (1, 0) }), Err(MoveError::NoSheepToMove));
	}

	#[test]
	fn move_sheep_rejects_onto_sheep()
	{
		let mut board = movement_board(&[(2, 2), (2, 3)], &[]);
		assert_eq!(board.make_move(Move::MoveSheep { from: (2, 2), to: (2, 3) }), Err(MoveError::SheepOntoOccupied));
	}

	#[test]
	fn move_sheep_rejects_onto_tiger()
	{
		let mut board = movement_board(&[(1, 0)], &[(0, 0)]);
		assert_eq!(board.make_move(Move::MoveSheep { from: (1, 0), to: (0, 0) }), Err(MoveError::SheepOntoOccupied));
	}

	// --- kill_sheep ---

	#[test]
	fn kill_sheep_removes_sheep()
	{
		let mut board = empty_board();
		board.make_move(Move::PlaceSheep { pos: (2, 2) }).unwrap();
		board.kill_sheep((2, 2));
		assert!(is_empty(&board, (2, 2)));
		assert_eq!(board.sheep_left(), 19);
	}

	// --- get_moves / make_move ---

	/// Slow reference: try every (from, to) pair with the rule checks used by check_move.
	fn reference_moves(board: &GameBoard, turn: Turn) -> Vec<Move>
	{
		let all: Vec<Pos> = (0..25).map(pos_of).collect();
		let mut moves = Vec::new();
		match turn
		{
			Turn::Sheep if board.sheep_left() > 0 =>
			{
				for &pos in &all
				{
					if is_empty(board, pos) { moves.push(Move::PlaceSheep { pos }); }
				}
			}
			Turn::Sheep =>
			{
				for &from in &all
				{
					for &to in &all
					{
						if is_sheep(board, from) && is_empty(board, to) && GameBoard::on_line(from, to, 1)
						{
							moves.push(Move::MoveSheep { from, to });
						}
					}
				}
			}
			Turn::Tiger =>
			{
				for &from in &all
				{
					for &to in &all
					{
						if !is_tiger(board, from) || !is_empty(board, to) { continue; }
						if GameBoard::on_line(from, to, 1)
							|| (GameBoard::on_line(from, to, 2) && is_sheep(board, GameBoard::midpoint(from, to)))
						{
							moves.push(Move::MoveTiger { from, to });
						}
					}
				}
			}
		}
		moves.sort();
		moves
	}

	fn assert_moves_match(board: &GameBoard, turn: Turn)
	{
		let mut moves = board.get_moves(turn);
		moves.sort();
		assert_eq!(moves, reference_moves(board, turn), "turn {turn:?}");
	}

	#[test]
	fn get_moves_default_board()
	{
		let board = GameBoard::default();
		let tiger_moves = board.get_moves(Turn::Tiger);
		assert_eq!(tiger_moves.len(), 12);
		assert!(tiger_moves.contains(&Move::MoveTiger { from: (0, 0), to: (1, 1) }));
		assert!(tiger_moves.contains(&Move::MoveTiger { from: (4, 4), to: (4, 3) }));
		assert_eq!(board.get_moves(Turn::Sheep).len(), 21);
		assert_moves_match(&board, Turn::Tiger);
		assert_moves_match(&board, Turn::Sheep);
	}

	#[test]
	fn get_moves_tiger_jumps()
	{
		let board = movement_board(&[(1, 0), (1, 1), (0, 1), (3, 2)], &[(0, 0), (2, 2)]);
		let moves = board.get_moves(Turn::Tiger);
		assert!(moves.contains(&Move::MoveTiger { from: (0, 0), to: (2, 0) }));
		assert!(moves.contains(&Move::MoveTiger { from: (0, 0), to: (0, 2) }));
		assert!(moves.contains(&Move::MoveTiger { from: (2, 2), to: (4, 2) }));
		assert!(!moves.contains(&Move::MoveTiger { from: (2, 2), to: (0, 0) }));
		assert_moves_match(&board, Turn::Tiger);
	}

	#[test]
	fn get_moves_no_diagonal_from_odd_points()
	{
		let board = movement_board(&[(1, 2), (2, 1)], &[(1, 0)]);
		assert_moves_match(&board, Turn::Sheep);
		assert_moves_match(&board, Turn::Tiger);
		let moves = board.get_moves(Turn::Sheep);
		assert!(!moves.contains(&Move::MoveSheep { from: (1, 2), to: (2, 3) }));
	}

	#[test]
	fn get_moves_ignores_sheep_count_bits()
	{
		// A full 0b1111111 in hand must not be read as sheep on any board points.
		let board = GameBoard { tiger_mask: 1 << 24, sheep_mask: 0b1111111 * ONE_SHEEP };
		assert_moves_match(&board, Turn::Tiger);
		assert_eq!(board.get_moves(Turn::Sheep).len(), 24);
	}

	#[test]
	fn get_moves_matches_reference_on_random_boards()
	{
		let mut seed: u64 = 0x2545F4914F6CDD1D;
		let mut next = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
		for _ in 0..2000
		{
			let mut board = GameBoard { tiger_mask: 0, sheep_mask: (next() % 3) as u32 * ONE_SHEEP };
			for idx in 0..25
			{
				match next() % 4
				{
					0 => board.set(pos_of(idx), PieceType::Tiger),
					1 | 2 => board.set(pos_of(idx), PieceType::Sheep),
					_ => (),
				}
			}
			assert_moves_match(&board, Turn::Tiger);
			assert_moves_match(&board, Turn::Sheep);
		}
	}

	#[test]
	fn make_move_plays_every_generated_move()
	{
		let mut seed: u64 = 0x9E3779B97F4A7C15;
		let mut next = || { seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17; seed };
		let mut board = GameBoard::default();
		let mut turn = Turn::Sheep;
		for _ in 0..200
		{
			let moves = board.get_moves(turn);
			if moves.is_empty() { break; }
			board.make_move(moves[(next() % moves.len() as u64) as usize]).unwrap();
			turn = if turn == Turn::Sheep { Turn::Tiger } else { Turn::Sheep };
		}
	}

	#[test]
	fn make_move_dispatches_each_kind()
	{
		let mut board = GameBoard::default();
		board.make_move(Move::PlaceSheep { pos: (1, 0) }).unwrap();
		assert!(is_sheep(&board, (1, 0)));
		board.make_move(Move::MoveTiger { from: (0, 0), to: (2, 0) }).unwrap();
		assert!(is_tiger(&board, (2, 0)));
		assert!(is_empty(&board, (1, 0)));

		let mut board = movement_board(&[(2, 2)], &[]);
		board.make_move(Move::MoveSheep { from: (2, 2), to: (3, 3) }).unwrap();
		assert!(is_sheep(&board, (3, 3)));
	}

	#[test]
	fn check_move_accepts_every_generated_move()
	{
		let board = GameBoard::default();
		for turn in [Turn::Tiger, Turn::Sheep]
		{
			for mv in board.get_moves(turn)
			{
				assert_eq!(board.check_move(mv), Ok(()), "{mv:?}");
			}
		}
	}

	#[test]
	fn check_move_reports_each_error()
	{
		let board = GameBoard::default();
		assert_eq!(board.check_move(Move::PlaceSheep { pos: (0, 0) }), Err(MoveError::PlaceOnOccupied));
		assert_eq!(board.check_move(Move::MoveSheep { from: (1, 0), to: (2, 0) }), Err(MoveError::SheepStillInHand));
		assert_eq!(board.check_move(Move::MoveTiger { from: (2, 2), to: (2, 3) }), Err(MoveError::NoTigerToMove));
		assert_eq!(board.check_move(Move::MoveTiger { from: (0, 0), to: (4, 0) }), Err(MoveError::TigerOntoOccupied));
		assert_eq!(board.check_move(Move::MoveTiger { from: (0, 0), to: (2, 0) }), Err(MoveError::JumpOverNonSheep));
		assert_eq!(board.check_move(Move::MoveTiger { from: (0, 0), to: (1, 2) }), Err(MoveError::TigerNotOnLine));

		let board = movement_board(&[(1, 2), (1, 3)], &[]);
		assert_eq!(board.check_move(Move::PlaceSheep { pos: (0, 0) }), Err(MoveError::NoSheepLeft));
		assert_eq!(board.check_move(Move::MoveSheep { from: (2, 2), to: (2, 3) }), Err(MoveError::NoSheepToMove));
		assert_eq!(board.check_move(Move::MoveSheep { from: (1, 2), to: (1, 3) }), Err(MoveError::SheepOntoOccupied));
		assert_eq!(board.check_move(Move::MoveSheep { from: (1, 2), to: (2, 3) }), Err(MoveError::SheepNotOneStep));
	}

	#[test]
	fn make_move_rejects_illegal_move()
	{
		let mut board = GameBoard::default();
		assert_eq!(board.make_move(Move::MoveTiger { from: (0, 0), to: (2, 0) }), Err(MoveError::JumpOverNonSheep));
		assert!(is_tiger(&board, (0, 0)));
		assert!(is_empty(&board, (2, 0)));
	}

	// --- move_tiger ---

	#[test]
	fn move_tiger_orthogonal_step()
	{
		let mut board = GameBoard::default();
		board.make_move(Move::MoveTiger { from: (0, 0), to: (1, 0) }).unwrap();
		assert!(is_empty(&board, (0, 0)));
		assert!(is_tiger(&board, (1, 0)));
	}

	#[test]
	fn move_tiger_diagonal_step_from_even_point()
	{
		let mut board = GameBoard::default();
		board.make_move(Move::MoveTiger { from: (0, 0), to: (1, 1) }).unwrap();
		assert!(is_empty(&board, (0, 0)));
		assert!(is_tiger(&board, (1, 1)));
	}

	#[test]
	fn move_tiger_orthogonal_jump_kills_sheep()
	{
		let mut board = GameBoard::default();
		board.make_move(Move::PlaceSheep { pos: (1, 0) }).unwrap();
		board.make_move(Move::MoveTiger { from: (0, 0), to: (2, 0) }).unwrap();
		assert!(is_empty(&board, (0, 0)));
		assert!(is_empty(&board, (1, 0)));
		assert!(is_tiger(&board, (2, 0)));
	}

	#[test]
	fn move_tiger_diagonal_jump_kills_sheep()
	{
		let mut board = GameBoard::default();
		board.make_move(Move::PlaceSheep { pos: (1, 1) }).unwrap();
		board.make_move(Move::MoveTiger { from: (0, 0), to: (2, 2) }).unwrap();
		assert!(is_empty(&board, (0, 0)));
		assert!(is_empty(&board, (1, 1)));
		assert!(is_tiger(&board, (2, 2)));
	}

	#[test]
	fn move_tiger_jump_leaves_other_sheep_alone()
	{
		let mut board = GameBoard::default();
		board.make_move(Move::PlaceSheep { pos: (1, 0) }).unwrap();
		board.make_move(Move::PlaceSheep { pos: (0, 1) }).unwrap();
		board.make_move(Move::MoveTiger { from: (0, 0), to: (2, 0) }).unwrap();
		assert!(is_sheep(&board, (0, 1)));
		assert_eq!(board.sheep_left(), 18);
	}

	#[test]
	fn move_tiger_rejects_jumping_empty_point()
	{
		let mut board = GameBoard::default();
		assert_eq!(board.make_move(Move::MoveTiger { from: (0, 0), to: (2, 0) }), Err(MoveError::JumpOverNonSheep));
	}

	#[test]
	fn move_tiger_rejects_jumping_tiger()
	{
		let mut board = GameBoard::default();
		board.make_move(Move::MoveTiger { from: (0, 0), to: (1, 0) }).unwrap();
		board.make_move(Move::MoveTiger { from: (4, 0), to: (3, 0) }).unwrap();
		board.make_move(Move::MoveTiger { from: (3, 0), to: (2, 0) }).unwrap();
		assert_eq!(board.make_move(Move::MoveTiger { from: (1, 0), to: (3, 0) }), Err(MoveError::JumpOverNonSheep));
	}

	#[test]
	fn move_tiger_rejects_diagonal_from_odd_point()
	{
		let mut board = GameBoard::default();
		board.make_move(Move::MoveTiger { from: (0, 0), to: (1, 0) }).unwrap();
		assert_eq!(board.make_move(Move::MoveTiger { from: (1, 0), to: (2, 1) }), Err(MoveError::TigerNotOnLine));
	}

	#[test]
	fn move_tiger_rejects_diagonal_jump_from_odd_point()
	{
		let mut board = GameBoard::default();
		board.make_move(Move::MoveTiger { from: (0, 0), to: (1, 0) }).unwrap();
		board.make_move(Move::PlaceSheep { pos: (2, 1) }).unwrap();
		assert_eq!(board.make_move(Move::MoveTiger { from: (1, 0), to: (3, 2) }), Err(MoveError::TigerNotOnLine));
	}

	#[test]
	fn move_tiger_rejects_three_steps()
	{
		let mut board = GameBoard::default();
		assert_eq!(board.make_move(Move::MoveTiger { from: (0, 0), to: (3, 0) }), Err(MoveError::TigerNotOnLine));
	}

	#[test]
	fn move_tiger_rejects_knight_move()
	{
		let mut board = GameBoard::default();
		assert_eq!(board.make_move(Move::MoveTiger { from: (0, 0), to: (1, 2) }), Err(MoveError::TigerNotOnLine));
	}

	#[test]
	fn move_tiger_rejects_from_empty()
	{
		let mut board = GameBoard::default();
		assert_eq!(board.make_move(Move::MoveTiger { from: (2, 2), to: (2, 3) }), Err(MoveError::NoTigerToMove));
	}

	#[test]
	fn move_tiger_rejects_from_sheep()
	{
		let mut board = GameBoard::default();
		board.make_move(Move::PlaceSheep { pos: (2, 2) }).unwrap();
		assert_eq!(board.make_move(Move::MoveTiger { from: (2, 2), to: (2, 3) }), Err(MoveError::NoTigerToMove));
	}

	#[test]
	fn move_tiger_rejects_onto_sheep()
	{
		let mut board = GameBoard::default();
		board.make_move(Move::PlaceSheep { pos: (1, 0) }).unwrap();
		assert_eq!(board.make_move(Move::MoveTiger { from: (0, 0), to: (1, 0) }), Err(MoveError::TigerOntoOccupied));
	}

	#[test]
	fn move_tiger_rejects_onto_tiger()
	{
		let mut board = GameBoard::default();
		board.make_move(Move::MoveTiger { from: (0, 0), to: (1, 0) }).unwrap();
		board.make_move(Move::MoveTiger { from: (1, 0), to: (0, 0) }).unwrap();
		board.make_move(Move::MoveTiger { from: (0, 0), to: (1, 0) }).unwrap();
		board.make_move(Move::MoveTiger { from: (4, 0), to: (3, 0) }).unwrap();
		board.make_move(Move::MoveTiger { from: (3, 0), to: (2, 0) }).unwrap();
		assert_eq!(board.make_move(Move::MoveTiger { from: (2, 0), to: (1, 0) }), Err(MoveError::TigerOntoOccupied));
	}

	// --- sheep_left / sheep_count ---

	#[test]
	fn sheep_left_counts_down_only_when_placing()
	{
		let mut board = GameBoard::default();
		assert_eq!(board.sheep_left(), 20);
		board.make_move(Move::PlaceSheep { pos: (1, 0) }).unwrap();
		assert_eq!(board.sheep_left(), 19);
		board.make_move(Move::MoveTiger { from: (0, 0), to: (2, 0) }).unwrap();
		assert_eq!(board.sheep_left(), 19);
		board.make_move(Move::PlaceSheep { pos: (2, 2) }).unwrap();
		assert_eq!(board.sheep_left(), 18);
	}

	#[test]
	fn sheep_count_counts_sheep_on_board()
	{
		let mut board = GameBoard::default();
		assert_eq!(board.sheep_count(), 0);
		board.make_move(Move::PlaceSheep { pos: (1, 0) }).unwrap();
		board.make_move(Move::MoveTiger { from: (4, 4), to: (3, 3) }).unwrap();
		board.make_move(Move::PlaceSheep { pos: (2, 2) }).unwrap();
		assert_eq!(board.sheep_count(), 2);
		board.make_move(Move::MoveTiger { from: (0, 0), to: (2, 0) }).unwrap();
		assert_eq!(board.sheep_count(), 1);
		assert_eq!(board.sheep_left(), 18);
	}

	#[test]
	fn sheep_count_ignores_sheep_in_hand()
	{
		let board = GameBoard { tiger_mask: 0, sheep_mask: 0b1111111 * ONE_SHEEP };
		assert_eq!(board.sheep_count(), 0);
		let board = movement_board(&[(0, 0), (4, 4), (2, 3)], &[]);
		assert_eq!(board.sheep_count(), 3);
	}

	// --- check_win ---

	#[test]
	fn check_win_not_at_start()
	{
		let board = GameBoard::default();
		assert!(!board.check_win(Turn::Sheep));
		assert!(!board.check_win(Turn::Tiger));
	}

	#[test]
	fn check_win_after_fifth_kill()
	{
		let mut board = GameBoard::default();
		let kills = [
			((1, 0), (0, 0), (2, 0)),
			((1, 4), (0, 4), (2, 4)),
			((4, 1), (4, 0), (4, 2)),
			((2, 1), (2, 0), (2, 2)),
			((1, 1), (2, 2), (0, 0)),
		];
		for (i, &(sheep, from, to)) in kills.iter().enumerate()
		{
			assert!(!board.check_win(Turn::Sheep), "before kill {}", i + 1);
			board.make_move(Move::PlaceSheep { pos: sheep }).unwrap();
			board.make_move(Move::MoveTiger { from, to }).unwrap();
			assert!(is_empty(&board, sheep));
		}
		assert_eq!(board.sheep_count() + board.sheep_left(), 15);
		// Checked before the sheep's turn, so the tigers (who just moved) have won.
		assert!(board.check_win(Turn::Sheep));
	}

	#[test]
	fn check_win_counts_sheep_on_board_and_in_hand()
	{
		// Sheep fill the left two columns, well away from the tiger in the far corner.
		let on_board: Vec<Pos> = (0..2).flat_map(|x| (0..5).map(move |y| (x, y))).collect();
		let mut board = movement_board(&on_board[..5], &[(4, 4)]);
		board.sheep_mask |= 10 * ONE_SHEEP;
		assert!(board.check_win(Turn::Sheep), "5 on board + 10 in hand");

		let mut board = movement_board(&on_board[..6], &[(4, 4)]);
		board.sheep_mask |= 10 * ONE_SHEEP;
		assert!(!board.check_win(Turn::Sheep), "6 on board + 10 in hand");
		assert!(!board.check_win(Turn::Tiger), "6 on board + 10 in hand");
	}

	#[test]
	fn check_win_when_tigers_are_trapped()
	{
		let mut board = movement_board(&[(1, 0), (0, 1), (1, 1), (2, 0), (0, 2), (2, 2)], &[(0, 0)]);
		board.sheep_mask |= 14 * ONE_SHEEP;
		assert!(board.get_moves(Turn::Tiger).is_empty());
		assert!(board.check_win(Turn::Tiger));
		assert!(!board.check_win(Turn::Sheep));
	}

	#[test]
	fn check_win_when_sheep_are_trapped()
	{
		// The only empty point is (4, 4), and every point next to it holds a tiger.
		let tigers = [(0, 0), (3, 3), (4, 3), (3, 4)];
		let sheep: Vec<Pos> = (0..25).map(pos_of).filter(|p| !tigers.contains(p) && *p != (4, 4)).collect();
		assert_eq!(sheep.len(), 20);
		let board = movement_board(&sheep, &tigers);
		assert!(board.get_moves(Turn::Sheep).is_empty());
		assert!(board.check_win(Turn::Sheep));
		assert!(!board.check_win(Turn::Tiger));
	}
}
