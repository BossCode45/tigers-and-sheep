use std::{io, time::Duration};

use ratatui::{
	crossterm::{event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind}, execute},
	layout::{Constraint, Layout, Margin, Rect},
	style::{Color, Modifier, Style},
	symbols::merge::MergeStrategy,
	text::{Line, Span},
	widgets::{Block, Paragraph},
	DefaultTerminal, Frame,
};

use crate::{board::{GameBoard, Move, Turn}, board_view::{pos_at, BoardView}, player::{Input, Player}};

/// Runs a game in the terminal: draws the board, the move history and the current player's panel,
/// and passes mouse clicks on the board to the player whose turn it is.
#[derive(Default)]
pub struct GameHandler
{
	start: GameBoard,
	/// Every move played so far, with the board after it. Sheep play the even-numbered moves.
	history: Vec<(Move, GameBoard)>,
	/// Where the last draw put the board, for turning clicks into points.
	board_area: Rect,
	/// Where the last draw put each visible history entry, with its index in `history`.
	history_rows: Vec<(Rect, usize)>,
}

/// What to do after an event.
enum Action
{
	None,
	Quit,
	Play(Move),
	Revert(usize),
}

/// A bordered pane whose borders join up with its neighbours' where they overlap.
fn panel() -> Block<'static>
{
	Block::bordered().merge_borders(MergeStrategy::Exact)
}

fn side_name(turn: Turn) -> &'static str
{
	match turn
	{
		Turn::Sheep => "Sheep",
		Turn::Tiger => "Tigers",
	}
}

impl GameHandler
{
	/// The current board.
	pub fn board(&self) -> GameBoard
	{
		self.history.last().map_or(self.start, |&(_, board)| board)
	}

	/// The side to move. Sheep move first.
	pub fn turn(&self) -> Turn
	{
		if self.history.len().is_multiple_of(2) { Turn::Sheep } else { Turn::Tiger }
	}

	/// The side that has won, if the game is over.
	pub fn winner(&self) -> Option<Turn>
	{
		// check_win runs before `turn`'s move, so whoever moved last has won.
		self.board().check_win(self.turn()).then(|| self.turn().next_turn())
	}

	/// Play `m` for the side to move. Panics if it is illegal.
	pub fn play(&mut self, m: Move)
	{
		let mut board = self.board();
		board.make_move(m).unwrap();
		self.history.push((m, board));
	}

	/// Go back to the board before move `i` (0-based), dropping it and every move after it.
	pub fn revert(&mut self, i: usize)
	{
		self.history.truncate(i);
	}

	/// Play in the terminal with `p1` as the sheep and `p2` as the tigers, until someone wins and
	/// the user quits. Returns the winner, or None if the user quit before the game was won.
	pub fn play_game<T1: Player, T2: Player>(&mut self, p1: &mut T1, p2: &mut T2) -> io::Result<Option<Turn>>
	{
		let mut terminal = ratatui::init();
		let result = execute!(io::stdout(), EnableMouseCapture)
			.and_then(|()| self.run(&mut terminal, [p1, p2]));
		let _ = execute!(io::stdout(), DisableMouseCapture);
		ratatui::restore();
		result
	}

	fn run(&mut self, terminal: &mut DefaultTerminal, mut players: [&mut dyn Player; 2]) -> io::Result<Option<Turn>>
	{
		let idx = |turn: Turn| match turn { Turn::Sheep => 0, Turn::Tiger => 1 };
		loop
		{
			let (board, turn) = (self.board(), self.turn());
			let over = self.winner().is_some();
			players[idx(turn)].update(board, turn);
			// Drawn before `pick_move`, so a computer's "thinking" panel is on screen while it searches
			terminal.draw(|frame| self.draw(frame, &players))?;

			let player = &mut *players[idx(turn)];
			let action = match (!over).then(|| player.pick_move(board, turn)).flatten()
			{
				// Input that came in during the search goes first, so a quit or a revert isn't
				// lost when both players are computers and nothing ever waits for input
				Some(m) => self.pending_action(player, board, turn)?.unwrap_or(Action::Play(m)),
				None => self.handle_event(event::read()?, player, board, turn, over),
			};

			match action
			{
				Action::None => (),
				Action::Quit => return Ok(self.winner()),
				Action::Play(m) => self.play(m),
				Action::Revert(i) =>
				{
					self.revert(i);
					players.iter_mut().for_each(|p| p.reset());
				}
			}
		}
	}

