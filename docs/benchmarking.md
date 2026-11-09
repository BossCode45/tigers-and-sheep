# Benchmarking and profiling

## The `bench` command

```bash
cargo run --profile profiling -- bench [depth]     # default depth 6
```

[`src/bench.rs`](../src/bench.rs) searches six fixed positions, one from each phase of the game, to a
fixed depth without the TUI, and prints the move, score and time for each, plus a total:

| Label | Position after |
|-------|----------------|
| opening | 0 plies |
| tiger reply | 1 ply |
| mid-placement | 20 plies |
| end of placement | 38 plies |
| early movement | 45 plies |
| movement | 60 plies |

The positions come from `ClaudePlayer` playing itself to depth 4, which is deterministic and plays
much more like a real game than random moves. Each search uses a fresh `ClaudePlayer` with
`max_depth` set to the requested depth and an effectively infinite time limit, so every run does
exactly the same work and timings can be compared. Depth 9 takes well under a second in total.

**The rule for optimisations:** a change that only makes the search faster must leave every move
and every score unchanged. If they change, the change altered what the search computes, not just
how fast.

## Search counters

```bash
cargo run --profile profiling --features search-stats -- bench 9
```

With the `search-stats` feature, each position also prints:

- nodes searched and nodes per second, leaves, terminal positions, repetitions, move generations per node;
- cache probes, hit rate, the share of hits that settled the score without searching, and overwrites;
- cutoffs, and the share of them on the first move tried, which measures move ordering.

The counters are thread-local `SearchStats` fields bumped by the `count!` macro (defined in
`player.rs`, also used in `board.rs`). Without the feature `count!` expands to nothing. Counting
slows the search a little, so compare times only between builds with the same features.

## Profiling

The `profiling` profile is `release` with debug info. The dev shell includes `perf` and `samply`.

```bash
cargo build --profile profiling
perf record -e cycles:u -m 16 --call-graph dwarf target/profiling/tigers-and-sheep-rs bench 9
perf report
```

As a non-root user `perf record` needs `-m 16` (a small ring buffer) with `--call-graph dwarf`,
otherwise mmap fails against the `perf_event_mlock_kb` limit.

Or, for a flame graph in the browser:

```bash
samply record target/profiling/tigers-and-sheep-rs bench 9
```

## Check the clock source first

```bash
cat /sys/devices/system/clocksource/clocksource0/current_clocksource
```

The search reads the clock (`Instant::now()`) once per interior node to check its deadline. With the
`tsc` clock source that's a cheap instruction. If the kernel has fallen back to `hpet`, as has
happened on the development machine, every read becomes a system call and the search runs at nearly
half speed. Only compare timings taken on the same clock source.

## History

[`plans.org`](../plans.org) records each optimisation with its before and after timings, node counts
and cutoff rates, and the next candidates. Add to it when you make a change that affects speed.
