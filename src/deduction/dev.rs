//! Debugging-only tooling for [`Analysis`].
//!
//! Not used anywhere in the library itself. Kept around so an `Analysis`
//! state can be printed while developing the saturation algorithm.

use std::fmt;

use chess::{BitBoard, EMPTY};

use super::analysis::Analysis;

impl fmt::Display for Analysis {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "FEN: {}", self.board)?;
        for s in *self.board.combined() {
            write_bitboard(f, &s.to_string(), self.reachable[s.to_index()])?;
        }

        writeln!(f, "Steady:\n{}\n", self.steady.reverse_colors())?;
        writeln!(f, "Clear:\n{}\n", self.clear.reverse_colors())?;
        writeln!(f, "Capturable:\n{}\n", self.capturable.reverse_colors())?;
        writeln!(f, "Movable:\n{}\n", self.movable.reverse_colors())?;
        writeln!(f, "W Sac:\n{}\n", self.sacrificable[0].reverse_colors())?;
        writeln!(f, "B Sac:\n{}\n", self.sacrificable[1].reverse_colors())?;
        writeln!(f, "W attacked:\n{}\n", self.attacked[0].reverse_colors())?;
        writeln!(f, "B attacked:\n{}\n", self.attacked[1].reverse_colors())?;
        if let Some(squares) = self.uncapturable_while_steady {
            writeln!(f, "uncapturable_while_steady:\n{}", squares.reverse_colors())?;
        }
        if let Some(squares) = self.capturable_only_while_steady {
            writeln!(f, "capturable_only_while_steady:\n{}", squares.reverse_colors())?;
        }
        if let Some(bb) = self.first_steady_capturable {
            writeln!(f, "first_steady_capturable:\n{}", bb.reverse_colors())?;
        }
        writeln!(f, "promotable: {}", self.promotable)?;
        Ok(())
    }
}

/// Writes `bitboard` as a labeled list of squares (e.g. `name: { a1 b2 }`),
/// or `name: ALL` if every square is set. When `bitboard` has 40 or more
/// squares set, lists its (shorter) complement instead, prefixed `!`.
fn write_bitboard(f: &mut fmt::Formatter, name: &str, bitboard: BitBoard) -> fmt::Result {
    if bitboard == !EMPTY {
        writeln!(f, "  {}: ALL", name)?;
    } else {
        let (negated, bb) = if bitboard.popcnt() < 40 { (" ", bitboard) } else { ("!", !bitboard) };
        write!(f, "  {}: {}{{ ", name, negated)?;
        for element in bb {
            write!(f, "{} ", element)?;
        }
        writeln!(f, "}}")?;
    }
    Ok(())
}
