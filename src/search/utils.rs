//! Board/material queries and distance helpers used by the search and fast
//! analysis.

use std::sync::LazyLock;

use chess::{
    get_bishop_moves, BitBoard, Board, ChessMove, Color, File, MoveGen, Piece, Square, ALL_SQUARES,
    EMPTY,
};
use rustc_hash::FxHashSet;

use crate::{
    deduction::is_statically_unwinnable,
    utils::{DARK_SQUARES, LIGHT_SQUARES},
};

// ---------------------------------------------------------------------------
// Board / material queries.
// ---------------------------------------------------------------------------

/// True if the board has no knights, rooks, or queens.
pub(crate) fn only_pawns_and_bishops(board: &Board) -> bool {
    (*board.pieces(Piece::Knight) | *board.pieces(Piece::Rook) | *board.pieces(Piece::Queen))
        == EMPTY
}

/// Count white pawns with a black pawn or a black king directly in front.
pub(crate) fn nb_blocked_pawns(board: &Board) -> u32 {
    let white_pawns = (board.color_combined(Color::White) & board.pieces(Piece::Pawn)).0;
    let black_pawns = (board.color_combined(Color::Black) & board.pieces(Piece::Pawn)).0;
    let black_king = (board.color_combined(Color::Black) & board.pieces(Piece::King)).0;
    ((white_pawns << 8) & (black_pawns | black_king)).count_ones()
}

/// True if any pawn has no opposing pawn on the same file.
/// (white pawn not on rank 7, black pawn not on rank 2.)
pub(crate) fn has_lonely_pawns(board: &Board) -> bool {
    let white_pawns = board.color_combined(Color::White) & board.pieces(Piece::Pawn);
    let black_pawns = board.color_combined(Color::Black) & board.pieces(Piece::Pawn);

    let mut white_files = 0u8;
    let mut black_files = 0u8;
    for s in white_pawns {
        if s.get_rank() != chess::Rank::Seventh {
            white_files |= 1 << s.get_file().to_index();
        }
    }
    for s in black_pawns {
        if s.get_rank() != chess::Rank::Second {
            black_files |= 1 << s.get_file().to_index();
        }
    }
    white_files != black_files
}

