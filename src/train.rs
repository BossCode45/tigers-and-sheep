use std::{collections::VecDeque, fs, path::{Path, PathBuf}, sync::atomic::{AtomicUsize, Ordering}, time::{Duration, Instant}};

use crate::{board::{GameBoard, Move, Turn, SYMMETRIES}, claude_player::ClaudePlayer, network::{Adam, Network, Rng, Sample}, neural_player::NeuralPlayer, player::{MinimaxPlayer, Player}};

/// Settings for `run`.
#[derive(Clone, Debug)]
pub struct Config
{
	pub generations: usize,
	pub games_per_generation: usize,
	pub out_dir: PathBuf,
	/// Search depth for every move in self-play and matches. The depth cap, not a time limit,
	/// ends each search, so runs are repeatable.
	pub depth: usize,
	/// A game this long is called a draw, since nothing in the rules ends a game that goes round in circles.
	pub ply_cap: usize,
	/// Self-play picks moves at random for this many plies, so games start in different places.
	pub random_plies: usize,
	/// After that, the chance of a random move instead of the searched one.
	pub epsilon: f32,
	/// TD(λ): 1 trains every position on its game's result, smaller trusts later positions' search scores more.
	pub lambda: f32,
	/// Train on the samples of this many of the latest generations.
	pub buffer_generations: usize,
	pub epochs: usize,
	pub batch_size: usize,
	pub learning_rate: f32,
	/// Games against `ClaudePlayer`, and against the previous generation, after each generation (half as each side).
	pub match_games: usize,
}

