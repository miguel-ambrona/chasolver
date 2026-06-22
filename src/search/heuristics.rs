//! Move ordering heuristics for the helpmate search.

use chess::{Board, ChessMove, Color, Piece, Square, EMPTY};
use rustc_hash::FxHashSet;

use super::utils::{is_capture, is_pawn_advance, moves_toward_square};
use crate::{
    search::utils::flip_square_file,
    utils::{DARK_SQUARES, LIGHT_SQUARES},
};

/// Move variation types for depth adjustment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MoveVariation {
    /// Standard depth increment.
    Normal,
    /// Decrease depth (search deeper on promising moves).
    Reward,
    /// Increase depth (search shallower on bad moves).
    Punish,
}

/// Returns the depth-adjustment class for move `m`.
///
/// Punishable moves are immediately classified as `Punish`; otherwise
/// delegates to color-specific heuristics.
pub(crate) fn classify_move(
    board: &Board,
    m: ChessMove,
    punishable: &Option<FxHashSet<ChessMove>>,
    bishop_distances: &Option<[u8; 64]>,
) -> MoveVariation {
    if let Some(punishable) = punishable {
        if punishable.contains(&m) {
            return MoveVariation::Punish;
        }
    }
    match board.side_to_move() {
        Color::White => classify_white_move(board, m, bishop_distances),
        Color::Black => classify_black_move(board, m, bishop_distances),
    }
}

/// Returns the depth-adjustment class for a White move.
///
/// Rewards pawn advances, captures, and moves that approach the heuristic
/// target square ([`set_target`]) or improve a bishop's distance to the
/// mating corner ([`super::utils::compute_bishop_distances`]). Defaults to
/// `Normal`.
fn classify_white_move(
    board: &Board,
    m: ChessMove,
    bishop_distances: &Option<[u8; 64]>,
) -> MoveVariation {
    let moved_piece = board.piece_on(m.get_source()).unwrap();

    if is_pawn_advance(board, m)
        || is_capture(board, m)
        || set_target(board, moved_piece)
            .is_some_and(|t| moves_toward_square(m, moved_piece, t) < 0)
    {
        return MoveVariation::Reward;
    }

    if let Some(ds) = bishop_distances {
        if moved_piece == Piece::Bishop {
            let src_dist = ds[m.get_source().to_index()];
            let dest_dist = ds[m.get_dest().to_index()];

            if src_dist > 200 {
                return MoveVariation::Normal;
            }
            if src_dist <= 1 && src_dist < dest_dist {
                return MoveVariation::Reward;
            }
            if src_dist > 2 && src_dist > dest_dist {
                return MoveVariation::Reward;
            }
        }
    }

    MoveVariation::Normal
}

/// Returns the depth-adjustment class for a Black move.
///
/// Rewards pawn captures in sufficiently complex positions and moves that
/// approach the target square or shorten a bishop's distance to the mating
/// corner. Punishes other captures and pawn advances, which tend to
/// simplify the position away from the patterns the search is tuned to
/// explore. Defaults to `Normal`.
fn classify_black_move(
    board: &Board,
    m: ChessMove,
    bishop_distances: &Option<[u8; 64]>,
) -> MoveVariation {
    let moved_piece = board.piece_on(m.get_source()).unwrap();

    if bishop_distances.is_some()
        && board.combined().popcnt() > 5
        && is_capture(board, m)
        && (moved_piece == Piece::Pawn || board.piece_on(m.get_dest()) == Some(Piece::Pawn))
    {
        return MoveVariation::Reward;
    }

    if let Some(ds) = bishop_distances {
        if board.piece_on(m.get_source()) == Some(Piece::Bishop)
            && ds[m.get_source().to_index()] > ds[m.get_dest().to_index()]
        {
            return MoveVariation::Reward;
        }
    }

    if set_target(board, moved_piece).is_some_and(|t| moves_toward_square(m, moved_piece, t) < 0) {
        return MoveVariation::Reward;
    }

    if is_capture(board, m) || is_pawn_advance(board, m) {
        return MoveVariation::Punish;
    }

    MoveVariation::Normal
}

/// Returns the heuristic target square for `moved_piece`, or `None` if no
/// target applies.
///
/// Used to reward moves approaching the mating corner and penalize those moving
/// away. The target corner (H8-side or A8-side) is determined by the bishop
/// colors on the board.
fn set_target(board: &Board, moved_piece: Piece) -> Option<Square> {
    let bishops = board.pieces(Piece::Bishop);
    let white_bishops = board.color_combined(Color::White) & bishops;
    let black_bishops = board.color_combined(Color::Black) & bishops;

    let dark_corner = (white_bishops & DARK_SQUARES) != EMPTY
        || (white_bishops == EMPTY && (black_bishops & LIGHT_SQUARES) != EMPTY);

    let mut target = match board.side_to_move() {
        Color::White => match moved_piece {
            Piece::King => Some(Square::G7),   // We aim both at G6 and H7.
            Piece::Knight => Some(Square::E5), // We aim both at G6 and F7.
            _ => None,
        },
        Color::Black => match moved_piece {
            Piece::King => Some(Square::H8),
            Piece::Knight => Some(Square::G8),
            _ => None,
        },
    };

    if !dark_corner {
        target = target.map(flip_square_file);
    }

    target
}
