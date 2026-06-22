//! The `Analysis` struct: per-position state tracked during static analysis,
//! plus the queries it answers once that state has saturated (e.g.
//! [`Analysis::black_could_get_mated`]). See [`super::saturation`] for the
//! fixpoint algorithm that populates this state.

use chess::{get_king_moves, BitBoard, Board, Color, Piece, EMPTY, NUM_COLORS, NUM_SQUARES};

use super::utils::{get_piece_attacks, subsets_of_size};

/// Most fields below are *approximations* of some true game-theoretic
/// property of the position (e.g. "this piece can reach that square").
///
/// - An **over-approximation** may wrongly *include* an element (false
///   positive) but never wrongly excludes one. The safe inference runs on
///   absence: if `x` is not in the field, `x` definitely lacks the property.
///
/// - An **under-approximation** may wrongly *exclude* an element (false
///   negative) but never wrongly includes one. The safe inference runs on
///   presence: if `x` is in the field, `x` definitely has the property.
///
/// Both kinds only reach their stated guarantee *after saturation completes*
/// ([`Analysis::saturate`]); mid-saturation, neither direction can be relied
/// on.
pub(crate) struct Analysis {
    /// The position being analyzed.
    pub(crate) board: Board,

    /// Over-approximation of the squares that may be reachable by the pieces
    /// that are still on the board.
    ///
    /// For `s : Square`, `reachable[s.to_index()]` is a `BitBoard` encoding the
    /// squares that the piece currently on `s` may reach in the future.
    pub(crate) reachable: [BitBoard; NUM_SQUARES],

    /// Under-approximation of the squares where there are steady pieces (that
    /// will never be able to move, nor get captured, in the remaining game).
    ///
    /// Computed by starting from "every occupied square" and removing a
    /// square whenever the (over-approximated) `reachable` field suggests a
    /// way to move or capture that piece.
    pub(crate) steady: BitBoard,

    /// Under-approximation of the squares that are definitely clear (no piece
    /// will ever be able to move there in the remaining game).
    pub(crate) clear: BitBoard,

    /// Under-approximation of the squares that will always be attacked for the
    /// rest of the game, by each color. E.g.,
    /// `attacked[Color::White.to_index()]` contains the squares that will
    /// always be attacked by white pieces.
    pub(crate) attacked: [BitBoard; NUM_COLORS],

    /// Over-approximation of the pieces that may be captured at some point in
    /// the remaining game.
    pub(crate) capturable: BitBoard,

    /// Over-approximation of the pieces that may move somewhere other than
    /// their current square. Used by the stalemate filter to detect whether
    /// the opponent has any non-king pieces that can still move.
    pub(crate) movable: BitBoard,

    /// Over-approximation of the squares that can be reached by non-bishops of
    /// each color.
    ///
    /// This is in fact `reachable` restricted to non-bishop pieces.
    pub(crate) reachable_by_non_bishop: [BitBoard; NUM_COLORS],

    /// Over-approximation of the squares where pieces can be sacrificed by each
    /// color.
    ///
    /// This is the union of `reachable` over all non-king pieces of that color.
    pub(crate) sacrificable: [BitBoard; NUM_COLORS],

    /// If set, pieces on these squares cannot be captured while they are steady
    /// (i.e., before they have moved away from their starting square).
    ///
    /// Not an approximation: a caller-supplied assumption used to drive
    /// case-splits in `is_statically_unwinnable`, not a value saturation
    /// derives on its own.
    pub(crate) uncapturable_while_steady: Option<BitBoard>,

    /// If set, pieces on these squares can only be captured while still steady.
    /// They are treated as immovable: they stay in place and can only be taken
    /// there.
    ///
    /// Not an approximation: same role as [`Self::uncapturable_while_steady`],
    /// a caller-supplied assumption rather than a derived value.
    pub(crate) capturable_only_while_steady: Option<BitBoard>,

    /// The first group of squares found during saturation that are both steady
    /// and capturable. Used by `is_statically_unwinnable` to branch on the fate
    /// of such pieces. Only populated before any pawn promotion is detected.
    ///
    /// Neither an over- nor under-approximation: it intersects an
    /// under-approximation (`steady`) with an over-approximation
    /// (`capturable`), so it can both miss real steady-and-capturable squares
    /// and include spurious ones. That's fine, it only picks which case-split
    /// to try next, and is never relied on to justify a verdict on its own.
    pub(crate) first_steady_capturable: Option<BitBoard>,

