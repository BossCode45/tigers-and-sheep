use std::{fs, io, path::Path};

use crate::board::{GameBoard, Turn};

/// Inputs: a bit per point for tigers (0..25) and for sheep (25..50), the sheep in hand over 20
/// (`HAND`) and 1 if tigers are to move (`TIGER_TO_MOVE`). Nothing else, so the network has to
/// work out for itself what makes a position good.
pub const INPUTS: usize = 52;
const HAND: usize = 50;
const TIGER_TO_MOVE: usize = 51;
pub const HIDDEN1: usize = 64;
pub const HIDDEN2: usize = 32;

// All the weights live in one flat Vec, at these offsets, so the optimiser, the gradient and
// save/load can treat them as a single list of numbers.
/// Layer 1 weights, input-major: input i's weight to hidden unit j is at `W1 + i*HIDDEN1 + j`,
/// so an input's weights are contiguous and adding up the set bits' columns is cheap.
const W1: usize = 0;
const B1: usize = W1 + INPUTS * HIDDEN1;
/// Layer 2 weights, unit-major: hidden1 unit j's weight to hidden2 unit k is at `W2 + k*HIDDEN1 + j`.
const W2: usize = B1 + HIDDEN1;
const B2: usize = W2 + HIDDEN1 * HIDDEN2;
const W3: usize = B2 + HIDDEN2;
const B3: usize = W3 + HIDDEN2;
pub const PARAMS: usize = B3 + 1;

const MAGIC: &[u8; 4] = b"TSNN";
const VERSION: u32 = 1;