	/// Handle the events already waiting, without blocking, until one of them does something.
	fn pending_action(&self, player: &mut dyn Player, board: GameBoard, turn: Turn) -> io::Result<Option<Action>>
	{
		while event::poll(Duration::ZERO)?
		{
			let action = self.handle_event(event::read()?, player, board, turn, false);
			if !matches!(action, Action::None)
			{
				return Ok(Some(action));
			}
		}
		Ok(None)
	}

	fn handle_event(&self, ev: Event, player: &mut dyn Player, board: GameBoard, turn: Turn, over: bool) -> Action
	{
		match ev
		{
			Event::Key(key) if key.kind == KeyEventKind::Press =>
			{
				let ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
				if ctrl_c || matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
				{
					return Action::Quit;
				}
				if over
				{
					return Action::None;
				}
				player.handle_input(Input::Key(key), board, turn).map_or(Action::None, Action::Play)
			}
			Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) =>
			{
				let at = (mouse.column, mouse.row).into();
				if let Some(&(_, i)) = self.history_rows.iter().find(|(rect, _)| rect.contains(at))
				{
					return Action::Revert(i);
				}
				match pos_at(self.board_area, mouse.column, mouse.row)
				{
					Some(pos) if !over => player.handle_input(Input::Click(pos), board, turn).map_or(Action::None, Action::Play),
					_ => Action::None,
				}
			}
			_ => Action::None,
		}
	}

	fn draw(&mut self, frame: &mut Frame, players: &[&mut dyn Player; 2])
	{
		let (_, board_h) = BoardView::size(1);
		// Neighbouring panes overlap by a cell, so they share one border that `panel` merges into ┬ ┴ ┼
		let [top, bottom] = Layout::vertical([Constraint::Min(board_h + 4), Constraint::Length(6)]).spacing(-1).areas(frame.area());
		// History rows are like "123. ta5xc3  [↶]", so a fixed narrow pane fits them; the board gets the rest
		let [board_pane, history_pane] = Layout::horizontal([Constraint::Min(0), Constraint::Length(20)]).spacing(-1).areas(top);
		let [current_pane, last_pane] = Layout::horizontal([Constraint::Percentage(70), Constraint::Fill(1)]).spacing(-1).areas(bottom);

		let winner = self.winner();
		let turn = self.turn();
		let player = &*players[match turn { Turn::Sheep => 0, Turn::Tiger => 1 }];

		// The bottom row is drawn first, so the top row's bottom titles (keybinds, revert hint)
		// land on the shared border line over it. Its own titles go on its bottom border.
		// Player output, or the result once the game is over
		match winner
		{
			Some(w) =>
			{
				let block = panel().title_bottom(" Game over ");
				let text = vec![
					Line::styled(format!("{} win!", side_name(w)), Style::new().add_modifier(Modifier::BOLD)),
					Line::from("Click a move to revert to it, or press q to quit."),
				];
				frame.render_widget(Paragraph::new(text).block(block), current_pane);
			}
			None =>
			{
				let block = panel().title_bottom(format!(" {}: {} ", side_name(turn), player.name()));
				let inner = block.inner(current_pane);
				frame.render_widget(block, current_pane);
				frame.render_widget(player, inner);
			}
		}

		// The previous player's comments on the move it just made
		let last = turn.next_turn();
		let previous = &*players[match last { Turn::Sheep => 0, Turn::Tiger => 1 }];
		let title = if self.history.is_empty() { " Last turn ".to_string() }
			else
			{
				// Drop the name and shorten the label if the pane is too narrow for it
				let full = format!(" Last turn: {} ({}) ", side_name(last), previous.name());
				let fits = full.chars().count() as u16 + 2 <= last_pane.width;
				if fits { full } else { format!(" Last: {} ", side_name(last)) }
			};
		let block = panel().title_bottom(title);
		let inner = block.inner(last_pane);
		frame.render_widget(block, last_pane);
		if !self.history.is_empty()
		{
			previous.render_last_turn(inner, frame.buffer_mut());
		}

		// Board
		// Keep in step with `handle_event`
		let block = panel().title(" Board ").title_bottom(" click: select / move · q, Esc: quit ");
		let inner = block.inner(board_pane);
		frame.render_widget(block, board_pane);
		// Leave a cell of margin, then centre the biggest board that fits
		let room = inner.inner(Margin::new(1, 1));
		let (board_w, board_h) = BoardView::size(BoardView::scale_to_fit(room.width, room.height));
		let padded = room.centered(Constraint::Length(board_w), Constraint::Length(board_h));
		self.board_area = padded;
		let view = BoardView
		{
			board: self.board(),
			highlights: if winner.is_none() { player.highlights() } else { Default::default() },
			last_move: self.history.last().map(|&(m, _)| m),
		};
		frame.render_widget(&view, padded);

		// Move history, scrolled to keep the newest move in view
		let block = panel().title(" Moves ").title_bottom(" ↶ reverts ");
		let inner = block.inner(history_pane);
		frame.render_widget(block, history_pane);
		self.history_rows.clear();
		let first = self.history.len().saturating_sub(inner.height as usize);
		for (row, (i, (m, _))) in self.history.iter().enumerate().skip(first).enumerate()
		{
			let rect = Rect { y: inner.y + row as u16, height: 1, ..inner };
			let side = if i % 2 == 0 { Color::White } else { Color::Yellow };
			let line = Line::from(vec![
				Span::styled(format!("{:>3}. ", i + 1), Style::new().fg(Color::DarkGray)),
				Span::styled(format!("{:<8}", m.to_string()), Style::new().fg(side)),
				Span::styled("[↶]", Style::new().fg(Color::Cyan)),
			]);
			frame.render_widget(line, rect);
			self.history_rows.push((rect, i));
		}
	}

}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn turn_alternates_starting_with_sheep()
	{
		let mut game = GameHandler::default();
		assert_eq!(game.turn(), Turn::Sheep);
		game.play(Move::PlaceSheep { pos: (2, 2) });
		assert_eq!(game.turn(), Turn::Tiger);
		game.play(Move::MoveTiger { from: (0, 0), to: (1, 0) });
		assert_eq!(game.turn(), Turn::Sheep);
	}

	#[test]
	fn revert_goes_back_to_before_the_move()
	{
		let mut game = GameHandler::default();
		let start = game.board();
		game.play(Move::PlaceSheep { pos: (2, 2) });
		let after_first = game.board();
		game.play(Move::MoveTiger { from: (0, 0), to: (1, 0) });
		game.play(Move::PlaceSheep { pos: (3, 3) });

		game.revert(1);
		assert_eq!(game.board(), after_first);
		assert_eq!(game.turn(), Turn::Tiger);
		assert_eq!(game.history.len(), 1);

		game.revert(0);
		assert_eq!(game.board(), start);
		assert_eq!(game.turn(), Turn::Sheep);
	}

	#[test]
	#[should_panic]
	fn play_rejects_illegal_moves()
	{
		GameHandler::default().play(Move::PlaceSheep { pos: (0, 0) });
	}

	use ratatui::{backend::TestBackend, crossterm::event::{MouseEvent, KeyEvent}, Terminal};
	use crate::{claude_player::ClaudePlayer, local_player::LocalPlayer};

	fn draw(game: &mut GameHandler, players: &[&mut dyn Player; 2]) -> Terminal<TestBackend>
	{
		let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
		terminal.draw(|frame| game.draw(frame, players)).unwrap();
		terminal
	}

	fn click(column: u16, row: u16) -> Event
	{
		Event::Mouse(MouseEvent { kind: MouseEventKind::Down(MouseButton::Left), column, row, modifiers: KeyModifiers::NONE })
	}

	#[test]
	fn clicks_on_the_board_and_history()
	{
		let mut game = GameHandler::default();
		let (mut sheep, mut tiger) = (LocalPlayer::default(), LocalPlayer::default());
		sheep.update(game.board(), game.turn());
		let terminal = draw(&mut game, &[&mut sheep, &mut tiger]);
		let screen = format!("{:?}", terminal.backend().buffer());
		assert!(screen.contains("Sheep: You"), "{screen}");
		assert!(screen.contains("Click an empty point to place a sheep"), "{screen}");
		assert!(screen.contains("   a   b   c   d   e"), "{screen}");
		assert!(screen.contains("5  T───·───·───·───T"), "{screen}");

		// Click c3, the centre point
		let area = game.board_area;
		let action = game.handle_event(click(area.x + 3 + 8, area.y + 4), &mut sheep, game.board(), game.turn(), false);
		let Action::Play(m) = action else { panic!("no move") };
		assert_eq!(m, Move::PlaceSheep { pos: (2, 2) });
		game.play(m);

		let terminal = draw(&mut game, &[&mut sheep, &mut tiger]);
		let screen = format!("{:?}", terminal.backend().buffer());
		assert!(screen.contains("1. sc3     [↶]"), "{screen}");
		assert!(screen.contains("Tigers: You"), "{screen}");
		assert!(screen.contains("Last: Sheep"), "{screen}");
		assert!(screen.contains("Played sc3"), "{screen}");

		let (rect, i) = game.history_rows[0];
		let action = game.handle_event(click(rect.x + 2, rect.y), &mut tiger, game.board(), game.turn(), false);
		assert!(matches!(action, Action::Revert(0)) && i == 0);

		let q = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
		assert!(matches!(game.handle_event(q, &mut tiger, game.board(), game.turn(), false), Action::Quit));
	}

	#[test]
	fn board_grows_in_a_big_terminal()
	{
		let mut game = GameHandler::default();
		game.play(Move::PlaceSheep { pos: (2, 2) });
		let (mut sheep, mut tiger) = (LocalPlayer::default(), LocalPlayer::default());
		let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
		terminal.draw(|frame| game.draw(frame, &[&mut sheep, &mut tiger])).unwrap();
		let area = game.board_area;
		assert!(area.height > BoardView::size(1).1, "{area:?}");
		let (w, h) = (area.width, area.height);
		assert_eq!(BoardView::size(BoardView::scale_to_fit(w, h)), (w, h));

		// Find the sheep on screen and click it
		let buf = terminal.backend().buffer();
		let at = (area.left()..area.right()).flat_map(|x| (area.top()..area.bottom()).map(move |y| (x, y)))
			.find(|&(x, y)| buf[(x, y)].symbol() == "S").expect("sheep drawn");
		assert_eq!(pos_at(area, at.0, at.1), Some((2, 2)));
	}

	#[test]
	fn last_turn_panel_shows_the_computer_search()
	{
		let mut game = GameHandler::default();
		let mut sheep = LocalPlayer::default();
		let mut tiger = ClaudePlayer::new(std::time::Duration::from_millis(50));
		game.play(Move::PlaceSheep { pos: (2, 2) });
		let m = tiger.pick_move(game.board(), game.turn()).unwrap();
		game.play(m);

		let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
		terminal.draw(|frame| game.draw(frame, &[&mut sheep, &mut tiger])).unwrap();
		let screen = format!("{:?}", terminal.backend().buffer());
		assert!(screen.contains("Last turn: Tigers (Claude)"), "{screen}");
		assert!(screen.contains(&format!("Played {m}")), "{screen}");
		assert!(screen.contains("Searched to depth"), "{screen}");
		assert!(screen.contains("Score: "), "{screen}");

		// Borders are shared and joined where panes meet, and every pane reaches the right edge
		let buf = terminal.backend().buffer();
		let row = |y: u16| (0..100).map(|x| buf[(x, y)].symbol()).collect::<String>();
		assert!(row(0).starts_with("┌ Board ─") && row(0).ends_with("┐"), "{}", row(0));
		assert!(row(0).contains("┬ Moves "), "{}", row(0));
		assert_eq!(row(18).chars().next(), Some('├'), "{}", row(18));
		assert!(row(18).contains('┴') && row(18).contains('┬') && row(18).ends_with("┤"), "{}", row(18));
		assert!(row(18).contains(" click: select / move · q, Esc: quit "), "{}", row(18));
		assert!(row(23).starts_with("└ Sheep: You ─") && row(23).contains('┴') && row(23).ends_with("┘"), "{}", row(23));
	}

	#[test]
	fn no_winner_at_the_start()
	{
		assert_eq!(GameHandler::default().winner(), None);
	}
}