    /// Over-approximation on whether some pawn can reach the promotion rank.
    /// If `true` does not mean possible, but plausible.
    /// After saturation, `false` means no promotion is possible from `board`.
    pub(crate) promotable: bool,
}

impl Analysis {
    /// Creates a new unwinnability analysis for the given board.
    pub(crate) fn new(board: Board) -> Self {
        let mut analysis = Self {
            board,
            reachable: [EMPTY; NUM_SQUARES],
            steady: *board.combined(),
            clear: !*board.combined(),
            attacked: [EMPTY; NUM_COLORS],
            sacrificable: [EMPTY; NUM_COLORS],
            capturable: EMPTY,
            movable: EMPTY,
            reachable_by_non_bishop: [EMPTY; NUM_COLORS],
            uncapturable_while_steady: None,
            capturable_only_while_steady: None,
            first_steady_capturable: None,
            promotable: false,
        };
        for s in *board.combined() {
            analysis.reachable[s.to_index()] |= BitBoard::from_square(s);
        }
        analysis.update_attacked_squares();
        analysis
    }

    /// Returns `true` if there is at least one square where Black's king could
    /// be checkmated: a white piece can reach an attacking square, and all king
    /// escape squares can be covered by black pieces or the white king.
    ///
    /// Must be called after saturation.
    pub(crate) fn black_could_get_mated(&self) -> bool {
        let white_king = self.board.color_combined(Color::White) & self.board.pieces(Piece::King);
        let white_king_reachable = self.reachable[white_king.to_square().to_index()];

        let black_king = self.board.color_combined(Color::Black) & self.board.pieces(Piece::King);
        let black_king_region = self.reachable[black_king.to_square().to_index()];

        let (visitors, visited_region) = self.visitors(black_king_region);

        // We need at least one visitor to deliver mate.
        if visitors == EMPTY {
            return false;
        }

        // For every candidate mating square:
        for mating_square in visited_region {
            let escaping_squares =
                get_king_moves(mating_square) & black_king_region & !visited_region;
            let (blockers, blocking_region) = self.blockers(escaping_squares);
            let n = blockers.popcnt().min(blocking_region.popcnt());

            // Try all ways to assign n black pieces to n squares in blocking_region.
            for covered_by_black in subsets_of_size(blocking_region, n) {
                let remaining = escaping_squares & !covered_by_black;
                // Check if the white king can cover all remaining squares from one position.
                if remaining == EMPTY {
                    return true;
                }
                let candidate_king_squares =
                    remaining.into_iter().fold(EMPTY, |acc, r| acc | get_king_moves(r))
                        & !(BitBoard::from_square(mating_square) | get_king_moves(mating_square));
                if (candidate_king_squares & white_king_reachable)
                    .into_iter()
                    .any(|s| get_king_moves(s) & remaining == remaining)
                {
                    return true;
                }
            }
        }

        false
    }

    /// Returns the white non-king pieces that can attack `region` (visitors),
    /// and the subset of `region` they can attack (visited_region). Only pieces
    /// that can move away from their starting square are considered.
    ///
    /// Must be called after saturation.
    fn visitors(&self, region: BitBoard) -> (BitBoard, BitBoard) {
        let mut visitors = EMPTY;
        let mut visited_region = EMPTY;
        for s in *self.board.color_combined(Color::White) {
            let piece = self.board.piece_on(s).unwrap();
            if piece == Piece::King {
                continue;
            }
            if self.reachable[s.to_index()] == BitBoard::from_square(s) {
                continue;
            }
            for t in self.reachable[s.to_index()] {
                let attacked_by_piece = get_piece_attacks(piece, Color::White, t, !self.clear);
                if region & attacked_by_piece != EMPTY {
                    visitors |= BitBoard::from_square(s);
                    visited_region |= region & attacked_by_piece;
                }
            }
        }
        (visitors, visited_region)
    }

    /// Returns the black non-king pieces that can reach `region` (blockers),
    /// and the subset of `region` they can reach (blocking_region).
    ///
    /// Must be called after saturation.
    fn blockers(&self, region: BitBoard) -> (BitBoard, BitBoard) {
        let mut blockers = EMPTY;
        let mut blocking_region = EMPTY;
        for s in *self.board.color_combined(Color::Black) & !self.board.pieces(Piece::King) {
            let reached_by_blocker = self.reachable[s.to_index()];
            if region & reached_by_blocker != EMPTY {
                blockers |= BitBoard::from_square(s);
                blocking_region |= region & reached_by_blocker;
            }
        }
        (blockers, blocking_region)
    }
}