impl Default for Config
{
	fn default() -> Self
	{
		Self
		{
			generations: 50,
			games_per_generation: 500,
			out_dir: PathBuf::from("weights"),
			depth: 3,
			ply_cap: 200,
			random_plies: 10,
			epsilon: 0.05,
			lambda: 0.7,
			buffer_generations: 5,
			epochs: 2,
			batch_size: 256,
			learning_rate: 1e-3,
			match_games: 100,
		}
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome
{
	Tiger,
	Sheep,
	Draw,
}

impl Outcome
{
	/// +1 for a tiger win, -1 for a sheep win, matching the sign of the scores.
	fn value(self) -> f32
	{
		match self { Outcome::Tiger => 1.0, Outcome::Sheep => -1.0, Outcome::Draw => 0.0 }
	}
}

/// A position from a game, with the searching player's score for it (clamped to -1..1).
#[derive(Clone, Copy, Debug)]
struct Ply
{
	board: GameBoard,
	turn: Turn,
	score: f32,
	/// A random move was played here instead of the searched one.
	random: bool,
}

/// Plays one game from the start. `choose` gives the move and score for a position; for the
/// first `random_plies` plies, and with chance `epsilon` after, a random move is played instead
/// (the position keeps `choose`'s score, and is marked `random` unless the two moves are the same).
fn play(mut choose: impl FnMut(GameBoard, Turn) -> (Move, f32), ply_cap: usize, random_plies: usize, epsilon: f32, rng: &mut Rng)
	-> (Outcome, Vec<Ply>)
{
	let mut board = GameBoard::default();
	let mut turn = Turn::Sheep;
	let mut plies = Vec::new();
	while plies.len() < ply_cap
	{
		if board.check_win(turn)
		{
			// The side that just moved has won
			let winner = match turn { Turn::Sheep => Outcome::Tiger, Turn::Tiger => Outcome::Sheep };
			return (winner, plies);
		}
		let (searched, score) = choose(board, turn);
		let mut m = searched;
		if plies.len() < random_plies || rng.next_f32() < epsilon
		{
			let moves = board.get_moves(turn);
			m = moves[rng.below(moves.len())];
		}
		plies.push(Ply { board, turn, score: score.clamp(-1.0, 1.0), random: m != searched });
		board.make_move_unchecked(m);
		turn = turn.next_turn();
	}
	(Outcome::Draw, plies)
}

/// TD(λ) targets, worked backwards: the last position's target is the result, and each earlier
/// one's is `(1 - λ)` times the next position's score plus `λ` times the next position's target.
///
/// A position where a random move was played is instead labelled with its own search score, which
/// also cuts the chain there, so earlier positions don't learn from the random move's result.
/// Without the cut, the sheep's random moves (which usually leave a sheep to be jumped, where a
/// tiger's random move costs little) taught the net that tigers nearly always win.
fn td_targets(plies: &[Ply], outcome: Outcome, lambda: f32) -> Vec<f32>
{
	let mut targets = vec![0.0; plies.len()];
	let mut next = outcome.value();
	for t in (0..plies.len()).rev()
	{
		targets[t] = if plies[t].random { plies[t].score } else { next };
		next = (1.0 - lambda) * plies[t].score + lambda * targets[t];
	}
	targets
}

/// Runs `f(0..n)` across all cores, returning the results in order.
fn parallel<T: Send>(n: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T>
{
	let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(n.max(1));
	let next = AtomicUsize::new(0);
	let mut results: Vec<(usize, T)> = std::thread::scope(|s|
	{
		let handles: Vec<_> = (0..threads).map(|_| s.spawn(||
		{
			let mut done = Vec::new();
			loop
			{
				let i = next.fetch_add(1, Ordering::Relaxed);
				if i >= n
				{
					return done;
				}
				done.push((i, f(i)));
			}
		})).collect();
		handles.into_iter().flat_map(|h| h.join().expect("worker thread panicked")).collect()
	});
	results.sort_by_key(|(i, _)| *i);
	results.into_iter().map(|(_, r)| r).collect()
}

/// A player that searches exactly `depth` plies.
fn neural(net: &Network, depth: usize) -> NeuralPlayer
{
	let mut player = NeuralPlayer::new(net.clone(), Duration::from_secs(1 << 20), 16);
	player.max_depth = depth;
	player
}

fn claude(depth: usize) -> ClaudePlayer
{
	let mut player = ClaudePlayer::new(Duration::from_secs(1 << 20));
	player.max_depth = depth;
	player
}

/// The move a minimax player picks, and the score it gave it.
fn search(player: &mut (impl MinimaxPlayer + Player), board: GameBoard, turn: Turn) -> (Move, f32)
{
	let m = player.pick_move(board, turn).expect("the game is not over, so there is a move");
	(m, player.last_search().map_or(0.0, |r| r.score))
}

/// One self-play game of `net` against itself, as training samples.
fn self_play(net: &Network, cfg: &Config, seed: u64) -> (Outcome, Vec<Sample>)
{
	let mut player = neural(net, cfg.depth);
	let mut rng = Rng::new(seed);
	let (outcome, plies) = play(|b, t| search(&mut player, b, t), cfg.ply_cap, cfg.random_plies, cfg.epsilon, &mut rng);
	let targets = td_targets(&plies, outcome, cfg.lambda);
	let samples = plies.iter().zip(targets).map(|(p, target)| Sample { board: p.board, turn: p.turn, target }).collect();
	(outcome, samples)
}

/// The share of points `net` scores over `games` against `opponent()` (a win is 1, a draw ½),
/// playing sheep in even games and tigers in odd ones. Each pair of games shares a seed, so both
/// sides get the same few random opening moves.
fn match_score<P: MinimaxPlayer + Player>(net: &Network, opponent: impl Fn() -> P + Sync, cfg: &Config, seed: u64) -> f32
{
	let points: f32 = parallel(cfg.match_games, |i|
	{
		let mut me = neural(net, cfg.depth);
		let mut them = opponent();
		let neural_side = if i % 2 == 0 { Turn::Sheep } else { Turn::Tiger };
		let mut rng = Rng::new(seed ^ (i / 2) as u64);
		let (outcome, _) = play(|b, t| if t == neural_side { search(&mut me, b, t) } else { search(&mut them, b, t) },
			cfg.ply_cap, 4, 0.0, &mut rng);
		match (outcome, neural_side)
		{
			(Outcome::Draw, _) => 0.5,
			(Outcome::Tiger, Turn::Tiger) | (Outcome::Sheep, Turn::Sheep) => 1.0,
			_ => 0.0,
		}
	}).into_iter().sum();
	points / cfg.match_games.max(1) as f32
}

/// The newest `gen-NNN.bin` in `dir`, as (generation, path).
fn latest_generation(dir: &Path) -> Option<(usize, PathBuf)>
{
	fs::read_dir(dir).ok()?
		.filter_map(|e| e.ok())
		.filter_map(|e| {
			let name = e.file_name().into_string().ok()?;
			let n = name.strip_prefix("gen-")?.strip_suffix(".bin")?.parse().ok()?;
			Some((n, e.path()))
		})
		.max_by_key(|(n, _)| *n)
}

/// Trains a network by self-play, saving each generation to `out_dir/gen-NNN.bin` and `out_dir/latest.bin`.
/// Carries on from the newest generation already in `out_dir`, if there is one.
pub fn run(cfg: &Config) -> std::io::Result<()>
{
	fs::create_dir_all(&cfg.out_dir)?;
	let (first, mut net) = match latest_generation(&cfg.out_dir)
	{
		Some((n, path)) =>
		{
			println!("Carrying on from {}", path.display());
			(n + 1, Network::load(&path)?)
		}
		None => (1, Network::random(0x5EED)),
	};
	let mut adam = Adam::new(cfg.learning_rate);
	let mut buffer: VecDeque<Vec<Sample>> = VecDeque::new();
	let mut rng = Rng::new(first as u64);
	println!("{:>4} {:>7} {:>6} {:>6} {:>6} {:>8} {:>8} {:>7} {:>7} {:>7}",
		"gen", "samples", "tigers", "sheep", "draws", "loss", "after", "vs prev", "vs Claude", "time s");
	for generation in first..first + cfg.generations
	{
		let start = Instant::now();
		let seed_base = (generation as u64) << 32;
		let games = parallel(cfg.games_per_generation, |i| self_play(&net, cfg, seed_base | i as u64));
		let count = |o: Outcome| games.iter().filter(|(g, _)| *g == o).count();
		let (tigers, sheep, draws) = (count(Outcome::Tiger), count(Outcome::Sheep), count(Outcome::Draw));
		buffer.push_back(games.into_iter().flat_map(|(_, s)| s).collect());
		while buffer.len() > cfg.buffer_generations
		{
			buffer.pop_front();
		}

		let mut samples: Vec<Sample> = buffer.iter().flatten().copied().collect();
		let loss_before = net.loss(&samples);
		let previous = net.clone();
		for _ in 0..cfg.epochs
		{
			// Shuffle, and show each position in a random one of its 8 orientations
			for i in (1..samples.len()).rev()
			{
				samples.swap(i, rng.below(i + 1));
			}
			for s in &mut samples
			{
				s.board = s.board.transformed(rng.below(SYMMETRIES));
			}
			for batch in samples.chunks(cfg.batch_size)
			{
				net.train_batch(batch, &mut adam);
			}
		}
		let loss_after = net.loss(&samples);

		let vs_prev = match_score(&net, || neural(&previous, cfg.depth), cfg, seed_base ^ 0xA);
		let vs_claude = match_score(&net, || claude(cfg.depth), cfg, seed_base ^ 0xB);
		net.save(&cfg.out_dir.join(format!("gen-{generation:03}.bin")))?;
		net.save(&cfg.out_dir.join("latest.bin"))?;
		println!("{generation:>4} {:>7} {tigers:>6} {sheep:>6} {draws:>6} {loss_before:>8.4} {loss_after:>8.4} {:>6.0}% {:>8.0}% {:>7.1}",
			samples.len(), vs_prev * 100.0, vs_claude * 100.0, start.elapsed().as_secs_f64());
	}
	Ok(())
}

#[cfg(test)]
mod tests
{
	use super::*;

	fn ply(score: f32) -> Ply
	{
		Ply { board: GameBoard::default(), turn: Turn::Sheep, score, random: false }
	}

	#[test]
	fn td_targets_blend_scores_and_the_result()
	{
		let plies = [ply(0.0), ply(0.2), ply(-0.4)];
		let t = td_targets(&plies, Outcome::Tiger, 0.5);
		assert_eq!(t[2], 1.0);
		assert_eq!(t[1], 0.5 * -0.4 + 0.5 * 1.0);
		assert_eq!(t[0], 0.5 * 0.2 + 0.5 * t[1]);
		// λ = 1 is just the result
		assert_eq!(td_targets(&plies, Outcome::Sheep, 1.0), vec![-1.0; 3]);
	}

	#[test]
	fn td_targets_stop_at_random_moves()
	{
		let random = Ply { random: true, ..ply(0.3) };
		let plies = [ply(0.0), random, ply(-0.4)];
		let t = td_targets(&plies, Outcome::Tiger, 1.0);
		// After the random move the result counts; at it and before it, only its search score does
		assert_eq!(t, vec![0.3, 0.3, 1.0]);
		let t = td_targets(&plies, Outcome::Tiger, 0.5);
		assert_eq!(t[1], 0.3);
		assert_eq!(t[0], 0.3);
	}

	#[test]
	fn self_play_games_are_legal_and_repeatable()
	{
		let cfg = Config { depth: 1, ply_cap: 60, ..Config::default() };
		let net = Network::random(3);
		let (outcome, samples) = self_play(&net, &cfg, 42);
		assert_eq!(self_play(&net, &cfg, 42), (outcome, samples.clone()));
		assert!(!samples.is_empty() && samples.len() <= cfg.ply_cap);
		assert_eq!(samples[0].board, GameBoard::default());
		// Each position follows from the one before by a legal move
		for w in samples.windows(2)
		{
			assert_eq!(w[1].turn, w[0].turn.next_turn());
			assert!(w[0].board.get_moves(w[0].turn).iter().any(|&m| w[0].board.after_move_unchecked(m) == w[1].board));
		}
		assert!(samples.iter().all(|s| (-1.0..=1.0).contains(&s.target)));
		if outcome == Outcome::Draw
		{
			assert_eq!(samples.len(), cfg.ply_cap);
		}
	}

	#[test]
	fn parallel_keeps_the_order()
	{
		assert_eq!(parallel(100, |i| i * 2), (0..100).map(|i| i * 2).collect::<Vec<_>>());
	}
}
