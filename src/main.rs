use std::time::Duration;

use crate::{claude_player::ClaudePlayer, game::GameHandler, local_player::LocalPlayer};

mod board;
mod player;
mod game;
#[path = "local-player.rs"]
mod local_player;
#[path = "claude-player.rs"]
mod claude_player;

fn main() {
	let mut game = GameHandler::default();
	// I play sheep, claude plays tiger
	let winner = game.play_game(&mut LocalPlayer, &mut ClaudePlayer::new(Duration::from_secs(2)));
	println!("{winner:?} wins!");
}
