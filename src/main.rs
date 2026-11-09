use std::{path::PathBuf, time::Duration};

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::{board::Outcome, claude_player::ClaudePlayer, game::GameHandler, local_player::LocalPlayer, network::Network, neural_player::NeuralPlayer, player::Player};

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

/// Tigers and Sheep, a hunt game on a 5x5 board. With no command, plays a game in the terminal.
#[derive(Parser)]
#[command(version, args_conflicts_with_subcommands = true)]
struct Cli
{
	#[command(subcommand)]
	command: Option<Command>,
	#[command(flatten)]
	play: PlayArgs,
}

#[derive(Subcommand)]
enum Command
{
	/// Play a game in the terminal (the default)
	Play(PlayArgs),
	/// Play against a trained network
	Neural
	{
		/// Weights file written by `train`
		weights: PathBuf,
		/// The side you play
		#[arg(value_enum, default_value_t = Side::Sheep)]
		side: Side,
		/// Seconds the network may think per move
		#[arg(short, long, default_value_t = 2.0)]
		time: f64,
	},
	/// Time the computer player's search on fixed positions, without the TUI
	Bench
	{
		/// Search depth
		#[arg(default_value_t = 6)]
		depth: usize,
	},
	/// Train a network by self-play
	///
	/// Starts from `--from`, or else the newest gen-NNN.bin in the out dir, or else random weights.
	Train(TrainArgs),
}

#[derive(Args)]
struct PlayArgs
{
	/// Who plays the sheep: `human`, `claude`, or a weights file for a neural player
	#[arg(short = 'S', long, default_value = "claude", value_parser = parse_player)]
	sheep: PlayerKind,
	/// Who plays the tigers: `human`, `claude`, or a weights file for a neural player
	#[arg(short = 'T', long, default_value = "claude", value_parser = parse_player)]
	tiger: PlayerKind,
	/// Seconds each computer player may think per move
	#[arg(short, long, default_value_t = 2.0)]
	time: f64,
}

#[derive(Clone)]
enum PlayerKind
{
	Human,
	Claude,
	Neural(PathBuf),
}

fn parse_player(s: &str) -> Result<PlayerKind, String>
{
	match s
	{
		"human" => Ok(PlayerKind::Human),
		"claude" => Ok(PlayerKind::Claude),
		path if PathBuf::from(path).is_file() => Ok(PlayerKind::Neural(path.into())),
		_ => Err(format!("expected `human`, `claude` or a weights file, but `{s}` is none of these")),
	}
}

#[derive(Clone, Copy, ValueEnum)]
enum Side
{
	Sheep,
	Tiger,
}

#[derive(Args)]
struct TrainArgs
{
	/// Generations to train
	#[arg(default_value_t = train::Config::default().generations)]
	generations: usize,
	/// Self-play games per generation
	#[arg(default_value_t = train::Config::default().games_per_generation)]
	games: usize,
	/// Where each generation's weights are saved
	#[arg(default_value = "weights")]
	out_dir: PathBuf,
	/// Start from this weights file
	#[arg(long)]
	from: Option<PathBuf>,
	/// Search depth for every move in self-play and matches
	#[arg(long, default_value_t = train::Config::default().depth)]
	depth: usize,
	/// A game this many plies long is a draw
	#[arg(long, default_value_t = train::Config::default().ply_cap)]
	ply_cap: usize,
	/// Plies of random moves at the start of each self-play game
	#[arg(long, default_value_t = train::Config::default().random_plies)]
	random_plies: usize,
	/// Chance of a random move after that
	#[arg(long, default_value_t = train::Config::default().epsilon)]
	epsilon: f32,
	/// TD(λ): 1 trains on each game's result alone, smaller trusts later search scores more
	#[arg(long, default_value_t = train::Config::default().lambda)]
	lambda: f32,
	/// Train on the samples of this many of the latest generations
	#[arg(long, default_value_t = train::Config::default().buffer_generations)]
	buffer_generations: usize,
	/// Don't balance positions from sheep-win and tiger-win games
	#[arg(long)]
	no_balance: bool,
	/// Passes over the samples per generation
	#[arg(long, default_value_t = train::Config::default().epochs)]
	epochs: usize,
	/// Positions per optimiser step
	#[arg(long, default_value_t = train::Config::default().batch_size)]
	batch_size: usize,
	/// Adam's learning rate
	#[arg(long, default_value_t = train::Config::default().learning_rate)]
	learning_rate: f32,
	/// Games against ClaudePlayer, and against the generation that last played them
	#[arg(long, default_value_t = train::Config::default().match_games)]
	match_games: usize,
	/// Play those matches only every this many generations
	#[arg(long, default_value_t = train::Config::default().match_every)]
	match_every: usize,
}

impl From<TrainArgs> for train::Config
{
	fn from(args: TrainArgs) -> Self
	{
		Self
		{
			generations: args.generations,
			games_per_generation: args.games,
			out_dir: args.out_dir,
			from: args.from,
			depth: args.depth,
			ply_cap: args.ply_cap,
			random_plies: args.random_plies,
			epsilon: args.epsilon,
			lambda: args.lambda,
			buffer_generations: args.buffer_generations,
			balance_outcomes: !args.no_balance,
			epochs: args.epochs,
			batch_size: args.batch_size,
			learning_rate: args.learning_rate,
			match_games: args.match_games,
			match_every: args.match_every,
		}
	}
}

fn main() -> std::io::Result<()>
{
	let cli = Cli::parse();
	match cli.command
	{
		Some(Command::Bench { depth }) =>
		{
			bench::run(depth);
			Ok(())
		}
		Some(Command::Train(args)) => train::run(&args.into()),
		Some(Command::Neural { weights, side, time }) =>
		{
			let human = PlayerKind::Human;
			let computer = PlayerKind::Neural(weights);
			let (sheep, tiger) = match side
			{
				Side::Sheep => (human, computer),
				Side::Tiger => (computer, human),
			};
			play(PlayArgs { sheep, tiger, time })
		}
		Some(Command::Play(args)) => play(args),
		None => play(cli.play),
	}
}

fn make_player(kind: PlayerKind, time_limit: Duration) -> std::io::Result<Box<dyn Player>>
{
	Ok(match kind
	{
		PlayerKind::Human => Box::new(LocalPlayer::default()),
		PlayerKind::Claude => Box::new(ClaudePlayer::new(time_limit)),
		PlayerKind::Neural(path) => Box::new(NeuralPlayer::new(Network::load(&path)?, time_limit, 20)),
	})
}

fn play(args: PlayArgs) -> std::io::Result<()>
{
	let time_limit = Duration::from_secs_f64(args.time);
	let mut sheep = make_player(args.sheep, time_limit)?;
	let mut tiger = make_player(args.tiger, time_limit)?;
	let mut game = GameHandler::default();
	match game.play_game(sheep.as_mut(), tiger.as_mut())?
	{
		Some(Outcome::Tiger) => println!("Tigers win!"),
		Some(Outcome::Sheep) => println!("Sheep win!"),
		Some(Outcome::Draw) => println!("Draw by repetition."),
		None => (),
	}
	Ok(())
}
