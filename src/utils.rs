use chess::{BitBoard, Board, BoardBuilder, ChessMove, Color, Rank, Square, ALL_PIECES};

use crate::Winnability;

/// The light squares of the chess board.
pub(crate) const LIGHT_SQUARES: BitBoard = BitBoard(0x55AA55AA55AA55AA);

/// The dark squares of the chess board.
pub(crate) const DARK_SQUARES: BitBoard = BitBoard(0xAA55AA55AA55AA55);

/// The 1st and 8th ranks.
pub(crate) const PROMOTION_RANKS: BitBoard = BitBoard(0xFF000000000000FF);

/// Applies the color-mirror transformation: reflects all pieces across the
/// horizontal axis (rank 1 becomes rank 8, rank 2 becomes rank 7, etc) and
/// simultaneously swaps their colors. Side to move, castling rights, and the
/// en-passant file are all mirrored accordingly.
///
/// The transformation is an involution (applying it twice recovers the original
/// position) and preserves the game-theoretic value: if the position is
/// potentially winnable for White, then the mirrored position is potentially
/// winnable for Black. This lets the engine always analyze from the perspective
/// of the side to move by normalizing to White's point of view.
pub(crate) fn mirror_board(board: &Board) -> Board {
    let mut builder = BoardBuilder::new();

    for piece in ALL_PIECES.iter() {
        let white_pieces = *board.pieces(*piece) & *board.color_combined(Color::White);
        let flipped_white = white_pieces.reverse_colors();
        for square in flipped_white {
            builder.piece(square, *piece, Color::Black);
        }

        let black_pieces = *board.pieces(*piece) & *board.color_combined(Color::Black);
        let flipped_black = black_pieces.reverse_colors();
        for square in flipped_black {
            builder.piece(square, *piece, Color::White);
        }
    }

    builder.side_to_move(!board.side_to_move());
    builder.castle_rights(Color::White, board.castle_rights(Color::Black));
    builder.castle_rights(Color::Black, board.castle_rights(Color::White));

    if let Some(ep_square) = board.en_passant() {
        builder.en_passant(Some(ep_square.get_file()));
    }

    Board::try_from(builder).expect("Failed to mirror board")
}

/// Translates the move sequence inside a [`Winnability`] value from the
/// color-mirrored board back to the original board's coordinate system.
/// Non-`Winnable` variants are returned unchanged.
pub(crate) fn mirror_moves(winnability: Winnability) -> Winnability {
    // Reflects a square across the horizontal axis, keeping its file.
    fn mirror_square(sq: Square) -> Square {
        Square::make_square(Rank::from_index(7 - sq.get_rank().to_index()), sq.get_file())
    }

    // Applies mirror_square to both endpoints of a move; promotion piece is
    // unchanged.
    fn mirror_move(m: ChessMove) -> ChessMove {
        ChessMove::new(
            mirror_square(m.get_source()),
            mirror_square(m.get_dest()),
            m.get_promotion(),
        )
    }

    match winnability {
        Winnability::Winnable { helpmate } => {
            Winnability::Winnable { helpmate: helpmate.into_iter().map(mirror_move).collect() }
        }
        other => other,
    }
}
