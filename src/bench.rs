use std::time::{Duration, Instant};

use crate::{board::{GameBoard, Turn}, claude_player::ClaudePlayer, player::{MinimaxPlayer, Player, SearchReport, SearchStats}};

/// Positions from each phase of the game, as (label, plies of random play from the start).
const POSITIONS: [(&str, usize); 6] = [
	("opening", 0),
	("tiger reply", 1),
	("mid-placement", 20),
	("end of placement", 38),
	("early movement", 45),
	("movement", 60),
];

/// The position after `plies` moves of `ClaudePlayer` playing itself to depth 4, which is
/// deterministic and plays a lot more like a real game than random moves do.
fn find_position(plies: usize) -> (GameBoard, Turn)
{
	let mut player = ClaudePlayer::new(Duration::from_secs(1 << 20));
	player.max_depth = 4;
	let mut board = GameBoard::default();
	let mut turn = Turn::Sheep;
	for _ in 0..plies
	{
		assert!(!board.check_win(turn), "the self-play game ended before ply {plies}");
		board.make_move_unchecked(player.pick_move(board, turn).expect("a move"));
		turn = turn.next_turn();
	}
	(board, turn)
}

/// Search each of `POSITIONS` to `depth` with a fresh `ClaudePlayer` and print what it found and how long it took.
/// The depth caps the search instead of the time limit, so runs do the same work and can be compared.
/// Build with `--features search-stats` to also print node counts (which slows the search a little).
pub fn run(depth: usize)
{
	println!("{:<17} {:<6} {:>5} {:>9} {:<8} {:>8}", "position", "turn", "depth", "time ms", "move", "score");
	let mut total_time = Duration::ZERO;
	let mut total_stats = SearchStats::default();
	for (label, plies) in POSITIONS
	{
		let (board, turn) = find_position(plies);
		let mut player = ClaudePlayer::new(Duration::from_secs(1 << 20));
		player.max_depth = depth;
		SearchStats::take();
		let start = Instant::now();
		let m = player.pick_move(board, turn).expect("a move");
		let time = start.elapsed();
		let stats = SearchStats::take();
		total_time += time;
		add(&mut total_stats, &stats);

		let SearchReport { mv, depth, score } = player.last_search().expect("a report");
		assert_eq!(mv, m);
		println!("{label:<17} {:<6} {depth:>5} {:>9.1} {:<8} {score:>8.1}",
			format!("{turn:?}"), time.as_secs_f64() * 1000.0, mv.to_string());
		print_stats(&stats, time);
	}
	println!("{:<17} {:<6} {:>5} {:>9.1}", "total", "", "", total_time.as_secs_f64() * 1000.0);
	print_stats(&total_stats, total_time);
}

fn add(total: &mut SearchStats, s: &SearchStats)
{
	total.nodes += s.nodes;
	total.leaves += s.leaves;
	total.terminal += s.terminal;
	total.cache_probes += s.cache_probes;
	total.cache_hits += s.cache_hits;
	total.cache_returns += s.cache_returns;
	total.cache_overwrites += s.cache_overwrites;
	total.cutoffs += s.cutoffs;
	total.first_move_cutoffs += s.first_move_cutoffs;
	total.move_gens += s.move_gens;
}

fn print_stats(s: &SearchStats, time: Duration)
{
	if !cfg!(feature = "search-stats")
	{
		return;
	}
	let pct = |a: u64, b: u64| if b == 0 { 0.0 } else { 100.0 * a as f64 / b as f64 };
	println!("    {} nodes ({:.2} M/s), {} leaves, {} terminal, {:.2} move gens/node",
		s.nodes, s.nodes as f64 / time.as_secs_f64() / 1e6, s.leaves, s.terminal,
		s.move_gens as f64 / s.nodes.max(1) as f64);
	println!("    cache: {} probes, {:.1}% hit, {:.1}% of hits settle the score, {} overwrites; cutoffs: {}, {:.1}% on the first move",
		s.cache_probes, pct(s.cache_hits, s.cache_probes), pct(s.cache_returns, s.cache_hits), s.cache_overwrites,
		s.cutoffs, pct(s.first_move_cutoffs, s.cutoffs));
}
