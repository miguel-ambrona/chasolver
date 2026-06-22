use std::str::FromStr;

use chasolver::{is_unwinnable_fast, winnability, Winnability};
use chess::{Board, BoardStatus, Color};
use rayon::prelude::*;

/// Checks that `winnability(board, intended_winner)` agrees with
/// `expected_winnable`, and that a `Winnable` result's `helpmate` actually
/// leads to a checkmate by `intended_winner`.
///
/// Also checks that `is_unwinnable_fast` is sound: it is incomplete by
/// design, so it can't be expected to match `expected_winnable` exactly, but
/// it must never claim unwinnable for a position that's actually winnable.
fn check_winnability(board: &Board, intended_winner: Color, expected_winnable: bool) {
    match winnability(board, intended_winner) {
        Some(Winnability::Winnable { helpmate }) => {
            let mated_board = helpmate.iter().fold(*board, |b, &m| b.make_move_new(m));
            assert_eq!(mated_board.status(), BoardStatus::Checkmate);
            assert_eq!(mated_board.side_to_move(), !intended_winner);
            assert!(expected_winnable);
        }
        Some(Winnability::Unwinnable) => assert!(!expected_winnable),
        None => panic!("Undetermined: {board}"),
    }

    if is_unwinnable_fast(board, intended_winner) {
        assert!(!expected_winnable, "is_unwinnable_fast wrongly claimed unwinnable: {board}");
    }
}

/// Checks every position in `data` against its expected classification.
///
/// Each line is supposed to be `<2-char classification> <FEN>`, where the first
/// character is `W` if White can win, `-` if not, and likewise for Black/the
/// second character.
fn check_positions(data: &str) {
    let entries: Vec<&str> =
        data.lines().filter(|line| !line.trim().is_empty() && !line.starts_with('#')).collect();

    entries.par_iter().for_each(|line| {
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        assert_eq!(parts.len(), 2, "Invalid line format: {}", line);

        let expected = parts[0];
        let fen = parts[1];

        let board = Board::from_str(fen).unwrap_or_else(|_| panic!("Failed to parse FEN: {}", fen));

        check_winnability(&board, Color::White, expected.starts_with("W"));
        check_winnability(&board, Color::Black, expected.ends_with("B"));
    });
}

#[test]
fn test_positions() {
    const POSITIONS: &str = include_str!("positions.txt");
    check_positions(POSITIONS);
}

#[test]
fn test_lichess() {
    const LICHESS: &str = include_str!("lichess.txt");
    check_positions(LICHESS);
}
