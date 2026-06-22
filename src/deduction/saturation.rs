//! The saturation algorithm: a fixpoint iteration that grows each piece's
//! reachable set (and the steady/clear/capturable/attacked/sacrificable state
//! derived from it) until no further progress can be made.
//!
//! Each iteration computes candidate moves for every piece and discards any
//! that [`Analysis::filter_targets`] rules out, so reachable sets only ever
//! grow (guaranteeing termination).
//!
//! At the fixpoint, the over/under-approximation guarantees documented on
//! [`Analysis`] hold.

use chess::{
    get_bishop_moves, get_file, get_king_moves, get_pawn_attacks, get_rook_moves, line, BitBoard,
    Color, Piece, Square, EMPTY, NUM_COLORS,
};

use super::{
    analysis::Analysis,
    utils::{get_piece_attacks, get_piece_moves},
};
use crate::utils::PROMOTION_RANKS;

impl Analysis {
    /// Recompute the attacked squares for both colors.
    ///
    /// Only considers pieces that are not (over-approximately) capturable,
    /// i.e. pieces guaranteed to still be on the board. For each such piece,
    /// a square only counts as attacked if it is attacked from *every* square
    /// in the piece's `reachable` set, since the piece may end up on any of
    /// them.
    pub(crate) fn update_attacked_squares(&mut self) {
        self.attacked = [EMPTY; NUM_COLORS];
        for s in *self.board.combined() & !self.capturable {
            let piece = self.board.piece_on(s).unwrap();
            let color = self.board.color_on(s).unwrap();
            let mut attacked = !EMPTY;
            let mut potential_obstacles = !self.clear;

            // Same-color bishops are transparent to each other: remove them from
            // potential_obstacles so a bishop's attack ray is not blocked by a friendly
            // bishop. This is safe for a same-color bishop on square b when
            // both conditions hold:
            //  1. No non-bishop of the same color can ever reach b
            //     (`!reachable_by_non_bishop`), so b is never occupied by a non-transparent
            //     friendly piece.
            //  2. No opponent piece can ever reach b (`!self.capturable`). Since every
            //     piece's origin square is always in its own `reachable` set, b ∉
            //     capturable guarantees that no opponent has b in its reachable squares,
            //     i.e. the opponent can never arrive at b, whether by capture or by
            //     movement. Together these ensure b is only ever occupied by same-color
            //     bishops, so treating it as transparent is sound.
            if piece == Piece::Bishop {
                potential_obstacles &= !(self.board.color_combined(color)
                    & self.board.pieces(Piece::Bishop)
                    & !self.reachable_by_non_bishop[color.to_index()]
                    & !self.capturable);
            }

            for t in self.reachable[s.to_index()] {
                attacked &= get_piece_attacks(piece, color, t, potential_obstacles);
            }

            self.attacked[color.to_index()] |= attacked;
        }
    }

