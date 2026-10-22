use ratatui::{buffer::Buffer, layout::Rect, style::{Color, Modifier, Style}, widgets::Widget};

use crate::{board::{GameBoard, Move, PieceType, Pos}, player::Highlights};

/// Columns between neighbouring points at scale 1. Twice `ROW_STEP`, since cells are about twice as tall as wide.
const COL_STEP: u16 = 4;
/// Rows between neighbouring points at scale 1.
const ROW_STEP: u16 = 2;
/// Columns taken by the rank labels on the left.
const LEFT: u16 = 3;

/// The spacing between points, in (columns, rows), at `scale`.
const fn steps(scale: u16) -> (u16, u16)
{
	(COL_STEP * scale, ROW_STEP * scale)
}

/// How far a drawn piece reaches past its point, in (columns, rows), at `scale`:
/// `T` at scale 1, `[T]` at scale 2, and a 3x3 box from scale 3 up.
const fn reach(scale: u16) -> (u16, u16)
{
	match scale
	{
		1 => (0, 0),
		2 => (1, 0),
		_ => (1, 1),
	}
}

/// The board with its lines, rank labels down the left and file labels underneath.
/// It is drawn at the largest whole-number scale that fits its area; at scale 1 it looks like:
///
/// ```text
/// 5  T───·───·───·───T
///    │ ╲ │ ╱ │ ╲ │ ╱ │
/// 4  ·───·───·───·───·
/// …
/// 1  T───·───·───·───T
///
///    a   b   c   d   e
/// ```
pub struct BoardView
{
	pub board: GameBoard,
	pub highlights: Highlights,
	pub last_move: Option<Move>,
}

impl BoardView
{
	/// The (width, height) the board needs at `scale` (1 or more): the grid, the pieces sticking
	/// out past its edge, then a blank row and the file labels.
	pub const fn size(scale: u16) -> (u16, u16)
	{
		let (cs, rs) = steps(scale);
		let (rx, ry) = reach(scale);
		(LEFT + 4 * cs + 1 + rx, ry + 4 * rs + 1 + ry + 2)
	}

	/// The largest scale whose board fits in `width` by `height`; 1 if even that doesn't fit.
	pub fn scale_to_fit(width: u16, height: u16) -> u16
	{
		let fits = |scale| { let (w, h) = BoardView::size(scale); w <= width && h <= height };
		let mut scale = 1;
		while fits(scale + 1)
		{
			scale += 1;
		}
		scale
	}

	/// The screen cell a point is drawn at at `scale`, relative to the widget's top-left.
	/// The grid sits below a row for the top pieces' boxes once they are boxed.
	fn cell((x, y): Pos, scale: u16) -> (u16, u16)
	{
		let (cs, rs) = steps(scale);
		(LEFT + x as u16 * cs, reach(scale).1 + y as u16 * rs)
	}

	fn piece_style(&self, pos: Pos, piece: &PieceType) -> Style
	{
		let mut style = match piece
		{
			PieceType::Tiger => Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
			PieceType::Sheep => Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
			PieceType::Empty => Style::new().fg(Color::DarkGray),
		};
		let (from, to) = match self.last_move
		{
			Some(Move::PlaceSheep { pos }) => (None, Some(pos)),
			Some(Move::MoveSheep { from, to } | Move::MoveTiger { from, to }) => (Some(from), Some(to)),
			None => (None, None),
		};
		if from == Some(pos) || to == Some(pos)
		{
			style = style.add_modifier(Modifier::UNDERLINED);
		}
		if self.highlights.targets.contains(&pos)
		{
			style = style.fg(Color::Green).add_modifier(Modifier::BOLD);
		}
		if self.highlights.selected == Some(pos)
		{
			style = style.add_modifier(Modifier::REVERSED);
		}
		style
	}
}

