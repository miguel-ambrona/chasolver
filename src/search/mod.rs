//! Helpmate search engine.
//!
//! [`iterative_deepening`] is the entry point: it deepens depth-by-depth,
//! delegating each depth's recursive exploration to [`engine::helpmate`].
//!
//! [`play_forced_moves`] is typically run first to fast-forward through any
//! stretch of the position with exactly one legal move, since those don't
//! need to be searched.
//!
//! Submodules:
//! - [`engine`]: the `Search` state and the recursive `helpmate` search.
//! - [`heuristics`]: move ordering, used to find a mate faster.
//! - [`probes`]: standalone, cheaper checks that don't go through `Search`.
//! - [`utils`]: board/material queries shared by the search and fast analysis.

mod engine;
mod heuristics;
mod probes;
mod utils;

use std::ops::RangeInclusive;

use chess::{Board, MoveGen};
pub(crate) use engine::Search;
pub(crate) use probes::{check_chain_search, probe_non_pawn_moves};
use rustc_hash::FxHashSet;
pub(crate) use utils::{
    compute_bishop_distances, compute_punishable, has_lonely_pawns, insufficient_white_material,
    nb_blocked_pawns, only_pawns_and_bishops,
};

use crate::Winnability;

/// Advances the position by playing forced moves (positions with exactly one
/// legal move).
///
/// Stops when a position has multiple choices or a repetition is detected.
/// On repetition, sets `search.interrupted = true`; in that case, the caller
/// should treat this as unwinnable.
///
/// Stops after 16 iterations. It's fine to stop there even if the actual forced
/// sequence runs longer (that should never happen in practice though, since the
/// world record for a forced sequence is 15 moves) because what matters is that
/// this function preserves the position's winnability, not that it plays the
/// sequence out to its end.
pub(crate) fn play_forced_moves(board: &Board, search: &mut Search) -> Board {
    let mut board = *board;
    let mut seen_positions: Option<FxHashSet<Board>> = None;

    for _ in 0..16 {
        let mut moves = MoveGen::new_legal(&board).peekable();
        let Some(m) = moves.next() else { break };
        if moves.peek().is_some() {
            break;
        }

        let seen = seen_positions.get_or_insert_with(|| {
            let mut set = FxHashSet::with_capacity_and_hasher(16, Default::default());
            set.insert(board);
            set
        });

        board = board.make_move_new(m);
        search.moves.push(m);
        search.iteration_nodes += 1;

        if !seen.insert(board) {
            search.reset_iteration();
            search.interrupted = true;
            return board;
        }
    }

    if seen_positions.is_some() {
        search.reset_iteration();
    }
    board
}

/// Searches for a helpmate by iterative deepening over the given depth range.
///
/// Returns `Some(Winnable)` (with the mating line) as soon as one is found,
/// `Some(Unwinnable)` if a depth completes without interruption (proving no
/// mate exists), or `None` if the global node limit is hit before the search
/// concludes.
pub(crate) fn iterative_deepening(
    board: &Board,
    search: &mut Search,
    depths_range: RangeInclusive<u8>,
) -> Option<Winnability> {
    for depth in depths_range {
        search.max_depth = depth;
        search.reset_iteration();

        match engine::helpmate(board, search, 0) {
            Some(helpmate) => return Some(Winnability::Winnable { helpmate }),
            None if !search.interrupted => return Some(Winnability::Unwinnable),
            None => (),
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use chess::Board;

    use super::*;

    #[test]
    fn test_play_forced_moves() {
        let board = Board::from_str("7k/8/6K1/8/8/8/8/6Q1 b - -").unwrap();
        let mut search = Search::new();
        let result = play_forced_moves(&board, &mut search);
        assert_ne!(result, board, "Should have made at least one forced move");
        assert!(!search.interrupted);

        let board = Board::from_str("k1b5/1p1p4/1P1P4/8/8/4p1p1/4P1P1/5B1K w - -").unwrap();
        let mut search = Search::new();
        play_forced_moves(&board, &mut search);
        assert!(search.interrupted, "Should detect repetition");
    }
}