    /// Performs a fixpoint iteration to saturate the analysis state.
    ///
    /// This method iteratively expands the reachable squares for each piece
    /// and updates every other field derived from them (steady, clear,
    /// capturable, movable, sacrificable, attacked, promotable) until no more
    /// progress can be made (the fixpoint is reached).
    pub(crate) fn saturate(&mut self) {
        loop {
            let mut progress = false;

            for s in *self.board.combined() {
                let piece = self.board.piece_on(s).unwrap();
                let color = self.board.color_on(s).unwrap();

                for source in self.reachable[s.to_index()] {
                    let mut targets = get_piece_moves(piece, color, source, self.steady);

                    // Do not consider pawn attacks if White has one only non-king piece left.
                    if piece == Piece::Pawn
                        && (color == Color::White
                            || self.board.color_combined(Color::White).popcnt() > 2)
                    {
                        targets |=
                            get_pawn_attacks(source, color, self.sacrificable[(!color).to_index()]);
                    }

                    targets = self.filter_targets(s, piece, color, source, targets);

                    self.update_reachable(s, targets, &mut progress);
                    self.update_movable(s, targets, &mut progress);
                }

                let reachable = self.reachable[s.to_index()];

                // Mark s as non-steady if the piece can move somewhere.
                let can_move_elsewhere = reachable & !BitBoard::from_square(s) != EMPTY;
                if can_move_elsewhere {
                    self.update_non_steady(BitBoard::from_square(s), &mut progress);
                }

                let mut capturable = EMPTY;
                let mut mask = !self.board.pieces(Piece::King);
                if piece == Piece::Pawn {
                    mask &= !get_file(s.get_file());
                }

                if color == Color::White || self.board.color_combined(Color::White).popcnt() > 2 {
                    for opp in *self.board.color_combined(!color) & mask {
                        if reachable & self.reachable[opp.to_index()] != EMPTY {
                            capturable |= BitBoard::from_square(opp);
                        }
                    }
                }
                self.update_capturable(capturable, &mut progress);

                // Mark squares as non-steady if they can be reached (captured).
                self.update_non_steady(
                    reachable & !BitBoard::from_square(s) & !self.board.pieces(Piece::King),
                    &mut progress,
                );

                // Mark reachable squares as non-clear.
                self.update_non_clear(reachable, &mut progress);

                // Track non-king pieces as potentially sacrificable.
                if piece != Piece::King {
                    self.update_sacrificable(color, reachable, &mut progress);
                }

                // If a pawn can promote, it can potentially reach any square except those
                // permanently blocked by steady same-color pieces (can't capture own pieces)
                // or steady kings (kings cannot be captured to become non-steady).
                if piece == Piece::Pawn && self.reachable[s.to_index()] & PROMOTION_RANKS != EMPTY {
                    self.promotable = true;
                    let unreachable_after_promotion = self.steady
                        & (self.board.color_combined(color) | self.board.pieces(Piece::King));
                    self.update_reachable(s, !EMPTY & !unreachable_after_promotion, &mut progress);
                }
            }

            self.update_attacked_squares();

            if !progress {
                break;
            }
        }
    }