impl Widget for &BoardView
{
	fn render(self, area: Rect, buf: &mut Buffer)
	{
		let scale = BoardView::scale_to_fit(area.width, area.height);
		let (cs, rs) = steps(scale);
		let lines = Style::new().fg(Color::Gray);
		let labels = Style::new().fg(Color::Cyan);
		// Clip to the area so a small terminal doesn't index outside the buffer
		let mut put = |cx: u16, cy: u16, s: &str, style: Style|
		{
			if cx < area.width && cy < area.height
			{
				buf.set_string(area.x + cx, area.y + cy, s, style);
			}
		};

		// Lines first, so the pieces are drawn over them
		for y in 0..5u8
		{
			for x in 0..5u8
			{
				let (cx, cy) = BoardView::cell((x, y), scale);
				if x < 4
				{
					put(cx + 1, cy, &"─".repeat(cs as usize - 1), lines);
				}
				if y < 4
				{
					for k in 1..rs
					{
						put(cx, cy + k, "│", lines);
					}
					if x < 4
					{
						// Diagonals run through points where x + y is even (see `GameBoard::on_line`).
						// Each row down the diagonal moves cs / rs = 2 columns across.
						let down_right = (x + y) % 2 == 0;
						for k in 1..rs
						{
							let col = if down_right { cx + 2 * k } else { cx + cs - 2 * k };
							put(col, cy + k, if down_right { "╲" } else { "╱" }, lines);
						}
					}
				}
			}
		}

		for y in 0..5u8
		{
			let (_, cy) = BoardView::cell((0, y), scale);
			put(0, cy, &(5 - y).to_string(), labels);
			for x in 0..5u8
			{
				let (cx, cy) = BoardView::cell((x, y), scale);
				let piece = self.board.get((x, y));
				let targeted = self.highlights.targets.contains(&(x, y));
				let symbol = match piece
				{
					PieceType::Tiger => "T",
					PieceType::Sheep => "S",
					PieceType::Empty if targeted => "●",
					PieceType::Empty => "·",
				};
				let style = self.piece_style((x, y), &piece);
				// The last move's underline is only for the symbol, not the box around it
				let frame = style.remove_modifier(Modifier::UNDERLINED);
				let boxed = targeted || !matches!(piece, PieceType::Empty);
				match scale
				{
					_ if !boxed => put(cx, cy, symbol, style),
					1 => put(cx, cy, symbol, style),
					2 =>
					{
						put(cx - 1, cy, "[", frame);
						put(cx, cy, symbol, style);
						put(cx + 1, cy, "]", frame);
					}
					_ =>
					{
						// Where a line meets the box, join it on
						put(cx - 1, cy - 1, if y > 0 { "┌┴┐" } else { "┌─┐" }, frame);
						put(cx - 1, cy, if x > 0 { "┤" } else { "│" }, frame);
						put(cx, cy, symbol, style);
						put(cx + 1, cy, if x < 4 { "├" } else { "│" }, frame);
						put(cx - 1, cy + 1, if y < 4 { "└┬┘" } else { "└─┘" }, frame);
					}
				}
			}
		}
		let files_row = BoardView::size(scale).1 - 1;
		for x in 0..5u8
		{
			let (cx, _) = BoardView::cell((x, 0), scale);
			put(cx, files_row, &((b'a' + x) as char).to_string(), labels);
		}
	}
}

/// The board point nearest a click at screen cell (`column`, `row`), for a `BoardView` drawn in `area`
/// (at the same scale `render` picks for it). Clicks on the lines between points go to the nearest
/// point; clicks off the grid give None.
pub fn pos_at(area: Rect, column: u16, row: u16) -> Option<Pos>
{
	let scale = BoardView::scale_to_fit(area.width, area.height);
	let (cs, rs) = steps(scale);
	let (rx, ry) = reach(scale);
	// Offsets from the top-left point, letting clicks land on the edge pieces' boxes
	let cx = column.checked_sub(area.x + LEFT - rx)?;
	let cy = row.checked_sub(area.y)?;
	if cx > 4 * cs + 1 + rx || cy > 4 * rs + 2 * ry
	{
		return None;
	}
	// Shifted by the reach, so (cx, cy) are offsets from one column/row before the first point
	let x = ((cx + cs / 2 - rx) / cs).min(4);
	let y = ((cy + rs / 2 - ry) / rs).min(4);
	Some((x as u8, y as u8))
}

#[cfg(test)]
mod tests
{
	use super::*;

