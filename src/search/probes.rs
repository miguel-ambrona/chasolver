//! Recursive position probes that don't drive the `Search`/`engine::helpmate`
//! machinery: a lightweight node-counting query (`probe_non_pawn_moves`) and
//! a standalone search strategy (`check_chain_search`).

use chess::{Board, Color, MoveGen, Piece, EMPTY};
use rustc_hash::FxHashSet;

use crate::{deduction::is_statically_unwinnable, Winnability};

/// Explores moves from `board` up to `max_nodes`, recording in
/// `non_pawn_seen` (indexed by color) whether each side has any non-pawn
/// move available. Stops early once both flags are set or the node budget is
/// exhausted. Used to decide whether a side is pawn-only without running a
/// full search.
pub(crate) fn probe_non_pawn_moves(
    board: &Board,
    non_pawn_seen: &mut [bool; 2],
    nodes: &mut u64,
    max_nodes: u64,
) {
    for m in MoveGen::new_legal(board) {
        let color_idx = board.side_to_move().to_index();
        if board.piece_on(m.get_source()) != Some(Piece::Pawn) {
            non_pawn_seen[color_idx] = true;
        }
        *nodes += 1;
        if *nodes >= max_nodes || non_pawn_seen.iter().all(|&s| s) {
            return;
        }
        let new_board = board.make_move_new(m);
        probe_non_pawn_moves(&new_board, non_pawn_seen, nodes, max_nodes);
        if *nodes >= max_nodes || non_pawn_seen.iter().all(|&s| s) {
            return;
        }
    }
}

/// Search for a checkmate by following check chains.
///
/// Recursively explores check-giving moves. Non-check leaves are evaluated with
/// static analysis; the result is `Some(Unwinnable)` only if every such leaf is
/// statically unwinnable. At depth 0 the first move is explored even without
/// giving check.
pub(crate) fn check_chain_search(
    board: &Board,
    visited: &mut FxHashSet<Board>,
    depth: u32,
) -> Option<Winnability> {
    if depth > 16 {
        return None;
    }
    visited.insert(*board);

    let mut legal_moves = MoveGen::new_legal(board).peekable();

    if legal_moves.peek().is_none() {
        if board.side_to_move() == Color::Black && *board.checkers() != EMPTY {
            return Some(Winnability::Winnable { helpmate: vec![] });
        }
        return Some(Winnability::Unwinnable);
    }

    for m in legal_moves {
        let new_board = board.make_move_new(m);
        if visited.contains(&new_board) {
            continue;
        }
        if *new_board.checkers() != EMPTY || depth == 0 {
            match check_chain_search(&new_board, visited, depth + 1) {
                w @ Some(Winnability::Winnable { .. }) => return w,
                None => return None,
                Some(Winnability::Unwinnable) => continue,
            }
        }
        if !is_statically_unwinnable(&new_board) {
            return None;
        }
    }

    Some(Winnability::Unwinnable)
}