    /// Narrows `targets` (candidate destinations for `piece`, which started at
    /// `initial_square` and is now at `source`, about to move to the target).
    /// We rule out moves that are illegal or that the position's structure
    /// makes pointless to consider.
    ///
    /// See the inline comments below for each individual rule.
    fn filter_targets(
        &self,
        initial_square: Square,
        piece: Piece,
        color: Color,
        source: Square,
        mut targets: BitBoard,
    ) -> BitBoard {
        // Focus only on new targets.
        targets &= !self.reachable[initial_square.to_index()];

        // Filter out steady friendly pieces.
        let steady_friends = self.board.color_combined(color) & self.steady;
        targets &= !steady_friends;

        // Filter attacked squares for king safety.
        if piece == Piece::King {
            targets &= !self.attacked[(!color).to_index()];
        }

        let white_king = self.board.color_combined(Color::White) & self.board.pieces(Piece::King);

        // Filter targets if we are mating the white king.
        if color == Color::Black && white_king & self.steady != EMPTY {
            // Direct check: the piece moving to `t` attacks the white king.
            for t in targets {
                if white_king & get_piece_attacks(piece, color, t, !self.clear) != EMPTY
                    && BitBoard::from_square(t) & self.sacrificable[0] == EMPTY
                {
                    targets &= !BitBoard::from_square(t);
                }
            }

            // Discovered check: moving from `source` to `t` opens a line for a steady black
            // piece to check the white king.
            let white_king_sq = white_king.to_square();
            let blockers = !self.clear & !BitBoard::from_square(source);
            let potential_checkers = self.board.color_combined(Color::Black)
                & self.steady
                & !BitBoard::from_square(source)
                & line(white_king_sq, source);
            for t in targets {
                for checker_sq in potential_checkers {
                    if get_piece_attacks(
                        self.board.piece_on(checker_sq).unwrap(),
                        Color::Black,
                        checker_sq,
                        blockers | BitBoard::from_square(t),
                    ) & white_king
                        != EMPTY
                    {
                        targets &= !BitBoard::from_square(t);
                        break;
                    }
                }
            }
        }

        // Filter targets if checking a steady opponent king at distance 1 with our last
        // remaining piece; since they will have to capture our piece,
        // leaving us with no material; unless we are checking from a square
        // that can be defended by our king, in which case we may be delivering
        // mate.
        let black_king = self.board.color_combined(Color::Black) & self.board.pieces(Piece::King);
        if color == Color::White && self.board.color_combined(Color::White).popcnt() == 2 {
            for t in targets {
                if self.reachable[white_king.to_square().to_index()] & get_king_moves(t) != EMPTY {
                    // If there is an opponent steady piece attacking t, we are not going to be
                    // checkmating anyway, so we can maybe go to target.
                    if (self.steady & self.board.color_combined(Color::Black) & !black_king).all(
                        |attacker_sq| {
                            get_piece_attacks(
                                self.board.piece_on(attacker_sq).unwrap(),
                                Color::Black,
                                attacker_sq,
                                !self.clear,
                            ) & BitBoard::from_square(t)
                                == EMPTY
                        },
                    ) {
                        continue;
                    }
                }

                if black_king & self.steady != EMPTY
                    && get_piece_attacks(piece, color, t, !self.clear)
                        & black_king
                        & get_king_moves(t)
                        != EMPTY
                {
                    targets &= !BitBoard::from_square(t);
                }
            }
        }

        // Filter targets if we are stalemating the opponent.
        //
        // Only done on king moves because their attack set from any square is exact;
        // for other pieces it is an over-approximation, so filtering could incorrectly
        // shrink `reachable` and produce unsound results.
        if piece == Piece::King {
            let opp = self.board.color_combined(!color);
            let opp_king = opp & self.board.pieces(Piece::King);
            let opp_can_only_move_king = !opp_king & opp & self.movable == EMPTY;
            let opp_king = opp_king.to_square();

            // The opponent king's reachable squares, plus its neighbors not yet proven
            // attacked or steady. The neighbors avoid a deadlock: relying on `reachable`
            // alone, this rule and its symmetric copy for the opponent's king could each
            // wait on the other's freedom being established first.
            let opp_region = self.reachable[opp_king.to_index()]
                | (get_king_moves(opp_king) & !self.attacked[color.to_index()] & !self.steady);

            let diag_sliders = self.board.color_combined(color)
                & (self.board.pieces(Piece::Bishop) | self.board.pieces(Piece::Queen));

            let line_sliders = self.board.color_combined(color)
                & (self.board.pieces(Piece::Rook) | self.board.pieces(Piece::Queen));

            if opp_can_only_move_king {
                for t in targets {
                    let attacked_from_target = get_king_moves(t) | BitBoard::from_square(t);

                    // If `t` is far from the opponent king, this rule does not apply.
                    if get_king_moves(opp_king) & attacked_from_target == EMPTY {
                        continue;
                    }

                    let blockers = !self.clear | BitBoard::from_square(t);

                    // Don't treat this as a stalemate if vacating `source` opens a line for a
                    // friendly slider to give check instead -- that would be checkmate, not a
                    // draw.
                    let diags = get_bishop_moves(source, blockers) & !blockers;
                    if diag_sliders
                        .into_iter()
                        .any(|s| self.reachable[s.to_index()] & diags != EMPTY)
                    {
                        continue;
                    };
                    let lines = get_rook_moves(source, blockers) & !blockers;
                    if line_sliders
                        .into_iter()
                        .any(|s| self.reachable[s.to_index()] & lines != EMPTY)
                    {
                        continue;
                    };

                    // If our king move to `t` leaves the opponent king with at most 1 free
                    // square, we are stalemating them, so we filter out `t`.
                    if (opp_region & !attacked_from_target).popcnt() <= 1 {
                        targets &= !BitBoard::from_square(t);
                    }
                }
            }
        }

        // Filter targets where there is a piece that cannot move out of the way,
        // and cannot be captured.
        for t in targets {
            let the_way = chess::between(source, t)
                | BitBoard::from_square(source)
                | BitBoard::from_square(t);

            for obstacle in *self.board.combined()
                & the_way
                & !BitBoard::from_square(initial_square)
                & !self.capturable
            {
                let mut the_real_way = the_way;

                if self.board.color_on(obstacle).unwrap() != color
                    && (self.board.piece_on(obstacle).unwrap() != Piece::Pawn
                        || piece != Piece::Pawn
                        || source.get_file() != t.get_file())
                {
                    the_real_way &= !BitBoard::from_square(obstacle);
                }

                if piece == Piece::Pawn
                    && self.board.piece_on(obstacle).unwrap() == Piece::Pawn
                    && self.reachable[initial_square.to_index()] & !get_file(t.get_file()) == EMPTY
                {
                    the_real_way |= chess::line(initial_square, obstacle);
                }

                if self.reachable[obstacle.to_index()] & !the_real_way == EMPTY {
                    targets &= !BitBoard::from_square(t);
                }
            }

            // Filter uncapturable while steady if any.
            if let Some(uncapturable) = self.uncapturable_while_steady {
                targets &= !(uncapturable & self.steady);
            }
        }

        if let Some(squares) = self.capturable_only_while_steady {
            if BitBoard::from_square(initial_square) & squares != EMPTY {
                targets = EMPTY;
            }
        }

        targets
    }

