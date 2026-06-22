//! A chess helpmate solver and unwinnability detector.
//!
//! Given a position and an intended winner, determines whether a helpmate is
//! achievable (a sequence of legal moves ending in checkmate by the intended
//! winner) or proves that none exists.
//!
//! Note that in a helpmate both players cooperate toward that goal: the
//! intended winner delivers checkmate, but not necessarily by force.
//!
//! See the [README](https://github.com/miguel-ambrona/chasolver#readme) for
//! the background motivating this crate, and for usage examples.

// Not part of the public API; keeps the README's usage example under test
// without rendering the whole README as a docs.rs page.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

mod deduction;
mod search;
mod utils;
mod winnability;

use chess::{Board, ChessMove, Color};
use winnability::{fast, full};

/// Result of a [`winnability`] analysis for the intended winner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Winnability {
    /// Helpmate is achievable for the intended winner; `helpmate` is a
    /// possible sequence of moves leading to checkmate.
    Winnable { helpmate: Vec<ChessMove> },

    /// Helpmate is provably impossible for the intended winner.
    Unwinnable,
}

/// Full helpmate search for `intended_winner`.
///
/// Returns `Some` result if the analysis is successful:
/// - [`Winnability::Winnable`] if the position is winnable for the intended
///   winner, together with a supporting helpmate sequence.
///
/// - [`Winnability::Unwinnable`] if no helpmate exists.
///
/// Returns `None` if winnability could not be decided on this position.
pub fn winnability(board: &Board, intended_winner: Color) -> Option<Winnability> {
    match intended_winner {
        Color::White => full::analysis(board),
        Color::Black => {
            let flipped_board = utils::mirror_board(board);
            let result = full::analysis(&flipped_board);
            result.map(utils::mirror_moves)
        }
    }
}

/// Fast but incomplete check for unwinnability; cheaper than [`winnability`]
/// when a mating line isn't needed.
///
/// Returns `true` if the position is provably unwinnable for `intended_winner`,
/// `false` if the analysis is inconclusive (the position may still be
/// unwinnable).
pub fn is_unwinnable_fast(board: &Board, intended_winner: Color) -> bool {
    match intended_winner {
        Color::White => fast::is_unwinnable(board),
        Color::Black => fast::is_unwinnable(&utils::mirror_board(board)),
    }
}