	fn rendered(view: &BoardView, area: Rect) -> Buffer
	{
		let mut buf = Buffer::empty(Rect::new(0, 0, area.right() + 2, area.bottom() + 2));
		view.render(area, &mut buf);
		buf
	}

	fn symbol(buf: &Buffer, x: u16, y: u16) -> &str
	{
		buf[(x, y)].symbol()
	}

	#[test]
	fn renders_default_board_with_labels()
	{
		let area = Rect::new(1, 1, BoardView::size(1).0, BoardView::size(1).1);
		let view = BoardView { board: GameBoard::default(), highlights: Highlights::default(), last_move: None };
		let buf = rendered(&view, area);
		for (pos, s) in [((0, 0), "T"), ((4, 0), "T"), ((0, 4), "T"), ((4, 4), "T"), ((2, 2), "·")]
		{
			let (cx, cy) = BoardView::cell(pos, 1);
			assert_eq!(symbol(&buf, area.x + cx, area.y + cy), s, "{pos:?}");
		}
		assert_eq!(symbol(&buf, area.x, area.y), "5");
		assert_eq!(symbol(&buf, area.x, area.y + 8), "1");
		assert_eq!(symbol(&buf, area.x + LEFT, area.y + BoardView::size(1).1 - 1), "a");
		assert_eq!(symbol(&buf, area.x + LEFT + 16, area.y + BoardView::size(1).1 - 1), "e");
		// (0, 0) has an even sum, so its diagonal runs down-right
		assert_eq!(symbol(&buf, area.x + LEFT + 2, area.y + 1), "╲");
		assert_eq!(symbol(&buf, area.x + LEFT + 6, area.y + 1), "╱");
	}

	#[test]
	fn renders_target_highlights()
	{
		let area = Rect::new(0, 0, BoardView::size(1).0, BoardView::size(1).1);
		let highlights = Highlights { selected: Some((0, 0)), targets: vec![(1, 1)] };
		let view = BoardView { board: GameBoard::default(), highlights, last_move: None };
		let buf = rendered(&view, area);
		let (cx, cy) = BoardView::cell((1, 1), 1);
		assert_eq!(symbol(&buf, cx, cy), "●");
		let (cx, cy) = BoardView::cell((0, 0), 1);
		assert!(buf[(cx, cy)].modifier.contains(Modifier::REVERSED));
	}

	#[test]
	fn renders_into_a_small_area_without_panicking()
	{
		let view = BoardView { board: GameBoard::default(), highlights: Highlights::default(), last_move: None };
		rendered(&view, Rect::new(0, 0, 5, 3));
	}

	#[test]
	fn scale_to_fit_picks_the_largest_board_that_fits()
	{
		for scale in 1..5
		{
			let (w, h) = BoardView::size(scale);
			assert_eq!(BoardView::scale_to_fit(w, h), scale);
			assert_eq!(BoardView::scale_to_fit(w + 5, h + 5), scale);
			assert_eq!(BoardView::scale_to_fit(w - 1, h), (scale - 1).max(1));
			assert_eq!(BoardView::scale_to_fit(w, h - 1), (scale - 1).max(1));
		}
		assert_eq!(BoardView::scale_to_fit(0, 0), 1);
	}

	#[test]
	fn renders_bigger_when_there_is_room()
	{
		let (w, h) = BoardView::size(2);
		let area = Rect::new(0, 0, w, h);
		let view = BoardView { board: GameBoard::default(), highlights: Highlights::default(), last_move: None };
		let buf = rendered(&view, area);
		let (cx, cy) = BoardView::cell((4, 4), 2);
		assert_eq!((cx, cy), (LEFT + 32, 16));
		assert_eq!(symbol(&buf, cx, cy), "T");
		assert_eq!(symbol(&buf, 0, 16), "1");
		assert_eq!(symbol(&buf, cx, h - 1), "e");
		// Lines fill the gaps: three rows of │ between points, a diagonal two columns per row
		for k in 1..4
		{
			assert_eq!(symbol(&buf, LEFT, k), "│");
			assert_eq!(symbol(&buf, LEFT + 2 * k, k), "╲");
		}
		assert_eq!(symbol(&buf, LEFT + 7, 0), "─");
	}