    /// Update the reachable squares of the piece that started on `square`.
    /// Also folds `reachable` into `reachable_by_non_bishop` for that piece's
    /// color, unless the piece is a bishop.
    fn update_reachable(&mut self, square: Square, reachable: BitBoard, progress: &mut bool) {
        let idx = square.to_index();
        Self::update_field(&mut self.reachable[idx], |curr| curr | reachable, progress);
        if self.board.piece_on(square) != Some(Piece::Bishop) {
            let color = self.board.color_on(square).unwrap();
            Self::update_field(
                &mut self.reachable_by_non_bishop[color.to_index()],
                |curr| curr | reachable,
                progress,
            );
        }
    }

    /// Mark the piece on `s` as movable if it can reach any square other than
    /// `s`. Used by the stalemate filter to detect whether the opponent has
    /// any non-king pieces that can still move.
    fn update_movable(&mut self, s: Square, targets: BitBoard, progress: &mut bool) {
        if targets & !BitBoard::from_square(s) != EMPTY {
            Self::update_field(&mut self.movable, |curr| curr | BitBoard::from_square(s), progress);
        }
    }

    /// Update the steady squares by removing the given non-steady squares.
    fn update_non_steady(&mut self, non_steady: BitBoard, progress: &mut bool) {
        Self::update_field(&mut self.steady, |curr| curr & !non_steady, progress);
    }

    /// Mark pieces as potentially capturable. Also records the first group of
    /// steady-and-capturable squares in `first_steady_capturable`.
    fn update_capturable(&mut self, capturable: BitBoard, progress: &mut bool) {
        if capturable & self.steady != EMPTY {
            match self.first_steady_capturable {
                None => self.first_steady_capturable = Some(capturable & self.steady),
                Some(b) if !self.promotable => {
                    self.first_steady_capturable = Some(b | (capturable & self.steady))
                }
                _ => (),
            }
        }
        Self::update_field(&mut self.capturable, |curr| curr | capturable, progress);
    }

    /// Update the sacrificable squares for the given color.
    fn update_sacrificable(&mut self, color: Color, reachable: BitBoard, progress: &mut bool) {
        let idx = color.to_index();
        Self::update_field(&mut self.sacrificable[idx], |curr| curr | reachable, progress);
    }

    /// Update the clear squares by removing the given occupable squares.
    fn update_non_clear(&mut self, occupable: BitBoard, progress: &mut bool) {
        Self::update_field(&mut self.clear, |curr| curr & !occupable, progress);
    }

