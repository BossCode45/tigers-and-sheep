use std::{path::Path, time::Duration};

use crate::{board::Turn, claude_player::ClaudePlayer, game::GameHandler, local_player::LocalPlayer, network::Network, neural_player::NeuralPlayer, player::Player};

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
mod network;
#[path = "neural-player.rs"]
mod neural_player;
mod train;

fn main() -> std::io::Result<()> {
	let mut args: Vec<String> = std::env::args().collect();
	// `--from <weights>` can go anywhere after `train`
	let from = args.iter().position(|a| a == "--from").map(|i|
	{
		let path = args.get(i + 1).cloned().expect("usage: --from <weights file>");
		args.drain(i..=i + 1);
		path
	});
	let arg = |i: usize| args.get(i).map(String::as_str);
	let number = |i: usize, default: usize| arg(i).map_or(default, |n| n.parse().expect("expected a number"));
	match arg(1)
	{
		// `bench [depth]` times the computer player's search without the TUI
		Some("bench") =>
		{
			bench::run(number(2, 6));
			return Ok(());
		}
		// `train [generations] [games per generation] [out dir] [--from <weights>]` trains a network
		// by self-play, starting from `--from`, or the newest generation in the out dir, or random weights
		Some("train") =>
		{
			let defaults = train::Config::default();
			let cfg = train::Config
			{
				generations: number(2, defaults.generations),
				games_per_generation: number(3, defaults.games_per_generation),
				out_dir: arg(4).map_or(defaults.out_dir.clone(), Into::into),
				from: from.map(Into::into),
				..defaults
			};
			return train::run(&cfg);
		}
		// `neural <weights> [sheep|tiger]`: you play the given side against a trained network
		Some("neural") =>
		{
			let net = Network::load(Path::new(arg(2).expect("usage: neural <weights file> [sheep|tiger]")))?;
			let side = match arg(3) { Some("tiger") => Turn::Tiger, _ => Turn::Sheep };
			let mut computer = NeuralPlayer::new(net, Duration::from_secs(2), 20);
			let mut human = LocalPlayer::default();
			let (sheep, tiger): (&mut dyn Player, &mut dyn Player) = match side
			{
				Turn::Sheep => (&mut human, &mut computer),
				Turn::Tiger => (&mut computer, &mut human),
			};
			return play(sheep, tiger);
		}
		_ => (),
	}
	play(&mut ClaudePlayer::new(Duration::from_secs(2)), &mut ClaudePlayer::new(Duration::from_secs(2)))
}

fn play(sheep: &mut dyn Player, tiger: &mut dyn Player) -> std::io::Result<()>
{
	let mut game = GameHandler::default();
	if let Some(winner) = game.play_game(sheep, tiger)?
	{
		println!("{winner:?} wins!");
	}
	Ok(())
}