/// A small, fast pseudo-random generator (xorshift64*), good enough for weight init and exploration.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng
{
	pub fn new(seed: u64) -> Self
	{
		// Zero is a fixed point of xorshift, and nearby seeds should still diverge quickly
		Self(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1)
	}

	pub fn next_u64(&mut self) -> u64
	{
		self.0 ^= self.0 >> 12;
		self.0 ^= self.0 << 25;
		self.0 ^= self.0 >> 27;
		self.0.wrapping_mul(0x2545F4914F6CDD1D)
	}

	/// Uniform in `0.0..1.0`.
	pub fn next_f32(&mut self) -> f32
	{
		(self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
	}

	/// Uniform in `0..n`.
	pub fn below(&mut self, n: usize) -> usize
	{
		(((self.next_u64() >> 32) * n as u64) >> 32) as usize
	}

	/// Standard normal, by Box-Muller.
	pub fn normal(&mut self) -> f32
	{
		let u = 1.0 - self.next_f32();
		let v = self.next_f32();
		(-2.0 * u.ln()).sqrt() * (std::f32::consts::TAU * v).cos()
	}
}

/// A position and the score the network should learn to give it (+1 tigers win, -1 sheep win).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample
{
	pub board: GameBoard,
	pub turn: Turn,
	pub target: f32,
}

/// Values of every layer for one position, kept by `forward_full` for backprop.
struct Activations
{
	h1: [f32; HIDDEN1],
	h2: [f32; HIDDEN2],
	out: f32,
}

/// A position scorer: inputs → 64 ReLU → 32 ReLU → tanh, positive favouring tigers.
#[derive(Clone, Debug, PartialEq)]
pub struct Network
{
	params: Vec<f32>,
}

/// Call `f` with the index of each input that is 1, and return the value of the `HAND` input.
#[inline(always)]
fn for_active(board: &GameBoard, turn: Turn, mut f: impl FnMut(usize)) -> f32
{
	let mut tigers = board.tiger_points();
	while tigers != 0
	{
		f(tigers.trailing_zeros() as usize);
		tigers &= tigers - 1;
	}
	let mut sheep = board.sheep_points();
	while sheep != 0
	{
		f(25 + sheep.trailing_zeros() as usize);
		sheep &= sheep - 1;
	}
	if turn == Turn::Tiger
	{
		f(TIGER_TO_MOVE);
	}
	board.sheep_left() as f32 / 20.0
}

impl Network
{
	/// Random weights (He initialisation), so a new network plays roughly at random.
	pub fn random(seed: u64) -> Self
	{
		let mut rng = Rng::new(seed);
		let mut params = vec![0.0; PARAMS];
		let mut init = |range: std::ops::Range<usize>, fan_in: usize, params: &mut [f32]|
		{
			let scale = (2.0 / fan_in as f32).sqrt();
			for p in &mut params[range]
			{
				*p = rng.normal() * scale;
			}
		};
		// About a third of the inputs are set at once, so scale layer 1 by that rather than all 52
		init(W1..B1, 20, &mut params);
		init(W2..B2, HIDDEN1, &mut params);
		init(W3..B3, HIDDEN2, &mut params);
		Self { params }
	}

	fn forward_full(&self, board: &GameBoard, turn: Turn) -> Activations
	{
		let p = &self.params;
		let mut h1 = [0.0; HIDDEN1];
		h1.copy_from_slice(&p[B1..B1 + HIDDEN1]);
		let hand = for_active(board, turn, |i|
		{
			for (h, w) in h1.iter_mut().zip(&p[W1 + i * HIDDEN1..W1 + (i + 1) * HIDDEN1])
			{
				*h += w;
			}
		});
		for (h, w) in h1.iter_mut().zip(&p[W1 + HAND * HIDDEN1..W1 + (HAND + 1) * HIDDEN1])
		{
			*h = (*h + hand * w).max(0.0);
		}
		let mut h2 = [0.0; HIDDEN2];
		for (k, h) in h2.iter_mut().enumerate()
		{
			let w = &p[W2 + k * HIDDEN1..W2 + (k + 1) * HIDDEN1];
			let sum: f32 = w.iter().zip(&h1).map(|(w, x)| w * x).sum();
			*h = (p[B2 + k] + sum).max(0.0);
		}
		let sum: f32 = p[W3..B3].iter().zip(&h2).map(|(w, x)| w * x).sum();
		Activations { h1, h2, out: (p[B3] + sum).tanh() }
	}

	/// The network's score for `board` with `turn` to move, in -1..1 (positive favours tigers).
	pub fn forward(&self, board: &GameBoard, turn: Turn) -> f32
	{
		self.forward_full(board, turn).out
	}

	/// Add the gradient of the squared error `(out - target)² / 2` for one sample to `grad`,
	/// returning that error.
	fn add_gradient(&self, s: &Sample, grad: &mut [f32]) -> f32
	{
		let p = &self.params;
		let a = self.forward_full(&s.board, s.turn);
		let err = a.out - s.target;
		// Back through tanh
		let d_out = err * (1.0 - a.out * a.out);
		grad[B3] += d_out;
		let mut d_h2 = [0.0; HIDDEN2];
		for k in 0..HIDDEN2
		{
			grad[W3 + k] += d_out * a.h2[k];
			// Back through ReLU: no gradient where the unit was off
			d_h2[k] = if a.h2[k] > 0.0 { d_out * p[W3 + k] } else { 0.0 };
		}
		let mut d_h1 = [0.0; HIDDEN1];
		for k in 0..HIDDEN2
		{
			if d_h2[k] == 0.0
			{
				continue;
			}
			grad[B2 + k] += d_h2[k];
			let row = W2 + k * HIDDEN1;
			for j in 0..HIDDEN1
			{
				grad[row + j] += d_h2[k] * a.h1[j];
				d_h1[j] += d_h2[k] * p[row + j];
			}
		}
		for j in 0..HIDDEN1
		{
			if a.h1[j] <= 0.0
			{
				d_h1[j] = 0.0;
			}
			grad[B1 + j] += d_h1[j];
		}
		let hand = for_active(&s.board, s.turn, |i|
		{
			for (g, d) in grad[W1 + i * HIDDEN1..W1 + (i + 1) * HIDDEN1].iter_mut().zip(&d_h1)
			{
				*g += d;
			}
		});
		for (g, d) in grad[W1 + HAND * HIDDEN1..W1 + (HAND + 1) * HIDDEN1].iter_mut().zip(&d_h1)
		{
			*g += hand * d;
		}
		err * err / 2.0
	}

	/// One optimiser step on the mean error over `batch`; returns that mean error.
	pub fn train_batch(&mut self, batch: &[Sample], adam: &mut Adam) -> f32
	{
		let mut grad = vec![0.0; PARAMS];
		let loss: f32 = batch.iter().map(|s| self.add_gradient(s, &mut grad)).sum();
		let n = batch.len().max(1) as f32;
		for g in &mut grad
		{
			*g /= n;
		}
		adam.step(&mut self.params, &grad);
		loss / n
	}

	/// The mean error over `samples`, without training.
	pub fn loss(&self, samples: &[Sample]) -> f32
	{
		let total: f32 = samples.iter().map(|s| { let e = self.forward(&s.board, s.turn) - s.target; e * e / 2.0 }).sum();
		total / samples.len().max(1) as f32
	}

	/// Saves as "TSNN", then the version and layer sizes as little-endian u32s, then every weight as a little-endian f32.
	pub fn save(&self, path: &Path) -> io::Result<()>
	{
		let mut bytes = Vec::with_capacity(20 + 4 * PARAMS);
		bytes.extend_from_slice(MAGIC);
		for n in [VERSION, INPUTS as u32, HIDDEN1 as u32, HIDDEN2 as u32]
		{
			bytes.extend_from_slice(&n.to_le_bytes());
		}
		for p in &self.params
		{
			bytes.extend_from_slice(&p.to_le_bytes());
		}
		fs::write(path, bytes)
	}

	pub fn load(path: &Path) -> io::Result<Self>
	{
		let bytes = fs::read(path)?;
		let bad = |msg: &str| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {msg}", path.display()));
		if bytes.len() != 20 + 4 * PARAMS || &bytes[..4] != MAGIC
		{
			return Err(bad("not a network file of the expected size"));
		}
		let word = |i: usize| u32::from_le_bytes(bytes[4 + 4 * i..8 + 4 * i].try_into().unwrap());
		if [word(0), word(1), word(2), word(3)] != [VERSION, INPUTS as u32, HIDDEN1 as u32, HIDDEN2 as u32]
		{
			return Err(bad("network has a different version or layer sizes"));
		}
		let params = bytes[20..].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
		Ok(Self { params })
	}
}

/// The Adam optimiser's state: running averages of each weight's gradient and squared gradient.
pub struct Adam
{
	pub learning_rate: f32,
	m: Vec<f32>,
	v: Vec<f32>,
	steps: i32,
}

impl Adam
{
	const BETA1: f32 = 0.9;
	const BETA2: f32 = 0.999;
	const EPSILON: f32 = 1e-8;

	pub fn new(learning_rate: f32) -> Self
	{
		Self { learning_rate, m: vec![0.0; PARAMS], v: vec![0.0; PARAMS], steps: 0 }
	}

	fn step(&mut self, params: &mut [f32], grad: &[f32])
	{
		self.steps += 1;
		// Corrects the averages' bias towards their zero start
		let c1 = 1.0 - Self::BETA1.powi(self.steps);
		let c2 = 1.0 - Self::BETA2.powi(self.steps);
		for i in 0..params.len()
		{
			self.m[i] = Self::BETA1 * self.m[i] + (1.0 - Self::BETA1) * grad[i];
			self.v[i] = Self::BETA2 * self.v[i] + (1.0 - Self::BETA2) * grad[i] * grad[i];
			params[i] -= self.learning_rate * (self.m[i] / c1) / ((self.v[i] / c2).sqrt() + Self::EPSILON);
		}
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	/// Positions from a pseudo-random game, with made-up targets.
	fn samples(n: usize, seed: u64) -> Vec<Sample>
	{
		let mut rng = Rng::new(seed);
		let mut board = GameBoard::default();
		let mut turn = Turn::Sheep;
		let mut out = Vec::new();
		while out.len() < n
		{
			if board.check_win(turn)
			{
				board = GameBoard::default();
				turn = Turn::Sheep;
			}
			out.push(Sample { board, turn, target: rng.next_f32() * 1.6 - 0.8 });
			let moves = board.get_moves(turn);
			board.make_move_unchecked(moves[rng.below(moves.len())]);
			turn = turn.next_turn();
		}
		out
	}

	#[test]
	fn gradient_matches_finite_differences()
	{
		let mut net = Network::random(7);
		let s = samples(30, 3)[29];
		let mut grad = vec![0.0; PARAMS];
		net.add_gradient(&s, &mut grad);
		let loss = |net: &Network| { let e = net.forward(&s.board, s.turn) as f64 - s.target as f64; e * e / 2.0 };
		let mut rng = Rng::new(11);
		// The input weights of unset inputs have no gradient, so check some of each layer's,
		// plus every weight that really has one
		let mut checked = 0;
		for _ in 0..4000
		{
			let i = rng.below(PARAMS);
			let eps = 1e-2;
			let old = net.params[i];
			net.params[i] = old + eps;
			let up = loss(&net);
			net.params[i] = old - eps;
			let down = loss(&net);
			net.params[i] = old;
			let numeric = (up - down) / (2.0 * eps as f64);
			let tolerance = 1e-3 + 2e-2 * numeric.abs().max(grad[i].abs() as f64);
			// A ReLU switching on or off inside ±eps makes the numeric estimate meaningless; such
			// kinks are rare, so skip a weight only where the two halves disagree wildly
			let left = (loss(&net) - down) / eps as f64;
			let right = (up - loss(&net)) / eps as f64;
			if (left - right).abs() > 0.1 * left.abs().max(right.abs()) + 1e-4
			{
				continue;
			}
			assert!((numeric - grad[i] as f64).abs() < tolerance, "weight {i}: numeric {numeric}, backprop {}", grad[i]);
			checked += 1;
		}
		assert!(checked > 3000);
	}

	#[test]
	fn training_can_fit_a_few_samples()
	{
		let mut net = Network::random(1);
		let data = samples(10, 5);
		let mut adam = Adam::new(3e-3);
		for _ in 0..2000
		{
			net.train_batch(&data, &mut adam);
		}
		assert!(net.loss(&data) < 1e-3, "loss {}", net.loss(&data));
	}

	#[test]
	fn save_and_load_round_trip()
	{
		let net = Network::random(9);
		let path = std::env::temp_dir().join(format!("tsnn-test-{}.bin", std::process::id()));
		net.save(&path).unwrap();
		let loaded = Network::load(&path);
		std::fs::write(&path, b"nope").unwrap();
		let broken = Network::load(&path);
		std::fs::remove_file(&path).unwrap();
		assert_eq!(loaded.unwrap(), net);
		assert_eq!(broken.unwrap_err().kind(), io::ErrorKind::InvalidData);
	}

	#[test]
	fn forward_matches_a_plain_matrix_multiply()
	{
		let net = Network::random(4);
		for s in samples(50, 8)
		{
			let mut x = [0.0f32; INPUTS];
			let hand = for_active(&s.board, s.turn, |i| x[i] = 1.0);
			x[HAND] = hand;
			let p = &net.params;
			let h1: Vec<f32> = (0..HIDDEN1).map(|j| (p[B1 + j] + (0..INPUTS).map(|i| x[i] * p[W1 + i * HIDDEN1 + j]).sum::<f32>()).max(0.0)).collect();
			let h2: Vec<f32> = (0..HIDDEN2).map(|k| (p[B2 + k] + (0..HIDDEN1).map(|j| h1[j] * p[W2 + k * HIDDEN1 + j]).sum::<f32>()).max(0.0)).collect();
			let out = (p[B3] + (0..HIDDEN2).map(|k| h2[k] * p[W3 + k]).sum::<f32>()).tanh();
			assert!((net.forward(&s.board, s.turn) - out).abs() < 1e-5);
		}
	}
}