/// Check if White has insufficient material to mate Black.
pub(crate) fn insufficient_white_material(board: &Board) -> bool {
    let white = board.color_combined(Color::White);
    let black = board.color_combined(Color::Black);

    if white.popcnt() == 1 {
        return true;
    }

    let minor_pieces = board.pieces(Piece::Knight) | board.pieces(Piece::Bishop);

    if white.popcnt() == 2
        && white & board.pieces(Piece::Knight) != EMPTY
        && black & (minor_pieces | board.pieces(Piece::Rook) | board.pieces(Piece::Pawn)) == EMPTY
    {
        return true;
    }

    let white_bishops = white & board.pieces(Piece::Bishop);
    if white_bishops != EMPTY {
        let bishops_color =
            if DARK_SQUARES & white_bishops != EMPTY { DARK_SQUARES } else { LIGHT_SQUARES };

        if white.popcnt() == white_bishops.popcnt() + 1
            && !bishops_color & board.pieces(Piece::Bishop) == EMPTY
            && *board.pieces(Piece::Knight) == EMPTY
            && *board.pieces(Piece::Pawn) == EMPTY
        {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Move queries.
// ---------------------------------------------------------------------------

/// Check if a move is a capture.
pub(crate) fn is_capture(board: &Board, m: ChessMove) -> bool {
    board.piece_on(m.get_dest()).is_some()
}

/// Check if a move is a white pawn advance (moving toward rank 8).
pub(crate) fn is_pawn_advance(board: &Board, m: ChessMove) -> bool {
    if board.piece_on(m.get_source()) != Some(Piece::Pawn) {
        return false;
    }
    if board.color_on(m.get_source()) == Some(Color::White) {
        m.get_dest().get_rank() > m.get_source().get_rank()
    } else {
        false
    }
}

/// Returns the set of legal moves from `board` that immediately lead to a
/// statically unwinnable position.
pub(crate) fn compute_punishable(board: &Board) -> FxHashSet<ChessMove> {
    MoveGen::new_legal(board)
        .filter(|&m| is_statically_unwinnable(&board.make_move_new(m)))
        .collect()
}

// ---------------------------------------------------------------------------
// Distance helpers.
// ---------------------------------------------------------------------------

/// Check if a move brings a piece closer to the target square.
///
/// Returns the change in distance to `target` after the move:
/// negative = moving closer, positive = moving farther, 0 = same distance,
/// unknown piece, or unreachable.
pub(crate) fn moves_toward_square(m: ChessMove, piece: Piece, target: Square) -> i8 {
    let (dist_src, dist_dst) = match piece {
        Piece::King => (king_distance(m.get_source(), target), king_distance(m.get_dest(), target)),
        Piece::Knight => {
            (knight_distance(m.get_source(), target), knight_distance(m.get_dest(), target))
        }
        _ => return 0,
    };
    dist_dst as i8 - dist_src as i8
}

/// Chebyshev distance (king distance) between two squares.
fn king_distance(a: Square, b: Square) -> u8 {
    let file_diff = (a.get_file().to_index() as i8 - b.get_file().to_index() as i8).abs();
    let rank_diff = (a.get_rank().to_index() as i8 - b.get_rank().to_index() as i8).abs();
    file_diff.max(rank_diff) as u8
}

/// Knight distance using precomputed table.
fn knight_distance(a: Square, b: Square) -> u8 {
    KNIGHT_DISTANCE[a.to_index()][b.to_index()]
}

/// Precomputed knight-move distance between every pair of squares, indexed
/// by `[a.to_index()][b.to_index()]`. Backs [`knight_distance`].
static KNIGHT_DISTANCE: LazyLock<[[u8; 64]; 64]> = LazyLock::new(|| {
    let mut table = [[0u8; 64]; 64];
    for sq_i in ALL_SQUARES.iter() {
        for sq_j in ALL_SQUARES.iter() {
            table[sq_i.to_index()][sq_j.to_index()] = compute_knight_distance(*sq_i, *sq_j);
        }
    }
    table
});

/// Computes the knight-move distance between `x` and `y` from the
/// (file, rank) deltas, using the standard closed-form formula, with a
/// special case for a (1, 1) delta next to a corner square (see
/// [`is_corner`]): the formula would say 2, but a corner's reduced mobility
/// actually forces 4.
fn compute_knight_distance(x: Square, y: Square) -> u8 {
    let file_dist =
        (x.get_file().to_index() as i8 - y.get_file().to_index() as i8).unsigned_abs() as usize;
    let rank_dist =
        (x.get_rank().to_index() as i8 - y.get_rank().to_index() as i8).unsigned_abs() as usize;

    let (min_dist, max_dist) =
        if file_dist < rank_dist { (file_dist, rank_dist) } else { (rank_dist, file_dist) };

    if min_dist == 1 && max_dist == 1 && (is_corner(x) || is_corner(y)) {
        return 4;
    }

    if min_dist % 2 == max_dist % 2 {
        match (min_dist, max_dist) {
            (0, 0) => 0,
            (0, 2) => 2,
            (0, 4) => 2,
            (2, 4) => 2,
            (1, 1) => 2,
            (1, 3) => 2,
            (3, 3) => 2,
            (7, 7) => 6,
            _ => 4,
        }
    } else {
        match (min_dist, max_dist) {
            (_, 7) => 5,
            (1, 2) => 1,
            (5, 6) => 5,
            _ => 3,
        }
    }
}

/// True for the four corner squares, where a knight takes longer to
/// maneuver than the generic distance formula above accounts for.
fn is_corner(s: Square) -> bool {
    matches!(s, Square::A1 | Square::H1 | Square::A8 | Square::H8)
}

/// Computes bishop distances from the target corner, measured in bishop moves
/// through squares outside `steady`. Squares unreachable from the corner get
/// distance `u8::MAX`. The target corner is determined by the color of White's
/// bishops (dark-square bishops aim for H8; light-square for A8).
pub(crate) fn compute_bishop_distances(board: &Board, steady: BitBoard) -> [u8; 64] {
    let bishops = board.pieces(Piece::Bishop);
    let white_bishops = board.color_combined(Color::White) & bishops;
    let black_bishops = board.color_combined(Color::Black) & bishops;
    let dark_corner = (white_bishops & DARK_SQUARES) != EMPTY
        || (white_bishops == EMPTY && (black_bishops & LIGHT_SQUARES) != EMPTY);

    let corner = if dark_corner {
        BitBoard::from_square(Square::H8)
            | BitBoard::from_square(Square::G8)
            | BitBoard::from_square(Square::H7)
    } else {
        BitBoard::from_square(Square::A8)
            | BitBoard::from_square(Square::B8)
            | BitBoard::from_square(Square::A7)
    };

    let mut distances = [u8::MAX; 64];
    let mut visited = EMPTY;
    let mut active = corner;
    let mut d = 0u8;
    loop {
        for s in active {
            distances[s.to_index()] = d;
        }
        visited |= active;
        d += 1;
        let mut next = EMPTY;
        for s in active {
            next |= get_bishop_moves(s, steady) & !steady;
        }
        active = next & !visited;
        if active == EMPTY {
            break;
        }
    }
    distances
}

/// Mirrors a square horizontally (file A becomes H, B becomes G, etc.),
/// preserving rank.
pub(crate) fn flip_square_file(sq: Square) -> Square {
    Square::make_square(sq.get_rank(), File::from_index(7 - sq.get_file().to_index()))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use chess::{Board, Square};

    use super::*;

    #[test]
    fn test_king_distance() {
        assert_eq!(king_distance(Square::A1, Square::A1), 0);
        assert_eq!(king_distance(Square::A1, Square::A2), 1);
        assert_eq!(king_distance(Square::A1, Square::B2), 1);
        assert_eq!(king_distance(Square::A1, Square::H8), 7);
    }

    #[test]
    fn test_insufficient_material() {
        let board = Board::from_str("7k/8/8/8/8/8/8/K7 w - - 0 1").unwrap();
        assert!(insufficient_white_material(&board));

        let board = Board::from_str("7k/8/8/8/8/8/8/KN6 w - - 0 1").unwrap();
        assert!(insufficient_white_material(&board));

        let board = Board::from_str("7k/8/8/8/8/8/8/KQ6 w - - 0 1").unwrap();
        assert!(!insufficient_white_material(&board));
    }
}
