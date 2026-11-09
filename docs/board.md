# The board

Everything about the rules lives in [`src/board.rs`](../src/board.rs), along with its unit tests at
the bottom of the file.

## Types

| Type | Meaning |
|------|---------|
| `Pos = (u8, u8)` | A point `(x, y)`, both in `0..5`. Out-of-range points panic in `bit_idx`. |
| `PieceType` | `Tiger`, `Sheep` or `Empty`; what `get(pos)` returns |
| `Turn` | `Tiger` or `Sheep`, the side about to move. `next_turn()` flips it. |
| `Move` | `PlaceSheep { pos }`, `MoveSheep { from, to }` or `MoveTiger { from, to }`. A tiger jump is a `MoveTiger` of two points; `is_jump` tells them apart. |
| `MoveError` | Why `check_move` rejected a move, with a human-readable `Display` |
| `Outcome` | `Tiger`, `Sheep` or `Draw`; `value()` is +1, -1 or 0, matching the sign of scores |
| `GameBoard` | The position, including the sheep still in hand. `Copy`, 8 bytes. |

## The bitboard

A `GameBoard` is two `u32`s:

```text
tiger_mask:  [ unused (7 bits) ][ 25 board bits ]
sheep_mask:  [ sheep in hand   ][ 25 board bits ]
               bits 25..32        bits 0..25
```

Point `(x, y)` is bit `x + 5*y`, so `(0, 0)` (a5, top left) is bit 0 and `(4, 4)` (e1) is bit 24:

```text
     a   b   c   d   e
5    0   1   2   3   4
4    5   6   7   8   9
3   10  11  12  13  14
2   15  16  17  18  19
1   20  21  22  23  24
```

The high 7 bits of `sheep_mask`, from `SHEEP_COUNT_SHIFT` = 25, aren't points: they hold the
number of sheep still in hand, 20 at the start. `sheep_left()` reads them, and placing a sheep sets
its board bit and subtracts `ONE_SHEEP` (`1 << 25`) in one go. Killing a sheep only clears its board
bit; the in-hand count is unplaced sheep, not living ones. So the number of dead sheep is
`20 - sheep_count() - sheep_left()`.

The default board has tigers on bits 0, 4, 20 and 24 (the corners) and 20 sheep in hand.
`GameBoard::EMPTY` has nothing at all and is used as the "no entry" board in the transposition table.

Useful accessors:

- `tiger_points()`, `sheep_points()` — the 25 board bits alone.
- `sheep_count()` — sheep on the board (`popcount` of the board bits).
- `key()` — the whole board as one `u64` (`sheep_mask << 32 | tiger_mask`). Different boards give
  different keys, so it serves as a hash key and for ordering.

## Lines

Every point has horizontal and vertical lines. Diagonals only run through points where `x + y` is
even (the corners, the centre and so on), as on a Bagh-Chal board:

```text
5  T───·───·───·───T
   │ ╲ │ ╱ │ ╲ │ ╱ │
4  ·───·───·───·───·
   │ ╱ │ ╲ │ ╱ │ ╲ │
3  ·───·───·───·───·
   …
```

`on_line(from, to, dist)` checks this for a single move: orthogonal and `dist` away, or diagonal,
`dist` away and starting from an even point. A diagonal step keeps `x + y`'s parity, so a diagonal
never leaves the even points.

## Move generation with bitmasks

`get_moves(turn)` doesn't loop over pieces. It works on all pieces of a side at once, one direction
at a time.

- `DIRS` lists the 8 directions as `(dx, dy)`: 4 orthogonal, then 4 diagonal. `DELTAS` is the
  matching change in bit index, `dx + 5*dy`.
- `STEP_SOURCES[d]` is a mask, built at compile time by `line_sources`, of the points from which
  one step in direction `d` stays on the board *and* follows a line. `JUMP_SOURCES[d]` is the same
  for two steps.
- `shift(mask, delta)` moves every point in a mask by `delta` bits. Shifting can wrap a point from
  the end of one row to the start of the next, but the source masks exclude every point for which
  that would happen.

