//! A challenge for the reader. Consider this position:
//!
//! `6kB/p3p1P1/2p3P1/p7/8/4P3/PKP5/8 w - -`
//!
//! White ran out of time. Result?
//!
//! Composition by Miguel Ambrona, 2022, Madrid. Despite how few pieces are
//! left, this position is genuinely hard for humans to classify as winnable
//! or unwinnable at a glance. A good reminder that the unwinnability problem
//! is not nearly as easy as it looks, and that settling it reliably takes
//! careful study and an algorithm designed with just as much care.

use std::str::FromStr;

use chasolver::{winnability, Winnability};
use chess::{Board, Color};

fn main() {
    let fen = "6kB/p3p1P1/2p3P1/p7/8/4P3/PKP5/8 w - -";
    let board = Board::from_str(fen).unwrap();

    println!("Position: {fen}");
    println!("White ran out of time. Result?\n");

    // Per FIDE rules (Article 6.9), White loses on time unless Black cannot
    // possibly deliver checkmate, in which case it's a draw. So the ruling comes
    // down to whether a helpmate exists for Black.

    match winnability(&board, Color::Black) {
        Some(Winnability::Winnable { helpmate }) => {
            let moves: Vec<_> = helpmate.iter().map(|m| m.to_string()).collect();
            println!("Black wins! Here is a possible helpmate:");
            println!("{}", moves.join(" "));
        }
        Some(Winnability::Unwinnable) => println!("Draw: Black cannot possibly deliver checkmate."),
        None => println!("Undetermined."),
    }

    // SPOILER ALERT:
    //
    // Black has very few moves left before they get stalemated. Before that
    // happens, White must clear one of the files (by capturing a black pawn),
    // then under-promote their pawn from that file and sacrifice the promoted
    // piece on some of the black pawns to unlock Black.
    //
    // Given such hurry, such a race against the clock, the instinct is to act
    // immediately.
    //
    // There does actually exist a helpmate for Black. However, the only way
    // through is to wait patiently. The black c pawn must be captured on c3
    // (not before!), so that the white king can then move to d2, the only
    // square from where he will not receive checks when the black pawns
    // advance.
    //
    // Acting in a hurry achieves nothing here. The lesson is a broader one:
    // the more pressing the urge to act immediately, the more it pays to
    // stay patient.
}
