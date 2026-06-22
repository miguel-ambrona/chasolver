//! Piece move/attack generation and formatting helpers used by
//! [`super::Analysis`].

use chess::{
    get_bishop_moves, get_king_moves, get_knight_moves, get_pawn_attacks, get_pawn_quiets,
    get_rook_moves, BitBoard, Color, Piece, Square, EMPTY,
};

/// Squares `piece` on `square` can move to, given `blockers`: quiet pushes
/// for pawns, full move geometry for every other piece.
pub(crate) fn get_piece_moves(
    piece: Piece,
    color: Color,
    square: Square,
    blockers: BitBoard,
) -> BitBoard {
    match piece {
        Piece::King => get_king_moves(square),
        Piece::Queen => get_rook_moves(square, blockers) | get_bishop_moves(square, blockers),
        Piece::Rook => get_rook_moves(square, blockers),
        Piece::Bishop => get_bishop_moves(square, blockers),
        Piece::Knight => get_knight_moves(square),
        Piece::Pawn => get_pawn_quiets(square, color, blockers),
    }
}

/// Squares `piece` on `square` attacks, given `blockers`: diagonal captures
/// for pawns, full move geometry for every other piece.
pub(crate) fn get_piece_attacks(
    piece: Piece,
    color: Color,
    square: Square,
    blockers: BitBoard,
) -> BitBoard {
    match piece {
        Piece::King => get_king_moves(square) | BitBoard::from_square(square),
        Piece::Pawn => get_pawn_attacks(square, color, !EMPTY),
        _ => get_piece_moves(piece, color, square, blockers),
    }
}

/// Iterates over all subsets of `bb` with exactly `n` squares.
///
/// Uses Gosper's hack to enumerate only valid masks (O(C(k,n)) iterations).
/// Masks are `u64`, matching `BitBoard`'s own width, so this is safe for `bb`
/// with up to 64 squares set.
pub(crate) fn subsets_of_size(bb: BitBoard, n: u32) -> impl Iterator<Item = BitBoard> {
    let squares: Vec<Square> = bb.into_iter().collect();
    let k = bb.popcnt();
    std::iter::successors((n <= k).then(|| if n > 0 { (1u64 << n) - 1 } else { 0 }), move |&mask| {
        if n == 0 {
            return None;
        }
        let c = mask & mask.wrapping_neg();
        let r = mask + c;
        let next = (((r ^ mask) >> 2) / c) | r;
        (next < (1u64 << k)).then_some(next)
    })
    .map(move |mask| {
        squares
            .iter()
            .enumerate()
            .filter(|(i, _)| mask & (1u64 << i) != 0)
            .fold(EMPTY, |acc, (_, &sq)| acc | BitBoard::from_square(sq))
    })
}
