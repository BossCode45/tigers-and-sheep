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

fn main() -> std::io::Result<()> {
	let mut game = GameHandler::default();
	let winner = game.play_game(&mut ClaudePlayer::new(Duration::from_secs(2)), &mut LocalPlayer::default())?;
	if let Some(winner) = winner
	{
		println!("{winner:?} wins!");
	}
	Ok(())
}
