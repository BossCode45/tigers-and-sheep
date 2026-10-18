use std::io::{self, BufRead, Write};

use crate::{board::{GameBoard, Move, PieceType, Turn}, player::Player};

/// A human player at the terminal. Shows the board, lists the legal moves and reads a choice from stdin.
pub struct LocalPlayer;

impl Player for LocalPlayer
{
	fn pick_move(&mut self, board: GameBoard, turn: Turn) -> Move
	{
		let mut moves = board.get_moves(turn);
		moves.sort();
		print!("{}", render_board(&board));
		println!();
		println!("{:?} to move. Sheep in hand: {}, on board: {}", turn, board.sheep_left(), board.sheep_count());
		for (i, mv) in moves.iter().enumerate()
		{
			println!("{:>3}) {}", i + 1, describe_move(*mv));
		}
		println!("Enter a move number, a point \"x y\" to place, or \"x y x y\" to move from/to. \"q\" quits.");

		let stdin = io::stdin();
		let mut lines = stdin.lock().lines();
		loop
		{
			print!("> ");
			io::stdout().flush().unwrap();
			let line = match lines.next()
			{
				Some(Ok(line)) => line,
				_ => std::process::exit(0),
			};
			if line.trim() == "q"
			{
				std::process::exit(0);
			}
			match parse_choice(&line, &board, turn, &moves)
			{
				Ok(mv) => return mv,
				Err(err) => println!("{err}"),
			}
		}
	}
}

/// Turn a line of input into one of `moves`, or explain why it isn't one.
fn parse_choice(input: &str, board: &GameBoard, turn: Turn, moves: &[Move]) -> Result<Move, String>
{
	let nums: Vec<u8> = input
		.split(|c: char| c.is_whitespace() || c == ',')
		.filter(|s| !s.is_empty())
		.map(|s| s.parse().map_err(|_| format!("\"{s}\" isn't a number")))
		.collect::<Result<_, _>>()?;

	let mv = match nums[..]
	{
		[n] =>
		{
			return moves.get((n as usize).wrapping_sub(1)).copied()
				.ok_or_else(|| format!("Pick a move from 1 to {}", moves.len()));
		}
		[x, y] => Move::PlaceSheep { pos: (x, y) },
		[fx, fy, tx, ty] => match turn
		{
			Turn::Sheep => Move::MoveSheep { from: (fx, fy), to: (tx, ty) },
			Turn::Tiger => Move::MoveTiger { from: (fx, fy), to: (tx, ty) },
		},
		_ => return Err("Enter 1, 2 or 4 numbers".to_string()),
	};

	if points(mv).iter().any(|&(x, y)| x > 4 || y > 4)
	{
		return Err("Points go from 0 to 4".to_string());
	}
	if moves.contains(&mv)
	{
		return Ok(mv);
	}
	match board.check_move(mv)
	{
		Err(err) => Err(err.to_string()),
		// Legal by the rules but not this side's move, e.g. placing as the tiger.
		Ok(()) => Err(format!("{} isn't a move for {:?}", describe_move(mv), turn)),
	}
}

fn points(mv: Move) -> Vec<(u8, u8)>
{
	match mv
	{
		Move::PlaceSheep { pos } => vec![pos],
		Move::MoveSheep { from, to } | Move::MoveTiger { from, to } => vec![from, to],
	}
}

fn describe_move(mv: Move) -> String
{
	match mv
	{
		Move::PlaceSheep { pos: (x, y) } => format!("Place sheep at {x} {y}"),
		Move::MoveSheep { from: (fx, fy), to: (tx, ty) } => format!("Sheep {fx} {fy} -> {tx} {ty}"),
		Move::MoveTiger { from: (fx, fy), to: (tx, ty) } =>
		{
			let jump = if fx.abs_diff(tx) == 2 || fy.abs_diff(ty) == 2 { " (jump)" } else { "" };
			format!("Tiger {fx} {fy} -> {tx} {ty}{jump}")
		}
	}
}

