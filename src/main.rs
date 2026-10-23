use std::time::Duration;

use crate::{claude_player::ClaudePlayer, game::GameHandler, local_player::LocalPlayer};

mod board;
mod player;
mod game;
#[path = "board-view.rs"]
mod board_view;
#[path = "local-player.rs"]
mod local_player;
#[path = "claude-player.rs"]
mod claude_player;
mod bench;

fn main() -> std::io::Result<()> {
	// `bench [depth]` times the computer player's search without the TUI
	let args: Vec<String> = std::env::args().collect();
	if args.get(1).is_some_and(|a| a == "bench")
	{
		let depth = args.get(2).map_or(6, |d| d.parse().expect("depth should be a number"));
		bench::run(depth);
		return Ok(());
	}
	let mut game = GameHandler::default();
	let winner = game.play_game(&mut ClaudePlayer::new(Duration::from_secs(2)),
								&mut ClaudePlayer::new(Duration::from_secs(2)))?;
	if let Some(winner) = winner
	{
		println!("{winner:?} wins!");
	}
	Ok(())
}
