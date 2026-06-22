//! Static analysis for helpmate winnability.
//!
//! Designed for White as the intended winner; see [`super::winnability`] for
//! how a Black-intended-winner query is mirrored before reaching this module.
//!
//! The central entry point is [`is_statically_unwinnable`], which runs
//! [`Analysis::saturate`] under various assumptions and checks whether Black's
//! king can never be mated.
//!
//! The analysis is *sound but incomplete*:
//! a `true` result guarantees unwinnability; `false` is inconclusive.

mod analysis;
mod dev;
mod saturation;
mod utils;

pub(crate) use analysis::Analysis;
use chess::{BitBoard, Board, Color, Piece};

/// Returns `true` if static analysis can prove the position is unwinnable for
/// White.
///
/// Runs [`Analysis::saturate`] under different assumptions about which pieces
/// can be captured, branching on the fate of pieces that are both steady and
/// capturable (`first_steady_capturable`). Returns `true` only if every
/// scenario rules out a checkmate.
pub(crate) fn is_statically_unwinnable(board: &Board) -> bool {
    if board.en_passant().is_some() {
        return false;
    }

    let mut analysis = Analysis::new(*board);

    let white = board.color_combined(Color::White);
    if white.popcnt() == 2 {
        let last_white_piece = white & !board.pieces(Piece::King);
        analysis.uncapturable_while_steady = Some(last_white_piece);
    }

    analysis.saturate();

    if !analysis.black_could_get_mated() {
        return true;
    }

    if let Some(squares) = analysis.first_steady_capturable {
        for s in squares {
            let mut analysis = Analysis::new(*board);
            analysis.uncapturable_while_steady = Some(BitBoard::from_square(s));
            analysis.saturate();

            if analysis.black_could_get_mated() {
                continue;
            }

            // Case B: the piece can only be captured while still on its starting square.
            // If this also rules out mate, no valid scenario exists and the position is
            // unwinnable.
            let mut analysis = Analysis::new(*board);
            analysis.capturable_only_while_steady = Some(BitBoard::from_square(s));
            analysis.saturate();

            if !analysis.black_could_get_mated() {
                return true;
            }
        }
    }

    // TODO: Generalize this for more than 2 squares (mainly aesthetic; could also
    // improve completeness).
    if let Some(squares) = analysis.first_steady_capturable {
        if squares.popcnt() == 2 {
            let s1 = squares.to_square();
            let s2 = (squares & !BitBoard::from_square(s1)).to_square();

            let mut analysis = Analysis::new(*board);
            analysis.uncapturable_while_steady = Some(squares);
            analysis.saturate();

            if analysis.black_could_get_mated() {
                return false;
            }

            let mut analysis = Analysis::new(*board);
            analysis.uncapturable_while_steady = Some(BitBoard::from_square(s1));
            analysis.capturable_only_while_steady = Some(BitBoard::from_square(s2));
            analysis.saturate();

            if analysis.black_could_get_mated() {
                return false;
            }

            let mut analysis = Analysis::new(*board);
            analysis.capturable_only_while_steady = Some(BitBoard::from_square(s1));
            analysis.uncapturable_while_steady = Some(BitBoard::from_square(s2));
            analysis.saturate();

            if analysis.black_could_get_mated() {
                return false;
            }

            let mut analysis = Analysis::new(*board);
            analysis.capturable_only_while_steady = Some(squares);
            analysis.saturate();

            return !analysis.black_could_get_mated();
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use chess::Board;

    use super::*;

    #[test]
    fn test_statically_unwinnable() {
        saturation::tests::all_test_cases().iter().for_each(|test_case| {
            let board = Board::from_str(&format!("{} w - -", test_case.fen)).unwrap();
            assert_eq!(is_statically_unwinnable(&board), !test_case.winnable);
        });

        let board = Board::from_str("B2b4/8/4k3/8/1p1p1p1p/1PpP1P1P/K1P4b/RB6 w - -").unwrap();
        assert!(is_statically_unwinnable(&board));

        let board = Board::from_str("1kb5/1p1p4/1P1P2p1/6p1/6Pb/6p1/6P1/7K w - -").unwrap();
        assert!(is_statically_unwinnable(&board));

        let board = Board::from_str("8/6R1/8/6p1/2N2pP1/PPPP4/QNP2P1N/1Bk1K2R b - g3").unwrap();
        assert!(!is_statically_unwinnable(&board));
    }
}
