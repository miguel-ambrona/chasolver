//! Fast, sound-but-incomplete unwinnability check.
//!
//! Designed for White as the intended winner; see [`super`] for how a
//! Black-intended-winner query is mirrored before reaching this module.

use std::cmp::max;

use chess::{Board, Color, MoveGen, Piece, EMPTY};
use rustc_hash::FxHashSet;

use crate::{
    deduction::is_statically_unwinnable,
    search::{
        check_chain_search, has_lonely_pawns, insufficient_white_material, nb_blocked_pawns,
        only_pawns_and_bishops, play_forced_moves, Search,
    },
    Winnability,
};

/// Fast but incomplete unwinnability analysis.
///
/// Escalates through phases of increasing cost, each gated by cheaper
/// conditions than the last, trying to prove the position is unwinnable for
/// White; tuned to be complete on the Lichess test corpus while staying
/// cheap on average. In order:
///
/// 1. Play out any forced sequence; a detected repetition counts as unwinnable.
/// 2. A bounded-depth, full-width search ([`is_dynamically_unwinnable`]),
///    deeper when only pawns and bishops remain.
/// 3. On quiet, mutually-blocked pawn structures with few legal moves, the same
///    search again at a much greater depth.
/// 4. On such structures that also have no lone (unopposed) pawns: static
///    analysis ([`is_statically_unwinnable`]), then check-chain following
///    ([`check_chain_search`]) if in check, then a full reachability closure
///    ([`exhaustively_unwinnable`]) if pawn-only and no pawn has moved twice in
///    any explored line.
///
/// Returns `true` only if the position is provably unwinnable for White.
pub(crate) fn is_unwinnable(board: &Board) -> bool {
    let mut board = *board;
    let mut search = Search::new();

    board = play_forced_moves(&board, &mut search);
    if search.interrupted {
        return true;
    }

    let mut legal_moves = MoveGen::new_legal(&board).peekable();
    if legal_moves.peek().is_none() {
        return !(board.side_to_move() == Color::Black && *board.checkers() != EMPTY);
    }

    let only_pawns_and_bishops = only_pawns_and_bishops(&board);
    let nb_pawns = board.pieces(Piece::Pawn).popcnt();

    let initial_depth = if only_pawns_and_bishops { 7 } else { 4 };

    let mut moved = MovedPieces::default();
    if is_dynamically_unwinnable(&board, initial_depth, false, &mut moved) {
        return true;
    }

    // No QRN moved, and enough pawn pairs are mutually blocking relative to
    // the pawn count.
    let low_entropy_candidate =
        !moved.qrn && nb_blocked_pawns(&board) >= max(1i8, (nb_pawns as i8 / 2) - 2) as u32;

    if only_pawns_and_bishops
        && low_entropy_candidate
        && moved.king != [true, true]
        && legal_moves.count() <= 8
        && is_dynamically_unwinnable(&board, 15, false, &mut moved)
    {
        return true;
    }

    // Plus no lone (unopposed) pawns, which could otherwise run free.
    let blocked_candidate = low_entropy_candidate && !has_lonely_pawns(&board);

    // Static analysis tends to be conclusive on blocked structures.
    if blocked_candidate && is_statically_unwinnable(&board) {
        return true;
    }

    // Or, if in check with enough pawns left, try following check chains.
    if *board.checkers() != EMPTY
        && blocked_candidate
        && nb_pawns >= 6
        && only_pawns_and_bishops
        && matches!(
            check_chain_search(&board, &mut FxHashSet::default(), 0),
            Some(Winnability::Unwinnable)
        )
    {
        return true;
    }

    // No bishops and plenty of pawns: cheap enough, once transpositions are
    // collapsed, to exhaust fully, as long as no pawn ran (moved twice).
    if blocked_candidate
        && only_pawns_and_bishops
        && nb_pawns >= 10
        && *board.pieces(Piece::Bishop) == EMPTY
        && !moved.two_pawn_moves_in_a_line
    {
        return exhaustively_unwinnable(&board);
    }

    false
}

/// Tracks, across a `is_dynamically_unwinnable` exploration, which piece types
/// have been seen moving.
#[derive(Default, Debug)]
struct MovedPieces {
    /// Whether the white (respectively black) king moved.
    king: [bool; 2],
    /// Whether any bishop moved.
    bishop: bool,
    /// Whether any queen, rook, or knight moved.
    qrn: bool,
    /// Whether some line, anywhere in the explored tree, played a pawn move
    /// twice (not necessarily on consecutive plies; other moves may fall
    /// between them).
    two_pawn_moves_in_a_line: bool,
}

/// Exhaustively prove a position is unwinnable for White by checking all
/// branches.
///
/// Returns `true` only if EVERY branch up to `depth` is provably unwinnable.
/// `moved` accumulates which piece types were seen moving anywhere in the
/// explored tree. `pawn_move_seen` is whether a pawn move has already been
/// played earlier on the line leading to `board` (not shared across
/// branches, unlike `moved`), used to detect a second pawn move anywhere
/// further down that same line.
fn is_dynamically_unwinnable(
    board: &Board,
    depth: u8,
    pawn_move_seen: bool,
    moved: &mut MovedPieces,
) -> bool {
    if insufficient_white_material(board) {
        return true;
    }

    let mut legal_moves = MoveGen::new_legal(board).peekable();

    if legal_moves.peek().is_none() {
        return !(board.side_to_move() == Color::Black && *board.checkers() != EMPTY);
    }

    if depth == 0 {
        return false;
    }

    for m in legal_moves {
        let moved_piece = board.piece_on(m.get_source()).unwrap();

        match moved_piece {
            Piece::King => moved.king[board.side_to_move().to_index()] = true,
            Piece::Pawn => moved.two_pawn_moves_in_a_line |= pawn_move_seen,
            Piece::Bishop => moved.bishop = true,
            _ => moved.qrn = true,
        };
        let new_board = board.make_move_new(m);
        let pawn_move_seen = pawn_move_seen || moved_piece == Piece::Pawn;
        if !is_dynamically_unwinnable(&new_board, depth - 1, pawn_move_seen, moved) {
            return false;
        }
    }

    true
}

/// Exhaustively determines whether `board` is unwinnable for White by
/// computing the full set of positions reachable from it.
///
/// Expands one ply at a time: each step plays every legal move from every
/// position in the current table, keeping only the resulting positions never
/// seen in any previous step, and feeds those into the next table. Naturally
/// terminates once a step produces nothing new, no depth limit needed, and
/// no position is ever explored twice.
///
/// Returns `true` (unwinnable) only if no reachable position is a checkmate of
/// Black. Gives up (returns `false`) if the reachable set grows past 3,000
/// positions before closing, rather than risk runaway memory/time.
pub(crate) fn exhaustively_unwinnable(board: &Board) -> bool {
    let mut seen = FxHashSet::default();
    seen.insert(*board);
    let mut table = vec![*board];
    let mut next_table = Vec::new();

    while !table.is_empty() {
        for board in &table {
            if insufficient_white_material(board) {
                continue;
            }

            let mut legal_moves = MoveGen::new_legal(board).peekable();
            if legal_moves.peek().is_none() {
                if board.side_to_move() == Color::Black && *board.checkers() != EMPTY {
                    return false;
                }
                continue;
            }

            for m in legal_moves {
                let new_board = board.make_move_new(m);
                if seen.insert(new_board) {
                    if seen.len() > 3000 {
                        return false;
                    }
                    next_table.push(new_board);
                }
            }
        }

        table.clear();
        std::mem::swap(&mut table, &mut next_table);
    }

    true
}
