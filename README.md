# CHA-Solver

[![crates.io](https://img.shields.io/crates/v/chasolver.svg)](https://crates.io/crates/chasolver)
[![docs.rs](https://docs.rs/chasolver/badge.svg)](https://docs.rs/chasolver)
[![build](https://github.com/miguel-ambrona/chasolver/actions/workflows/ci.yml/badge.svg)](https://github.com/miguel-ambrona/chasolver/actions/workflows/ci.yml)
[![website](https://img.shields.io/badge/website-chasolver.org-blue)](https://chasolver.org)

A chess *helpmate* solver and unwinnability detector.

Given a chess position and an intended winner, CHA-Solver either finds a
helpmate line (a sequence of legal moves that ends in a checkmate delivered
by the intended winner) or proves that no such sequence exists.

> [!NOTE]
> In a helpmate, the losing side is assumed to cooperate rather than play its
> best defense.

## What is CHA-Solver for?

When a player runs out of time, the game is almost always ruled a loss, *but it
is exceptionally declared a draw if their opponent cannot possibly checkmate
them*. Indeed, Article 6.9 of the
[FIDE Laws of Chess](https://www.fide.com/FIDE/handbook/LawsOfChess.pdf)
states:

> “[...] the game is drawn if the position is such that the opponent cannot
> checkmate the player’s king by any possible series of legal moves.”

Chess servers have historically ignored this rule, treating the problem as
computationally intractable, leading to many wrongly decided games.

CHA-Solver decides with mathematical certainty whether a helpmate is achievable
from a given position. The algorithm is described in a peer-reviewed
[paper](https://chasolver.org/FUN22-full.pdf) presented at the
*11th International Conference on Fun with Algorithms*,
[FUN 2022](https://sites.google.com/view/fun2022/).

In practice, CHA-Solver has resolved every single timeout position across the
entire [Lichess database](https://database.lichess.org/) of standard rated
games, more than 7 billion games in total. See [Performance](#performance)
for detailed benchmarks measured on a 10-million-game sample.

## Usage

Add `chasolver` to your `Cargo.toml`:

```toml
[dependencies]
chasolver = "3.0"
```

The main entry point is `winnability`, which takes a position and an intended
winner and returns an `Option<Winnability>`: `Some(Winnable)` or
`Some(Unwinnable)` if the search concludes, or `None` if it doesn't (the
node limit was reached before a definitive answer was found). `is_unwinnable_fast`
takes the same arguments and returns a plain `bool`; see the docs for more
details.

As an example, consider this position which arose in a real Lichess game
([ijyj0mHa](https://lichess.org/ijyj0mHa#120)).
White ran out of time and was adjudicated a loss, but this was an unfair
result: *Black cannot possibly deliver checkmate*, even with White's
cooperation, because there is a pawn wall that is permanently locked
(the bishops are helpless to unlock it).

![Position from lichess.org/ijyj0mHa](https://backscattering.de/web-boardimage/board.svg?fen=5b2/p7/Pp3k2/1Pp1pBp1/2P1P1P1/5K2/8/8&coordinates=true&size=300)

Surprisingly, *White can actually deliver checkmate* from this position,
although that is irrelevant for the above timeout adjudication
(it is White who ran out of time). Try to find a helpmate for White
yourself before checking the code below.

CHA-Solver proves both facts.

```rust
use chasolver::{winnability, Winnability};
use chess::{Board, BoardStatus, Color};
use std::str::FromStr;

let board = Board::from_str("5b2/p7/Pp3k2/1Pp1pBp1/2P1P1P1/5K2/8/8 w - -").unwrap();

// White ran out of time and was adjudicated a loss. But since Black cannot
// possibly deliver checkmate, the game should have been declared a draw.
assert_eq!(winnability(&board, Color::Black), Some(Winnability::Unwinnable));

// Surprisingly, had Black been the one to run out of time (after they got the turn),
// Black should be adjudicated a loss because a helpmate for White exists.
let Some(Winnability::Winnable { helpmate }) = winnability(&board, Color::White) else {
    panic!("expected this position to be winnable for White")
};

// Play out the mating line and verify Black is checkmated.
let mated_board = helpmate.iter().fold(board, |b, &m| b.make_move_new(m));
assert_eq!(mated_board.status(), BoardStatus::Checkmate);
assert_eq!(mated_board.side_to_move(), Color::Black);
```

> [!TIP]
> If you don't need the mating line and want to check many positions
> quickly, reach for `is_unwinnable_fast` instead, see
> [Performance](#performance) for the speed difference.

Full API documentation is available at
[docs.rs/chasolver](https://docs.rs/chasolver), and an interactive analyzer at
[chasolver.org](https://chasolver.org).

## Performance

Both the full search (`winnability`) and the fast check (`is_unwinnable_fast`)
return provably correct answers; they trade off speed against coverage. The
numbers below were measured on the timeout position of 10 million real
Lichess games that were adjudicated as a loss for the side that ran out of
time, single-threaded on an Intel Xeon w3-2435 (release build).

|                        | full                   | fast                             |
| ---------------------- | ---------------------- | -------------------------------- |
| Speed                  | ~30,000 positions/sec  | 700,000+ positions/sec           |
| Avg. time per position | 33 µs                  | 1.4 µs                           |
| Worst-case time        | 930 ms                 | 860 µs                           |
| Coverage               | Complete*              | Sound, but incomplete in theory  |
| Best for               | Per-game analysis      | Batch database scans             |

Both are complete on this set: they agree on every unwinnable position
(i.e. every unfairly classified game), and the full search additionally
returns an explicit helpmate for each position that turns out to be winnable.

\* Complete on every known legal position, real or constructed; not proven
complete in theory. You could be the first to find one it can't resolve!

## Acknowledgments

I would like to thank everyone who has contributed to this project, including
the community members whose queries to
[chasolver.org/analyzer](https://chasolver.org/analyzer) helped build
[`tests/positions.txt`](tests/positions.txt): a set of challenging positions,
each with its expected classification with respect to winnability, that
you're welcome to use to evaluate your own unwinnability solver.

Special thanks: [chasolver.org/thanks](https://chasolver.org/thanks).

## License

Licensed under the [MIT License](LICENSE).