/// The board with its lines, x across the top and y down the side:
///
/// ```text
///     0   1   2   3   4
/// 0   T - . - . - . - T
///     | \ | / | \ | / |
/// 1   . - . - . - . - .
/// ```
fn render_board(board: &GameBoard) -> String
{
	let mut out = String::from("    0   1   2   3   4\n");
	for y in 0..5u8
	{
		out += &format!("{y}   ");
		for x in 0..5u8
		{
			out += match board.get((x, y))
			{
				PieceType::Tiger => "T",
				PieceType::Sheep => "S",
				PieceType::Empty => ".",
			};
			if x < 4
			{
				out += " - ";
			}
		}
		out += "\n";
		if y < 4
		{
			out += "    ";
			for x in 0..5u8
			{
				out += "|";
				if x < 4
				{
					out += if (x + y) % 2 == 0 { " \\ " } else { " / " };
				}
			}
			out += "\n";
		}
	}
	out
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn render_default_board()
	{
		let expected = "    0   1   2   3   4\n\
			0   T - . - . - . - T\n    | \\ | / | \\ | / |\n\
			1   . - . - . - . - .\n    | / | \\ | / | \\ |\n\
			2   . - . - . - . - .\n    | \\ | / | \\ | / |\n\
			3   . - . - . - . - .\n    | / | \\ | / | \\ |\n\
			4   T - . - . - . - T\n";
		assert_eq!(render_board(&GameBoard::default()), expected);
	}

	#[test]
	fn parse_choice_by_number()
	{
		let board = GameBoard::default();
		let moves = board.get_moves(Turn::Tiger);
		assert_eq!(parse_choice("1", &board, Turn::Tiger, &moves), Ok(moves[0]));
		assert_eq!(parse_choice(" 3 ", &board, Turn::Tiger, &moves), Ok(moves[2]));
		assert!(parse_choice("0", &board, Turn::Tiger, &moves).is_err());
		assert!(parse_choice("99", &board, Turn::Tiger, &moves).is_err());
	}

	#[test]
	fn parse_choice_by_points()
	{
		let board = GameBoard::default();
		let sheep = board.get_moves(Turn::Sheep);
		let tiger = board.get_moves(Turn::Tiger);
		assert_eq!(parse_choice("2 2", &board, Turn::Sheep, &sheep), Ok(Move::PlaceSheep { pos: (2, 2) }));
		assert_eq!(parse_choice("2,2", &board, Turn::Sheep, &sheep), Ok(Move::PlaceSheep { pos: (2, 2) }));
		assert_eq!(parse_choice("0 0 1 1", &board, Turn::Tiger, &tiger), Ok(Move::MoveTiger { from: (0, 0), to: (1, 1) }));
	}

	#[test]
	fn parse_choice_explains_bad_input()
	{
		let board = GameBoard::default();
		let sheep = board.get_moves(Turn::Sheep);
		let tiger = board.get_moves(Turn::Tiger);
		assert_eq!(parse_choice("0 0", &board, Turn::Sheep, &sheep), Err("Can't place a sheep on an occupied point!".to_string()));
		assert_eq!(parse_choice("0 0 2 0", &board, Turn::Tiger, &tiger), Err("Tigers can only jump over a sheep!".to_string()));
		assert_eq!(parse_choice("2 2", &board, Turn::Tiger, &tiger), Err("Place sheep at 2 2 isn't a move for Tiger".to_string()));
		assert_eq!(parse_choice("5 0", &board, Turn::Sheep, &sheep), Err("Points go from 0 to 4".to_string()));
		assert!(parse_choice("a", &board, Turn::Sheep, &sheep).is_err());
		assert!(parse_choice("1 2 3", &board, Turn::Sheep, &sheep).is_err());
		assert!(parse_choice("", &board, Turn::Sheep, &sheep).is_err());
	}
}