	fn row(buf: &Buffer, y: u16) -> String
	{
		(0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
	}

	#[test]
	fn pieces_get_brackets_at_scale_2()
	{
		let (w, h) = BoardView::size(2);
		let highlights = Highlights { selected: None, targets: vec![(1, 1)] };
		let view = BoardView { board: GameBoard::default(), highlights, last_move: None };
		let buf = rendered(&view, Rect::new(0, 0, w, h));
		assert!(row(&buf, 0).starts_with("5 [T]──────·"), "{}", row(&buf, 0));
		assert!(row(&buf, 4).starts_with("4  ·──────[●]──────·"), "{}", row(&buf, 4));
	}

	#[test]
	fn pieces_get_boxes_from_scale_3()
	{
		let (w, h) = BoardView::size(3);
		let mut board = GameBoard::default();
		board.make_move(Move::PlaceSheep { pos: (2, 2) }).unwrap();
		let view = BoardView { board, highlights: Highlights::default(), last_move: None };
		let buf = rendered(&view, Rect::new(0, 0, w, h));
		// Corner tiger: lines only join on the sides that have them
		assert!(row(&buf, 0).starts_with("  ┌─┐"), "{}", row(&buf, 0));
		assert!(row(&buf, 1).starts_with("5 │T├───"), "{}", row(&buf, 1));
		assert!(row(&buf, 2).starts_with("  └┬┘"), "{}", row(&buf, 2));
		// Centre sheep: lines join on all four sides
		let (cx, cy) = BoardView::cell((2, 2), 3);
		let around = |dy: u16| (cx - 1..=cx + 1).map(|x| buf[(x, cy + dy - 1)].symbol()).collect::<String>();
		assert_eq!([around(0), around(1), around(2)], ["┌┴┐", "┤S├", "└┬┘"]);
		assert_eq!(symbol(&buf, cx, h - 1), "c");
	}

	#[test]
	fn pos_at_takes_clicks_on_a_piece_box()
	{
		let (w, h) = BoardView::size(3);
		let area = Rect::new(4, 3, w, h);
		for pos in [(0, 0), (4, 4), (2, 2), (4, 0)]
		{
			let (cx, cy) = BoardView::cell(pos, 3);
			for (dx, dy) in [(-1, -1), (1, 1), (-1, 1), (1, -1)]
			{
				let column = (area.x + cx).checked_add_signed(dx).unwrap();
				let row = (area.y + cy).checked_add_signed(dy).unwrap();
				assert_eq!(pos_at(area, column, row), Some(pos), "{pos:?} {dx},{dy}");
			}
		}
		assert_eq!(pos_at(area, area.x + 1, area.y), None);
		assert_eq!(pos_at(area, area.x + LEFT, area.y + h - 1), None); // file labels
	}

	#[test]
	fn pos_at_finds_every_drawn_point()
	{
		for scale in 1..6
		{
			let (w, h) = BoardView::size(scale);
			let area = Rect::new(3, 2, w, h);
			for y in 0..5
			{
				for x in 0..5
				{
					let (cx, cy) = BoardView::cell((x, y), scale);
					assert_eq!(pos_at(area, area.x + cx, area.y + cy), Some((x, y)), "scale {scale}");
				}
			}
		}
	}

	#[test]
	fn pos_at_snaps_line_clicks_to_nearest_point()
	{
		let area = Rect::new(0, 0, BoardView::size(1).0, BoardView::size(1).1);
		assert_eq!(pos_at(area, LEFT + 1, 0), Some((0, 0)));
		assert_eq!(pos_at(area, LEFT + 3, 0), Some((1, 0)));
		assert_eq!(pos_at(area, LEFT + 16, 1), Some((4, 1)));
	}

	#[test]
	fn pos_at_rejects_clicks_off_the_grid()
	{
		let area = Rect::new(2, 2, BoardView::size(1).0, BoardView::size(1).1);
		assert_eq!(pos_at(area, 0, 0), None);
		assert_eq!(pos_at(area, area.x, area.y), None); // rank label
		assert_eq!(pos_at(area, area.x + LEFT, area.y + 9), None); // below the last rank
		assert_eq!(pos_at(area, area.x + LEFT + 18, area.y), None);
	}
}