Then:

```text
sheep steps in direction d  = shift(sheep  & STEP_SOURCES[d], δ) & empty
tiger steps in direction d  = shift(tigers & STEP_SOURCES[d], δ) & empty
jumped sheep                = shift(tigers & JUMP_SOURCES[d], δ) & sheep
tiger jumps in direction d  = shift(jumped sheep, δ) & empty
```

`sheep_steps()` and `tiger_targets()` compute these masks, and `get_moves` turns each set bit back
into a `Move` (the source is the destination minus `δ`, or minus `2δ` for a jump). Tiger jumps are
listed before tiger steps, since the search wants to try kills first. While sheep are in hand, the
sheep's only moves are placements on every empty point.

`has_moves(turn)` uses the same masks but only asks whether any is non-zero, without building a
`Vec`. `check_win` uses it, since it's called at every node of the search.

## Playing moves

- `check_move(mv)` returns `Ok(())` or the specific `MoveError`: placing with none in hand, onto an
  occupied point, moving a sheep while some are still in hand, moving something that isn't there,
  moving off a line, jumping over something that isn't a sheep, and so on. The tests assert on the
  exact variant.
- `make_move(mv)` checks first and leaves the board unchanged on an error.
- `make_move_unchecked(mv)` skips the checks. Use it only for moves from `get_moves` (the search
  does, for speed). An illegal move here leaves the board in a state the rules can't reach.
- `after_move_unchecked(mv)` returns a new board instead of changing this one.

A tiger move of two points always kills the sheep in between (`move_tiger` uses the `midpoint`).

## Winning and drawing

`check_win(turn)` is called with the side **about to move**, and `true` means the side that **just
moved** has won:

- Tigers have won once 5 sheep are dead: `sheep_count() + sheep_left() <= 15`.
- Otherwise, the side that just moved has won if `turn` has no legal move. This is how sheep win,
  by leaving every tiger without a move. (Tigers could in principle win this way too, by leaving the
  sheep with no moves.)

So `Outcome::win(turn.next_turn())` is the winner when `check_win(turn)` is true.

### Repetitions

A game is drawn on a threefold repetition, as in chess: the same position with the same side to
move, for the third time. `repetitions(previous, board)` counts how many times `board` came up
before with the same side to move, where `previous` is every earlier board, oldest first. Since the
last board in `previous` had the other side to move, it looks at every second board going back:
`previous.iter().rev().skip(1).step_by(2)`.

Two shortcuts keep it cheap:

- Placing and killing can't be undone, so nothing can repeat while sheep are in hand: it returns 0.
- For the same reason, it stops scanning at the first earlier board with a different number of
  sheep on it.

`REPETITION_DRAW` = 2: a position that has come up twice before is a draw. The search uses a
stricter rule internally (one earlier occurrence scores as a draw); see [The minimax search](search.md#repetitions).

## Symmetries

The board has 8 symmetries: 4 rotations, each optionally mirrored. All of them map lines to lines,
because they keep the parity of `x + y`, so a transformed position is just as legal and equally good
for either side. The search uses this to share cache entries between symmetric positions, and
training uses it to show the network each position in a random orientation.

- `transform_pos(pos, sym)` with `sym` in `0..SYMMETRIES` (8): bit 2 of `sym` swaps x and y, then
  bit 0 mirrors x and bit 1 mirrors y.
- `inverse_symmetry(sym)` undoes it. Plain mirrors undo themselves; with the swap first, undoing
  means swapping which axis is mirrored.
- `transform_move(mv, sym)` transforms both ends of a move.
- `GameBoard::transformed(sym)` transforms a whole board. Doing that point by point would be slow,
  so `ROW_MAPS` (built at compile time) holds, for each symmetry and each of the 5 rows, where each
  of the 32 possible sets of points in that row end up. A mask then transforms in 5 table lookups
  ORed together. The sheep-in-hand bits are carried over unchanged.
- `canonical()` returns the transform with the smallest `key()` and the `sym` that reaches it. All 8
  symmetric boards have the same canonical board.
