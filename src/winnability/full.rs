//! Multi-phase winnability analysis, coordinating search and static analysis.
//!
//! Designed for White as the intended winner; see [`super`] for how a
//! Black-intended-winner query is mirrored before reaching this module.

use chess::{Board, Piece, EMPTY};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    deduction::{is_statically_unwinnable, Analysis},
    search::{
        check_chain_search, compute_bishop_distances, compute_punishable, iterative_deepening,
        play_forced_moves, probe_non_pawn_moves, Search,
    },
    winnability::fast::exhaustively_unwinnable,
    Winnability,
};

/// Runs a multi-phase helpmate search, escalating from cheap to expensive
/// techniques until the position is resolved or all phases are exhausted.
///
/// Designed for White as the intended winner.
pub(crate) fn analysis(board: &Board) -> Option<Winnability> {
    let mut search = Search::new();
    let mut pos = *board;

    // Phase 1: Play forced moves.
    pos = play_forced_moves(&pos, &mut search);

    if search.interrupted {
        return Some(Winnability::Unwinnable);
    }

    // Phase 2: Depth-1 search with heuristics that selectively extend promising
    // variations beyond depth 1, to catch trivial positions quickly.
    search.depth_node_limit = 1000;
    search.transposition_table = None;
    search.reset_global_nodes();
    let result = iterative_deepening(&pos, &mut search, 1..=1);
    if let Some(result) = result {
        return Some(result);
    }

    // Phase 3: Early static analysis. Skipped for positions with queens, rooks
    // or knights, where it is less likely to be relevant; deferred to phase 5 in
    // that case.
    let has_queens_rooks_or_knights =
        (pos.pieces(Piece::Queen) | pos.pieces(Piece::Rook) | pos.pieces(Piece::Knight)) != EMPTY;
    if !has_queens_rooks_or_knights && is_statically_unwinnable(&pos) {
        return Some(Winnability::Unwinnable);
    }

    // Phase 4: Iterative deepening with transposition table.
    search.depth_node_limit = 7000;
    search.transposition_table =
        Some(FxHashMap::with_capacity_and_hasher(1_000, Default::default()));
    search.reset_global_nodes();
    let result = iterative_deepening(&pos, &mut search, 2..=128);
    if let Some(result) = result {
        return Some(result);
    }

    // Phase 5: Static analysis (only for positions that skipped phase 3).
    if has_queens_rooks_or_knights && is_statically_unwinnable(&pos) {
        return Some(Winnability::Unwinnable);
    }

    // Phase 6: Exhaustive unwinnability check for positions where at least
    // one side can only move pawns (no non-pawn move observed within a
    // 1000-node probe).
    let mut non_pawn_seen = [false; 2];
    probe_non_pawn_moves(&pos, &mut non_pawn_seen, &mut 0, 1_000);
    if non_pawn_seen.iter().any(|&seen| !seen) && exhaustively_unwinnable(&pos) {
        return Some(Winnability::Unwinnable);
    }

    // Phase 7: Follow all check chains; evaluate non-check leaves statically.
    let mut visited: FxHashSet<Board> = FxHashSet::with_capacity_and_hasher(64, Default::default());
    let result = check_chain_search(&pos, &mut visited, 0);
    if let Some(result) = result {
        return Some(result);
    }

    // Phase 8: Guided deep search using bishop distances and punishable moves.
    let mut analysis = Analysis::new(pos);
    analysis.saturate();

    search.bishop_distances = Some(compute_bishop_distances(&pos, analysis.steady));
    search.punishable = Some(compute_punishable(&pos));

    search.depth_node_limit = u64::MAX;
    search.global_node_limit = 1 << 24;
    search.transposition_table =
        Some(FxHashMap::with_capacity_and_hasher(1 << 16, Default::default()));
    search.reset_global_nodes();
    iterative_deepening(&pos, &mut search, 2..=128)
}