    /// Helper to update a field if the computed new value differs from current.
    fn update_field<F>(current: &mut BitBoard, compute: F, progress: &mut bool)
    where
        F: FnOnce(BitBoard) -> BitBoard,
    {
        let new_value = compute(*current);
        if *current != new_value {
            *current = new_value;
            *progress = true;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::str::FromStr;

    use chess::{BitBoard, Board};

    use super::*;

    /// A position together with its expected post-saturation field values
    /// (as raw `BitBoard` bits) and ground-truth winnability.
    pub(crate) struct TestCase {
        pub(crate) fen: &'static str,
        pub(crate) steady: u64,
        pub(crate) clear: u64,
        pub(crate) attacked: (u64, u64),
        pub(crate) capturable: u64,
        pub(crate) winnable: bool,
    }

    /// Hand-verified saturation results for a range of positions, also used
    /// by `deduction::tests::test_statically_unwinnable`.
    const TEST_CASES: &[TestCase] = &[
        TestCase {
            fen: "8/1p6/kPp3p1/2P3p1/1pP3Pb/1P4p1/6P1/7K",
            steady: 569842407194624,
            clear: 281518488788992,
            attacked: (1419100238643200, 8021392859136),
            capturable: 70368744177664,
            winnable: false,
        },
        TestCase {
            fen: "7b/1k4bB/8/8/1p1p1p1p/1PpP1P1P/2P3K1/N7",
            steady: 2863531009,
            clear: 5310976,
            attacked: (1426719744, 5573120),
            capturable: 36028797018963968,
            winnable: true,
        },
        TestCase {
            fen: "7b/1k5B/8/8/1p1p1p1p/1PpP1P1P/2P3K1/N3b3",
            steady: 2863531008,
            clear: 0,
            attacked: (1426718720, 5573120),
            capturable: 36028797018963985,
            winnable: false,
        },
        TestCase {
            fen: "7b/1k4bB/8/4p3/1p1pPp1p/1PpP1P1P/2P1P1K1/N7",
            steady: 0,
            clear: 0,
            attacked: (0, 0),
            capturable: 9277415304234669057,
            winnable: true,
        },
        TestCase {
            fen: "4K3/8/8/8/8/p1p2p1p/P1pppp1P/bnrqkrnb",
            steady: 10861950,
            clear: 0,
            attacked: (4325376, 10845822),
            capturable: 0,
            winnable: false,
        },
        TestCase {
            fen: "4K3/8/8/8/8/p1p2p2/P1pppp2/bnrqkrnb",
            steady: 0,
            clear: 0,
            attacked: (0, 0),
            capturable: 2440431,
            winnable: true,
        },
        TestCase {
            fen: "k1bK4/1p1p4/1PpPp3/2P1Pp2/2p1pP2/2p1P3/2P5/8",
            steady: 0,
            clear: 0,
            attacked: (0, 0),
            capturable: 0x40a1e3434140400,
            winnable: true,
        },
        TestCase {
            fen: "Bb1k1b2/bKp1p1p1/1pP1P1P1/pP6/P5P1/8/8/8",
            steady: 2473978141211623424,
            clear: 9223557565466542079,
            attacked: (48137727165595648, 746940051698483200),
            capturable: 72057594037927936,
            winnable: false,
        },
        TestCase {
            fen: "Bb1k1b2/bKp1p1p1/1pP1P1P1/pP6/6P1/P7/8/8",
            steady: 2473978136899878912,
            clear: 9223557565466476543,
            attacked: (47856243598950400, 746940051664928768),
            capturable: 72057594037927936,
            winnable: true,
        },
        TestCase {
            fen: "k7/6p1/6P1/p1p1p1PK/P1P1P1P1/6PR/7P/8",
            steady: 18085133756137472,
            clear: 45212099231580159,
            attacked: (45212926914592768, 175922565087232),
            capturable: 0,
            winnable: false,
        },
        TestCase {
            fen: "5brk/4p1p1/3pP1P1/1B1P4/3p2p1/3P2p1/4K1P1/8",
            steady: 0,
            clear: 0,
            attacked: (0, 0),
            capturable: 6940143826963546112,
            winnable: true,
        },
        TestCase {
            fen: "7k/8/1p6/1Pp5/2Pp4/pB1Pp1p1/P1B1P1P1/1B1B2K1",
            steady: 2225000239360,
            clear: 5536525625856,
            attacked: (5540871275520, 21643962880), // White attacks are not tight here
            capturable: 0,
            winnable: false,
        },
        TestCase {
            fen: "7k/8/1p6/1Pp5/2Pp4/pB1Pp1p1/P1B1P1P1/3B2K1",
            steady: 0,
            clear: 0,
            attacked: (0, 0),
            capturable: 2225000371464,
            winnable: true,
        },
        TestCase {
            fen: "2b5/1p6/pPp3k1/2Pp3p/P2PpBpP/4P1P1/5K2/8",
            steady: 0,
            clear: 0,
            attacked: (0, 0),
            capturable: 288801628164718592,
            winnable: true,
        },
        TestCase {
            fen: "2b5/1p6/1Pp3k1/p1Pp3p/P2PpBpP/4P1P1/5K2/8",
            steady: 570156259475456,
            clear: 1108213235712,
            attacked: (1418742185590784, 5541961662464),
            capturable: 536870912,
            winnable: false,
        },
        TestCase {
            fen: "1k6/p1p1p1p1/P1P1P1P1/p1p1p1p1/8/8/P1P1P1P1/4K3",
            steady: 24018831508766720,
            clear: 48037663017533440,
            attacked: (47850746040811520, 186916976721920),
            capturable: 365072220160,
            winnable: false,
        },
        TestCase {
            fen: "8/2b5/8/4k2B/1p1p4/1P1Pp1p1/4P1P1/2b2BRK",
            steady: 173690912,
            clear: 1290,
            attacked: (363385024, 1419264),
            capturable: 549755813956,
            winnable: false,
        },
        TestCase {
            fen: "8/2b5/8/4k2B/1p1p4/1P1Pp1p1/4P1P1/2b1KBR1",
            steady: 0,
            clear: 0,
            attacked: (0, 0),
            capturable: 1126449836347492,
            winnable: true,
        },
        TestCase {
            fen: "1b2k3/8/1p2K3/bP6/Pp6/1Pp5/B1P5/NB6",
            steady: 2211958883586,
            clear: 0,
            attacked: (5506232616194, 2220531976704),
            capturable: 144115188075855873,
            winnable: false,
        },
        TestCase {
            fen: "1k6/pPp5/BpP4b/1P6/8/7B/8/5B1K",
            steady: 146093218084159488,
            clear: 72057594037927936,
            attacked: (363108226104819712, 506384499693584384),
            capturable: 140737488355328,
            winnable: false,
        },
        TestCase {
            fen: "8/8/1k1p1p2/3PpP2/3pKp2/b2P1P2/8/3B1B2",
            steady: 44221925425152,
            clear: 0,
            attacked: (92601578946560, 361453846528),
            capturable: 40,
            winnable: false,
        },
        TestCase {
            fen: "1b3kBR/4pP1P/1p1pP2P/1P1P4/8/K5p1/6P1/1B6",
            steady: 16190610028137283584,
            clear: 1516064996688134144,
            attacked: (5829932807375290368, 8102019800298946560),
            capturable: 4194304,
            winnable: true,
        },
        TestCase {
            fen: "1b6/6p1/k5P1/8/1p1p4/1P1Pp2p/4Pp1p/1B3Kbr",
            steady: 177909984,
            clear: 354765333,
            attacked: (354971760, 1435728),
            capturable: 70368744177664,
            winnable: false,
        },
        TestCase {
            fen: "8/1p3b2/pPp1p1p1/BkP1P1P1/1P6/BPB5/PKP5/8",
            steady: 658981160878080,
            clear: 12345959393014054912,
            attacked: (1594313452552192, 8448318111744),
            capturable: 0,
            winnable: true,
        },
        TestCase {
            fen: "k6B/1b4B1/2b2B2/4B3/3B4/1pB1B3/pP1B4/K7",
            steady: 131841,
            clear: 0,
            attacked: (328451, 1282),
            capturable: 9241421688591353856,
            winnable: true,
        },
        TestCase {
            fen: "N1b1N1N1/1pPpPpPp/1P1P1P1P/8/8/8/8/kb2B1K1",
            steady: 6196577054285103104,
            clear: 12250072461424459776,
            attacked: (12273903276444876800, 2908208255467520),
            capturable: 18,
            winnable: true,
        },
        TestCase {
            fen: "8/B4b2/2k5/1p1p4/1P1Pp1pB/1pp1P1P1/1p6/bK3b2",
            steady: 44465259011,
            clear: 17411412,
            attacked: (93012887303, 363335429),
            capturable: 281477124194304,
            winnable: false,
        },
        TestCase {
            fen: "8/2b4B/4k3/8/8/2p1p1p1/2PpP1P1/1B1K3n",
            steady: 5528712,
            clear: 41072,
            attacked: (11148316, 4237844),
            capturable: 36028797018963970,
            winnable: false,
        },
        TestCase {
            fen: "7k/5b2/p7/rbp5/p1p1p1p1/P1PpP1P1/PK1P4/RNB1B3",
            steady: 1122418625287,
            clear: 2862765224,
            attacked: (2853637895, 1108297257984),
            capturable: 0,
            winnable: false,
        },
        TestCase {
            fen: "8/8/7p/5p1P/5p1K/5Pp1/6P1/5kb1",
            steady: 141425226301440,
            clear: 70370086354944,
            attacked: (70372248518656, 276225368064),
            capturable: 64, // Not tight
            winnable: false,
        },
        TestCase {
            fen: "8/6pp/8/8/ppp5/bkp4Q/qnp2p2/n1K5",
            steady: 117901061,
            clear: 0, // Not tight (b1 will always be clear)
            attacked: (3598, 118427403),
            capturable: 54043195528454144,
            winnable: false,
        },
    ];

    /// Saturates every [`TEST_CASES`] position and runs `assert_fn` against
    /// each resulting `Analysis`/`TestCase` pair.
    pub(crate) fn run_analysis_test<F>(assert_fn: F)
    where
        F: Fn(&Analysis, &TestCase),
    {
        TEST_CASES.iter().for_each(|test_case| {
            let board = Board::from_str(&format!("{} w - -", test_case.fen)).unwrap();
            let mut analysis = Analysis::new(board);
            analysis.saturate();
            assert_fn(&analysis, test_case);
        })
    }

    /// Exposes [`TEST_CASES`] to `deduction::tests`.
    pub(crate) fn all_test_cases() -> &'static [TestCase] {
        TEST_CASES
    }

    #[test]
    fn test_steady() {
        run_analysis_test(|analysis, test_case| {
            assert_eq!(analysis.steady, BitBoard(test_case.steady));
        });
    }

    #[test]
    fn test_clear() {
        run_analysis_test(|analysis, test_case| {
            assert_eq!(analysis.clear, BitBoard(test_case.clear));
        });
    }

    #[test]
    fn test_attacked() {
        run_analysis_test(|analysis, test_case| {
            println!("expected:\n{}", BitBoard(test_case.attacked.0).reverse_colors());
            println!("got:\n{}", analysis.attacked[0].reverse_colors());
            assert_eq!(analysis.attacked[0], BitBoard(test_case.attacked.0));
            assert_eq!(analysis.attacked[1], BitBoard(test_case.attacked.1));
        });
    }

    #[test]
    fn test_capturable() {
        run_analysis_test(|analysis, test_case| {
            assert_eq!(analysis.capturable, BitBoard(test_case.capturable));
        });
    }
}
