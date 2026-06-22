use std::{
    io::{self, BufRead},
    str::FromStr,
    time::Instant,
};

use chasolver::{is_unwinnable_fast, winnability, Winnability};
use chess::{Board, Color};

/// Reads positions from stdin, one per line, in the form `<FEN> [white|black]`
/// (the trailing color sets the intended winner; if omitted, it defaults to the
/// side not to move).
///
/// Runs `winnability` (or `is_unwinnable_fast` under `--fast`) on each and
/// prints the verdict. Reports timing stats to stderr on exit.
///
/// Useful for batch analysis, e.g. benchmarking against a corpus of positions.
fn main() {
    eprintln!("CHA-Solver v{}", env!("CARGO_PKG_VERSION"));

    let skip_winnable = std::env::args().any(|a| a == "--skip-winnable");
    let fast_mode = std::env::args().any(|a| a == "--fast");

    let stdin = io::stdin();
    let mut stats = Stats::default();

    for line in stdin.lock().lines() {
        let line = line.expect("Failed to read line");
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "quit" {
            break;
        }

        let Some((board, winner)) = parse_line(line) else {
            eprintln!("Failed to parse: {}", line);
            continue;
        };

        let start = Instant::now();

        if fast_mode {
            let is_unwinnable = is_unwinnable_fast(&board, winner);
            stats.update(start.elapsed().as_nanos() as u64);
            if is_unwinnable {
                println!("unwinnable {}", line);
            }
            continue;
        }

        let result = winnability(&board, winner);
        stats.update(start.elapsed().as_nanos() as u64);
        match result {
            Some(Winnability::Winnable { helpmate }) => {
                if !skip_winnable {
                    let moves: Vec<_> = helpmate.iter().map(|m| m.to_string()).collect();
                    println!("winnable moves {} {}", moves.join(","), line)
                }
            }
            Some(Winnability::Unwinnable) => println!("unwinnable {}", line),
            None => println!("undetermined {}", line),
        }
    }

    stats.report();
}

/// Running count/sum/sum-of-squares/max of per-position analysis time, for
/// the summary printed when the input stream ends.
#[derive(Default)]
struct Stats {
    count: u64,
    total_ns: u64,
    total_ns_sq: u128,
    max_ns: u64,
}

impl Stats {
    /// Records one more position's elapsed time.
    fn update(&mut self, elapsed_ns: u64) {
        self.count += 1;
        self.total_ns += elapsed_ns;
        self.total_ns_sq += elapsed_ns as u128 * elapsed_ns as u128;
        self.max_ns = self.max_ns.max(elapsed_ns);
    }

    /// Prints count/average/std-dev/max to stderr. No-op if no positions
    /// were analyzed.
    fn report(&self) {
        if self.count == 0 {
            return;
        }
        let avg = self.total_ns / self.count;
        let variance = (self.total_ns_sq / self.count as u128).saturating_sub((avg as u128).pow(2));
        let std_dev = (variance as f64).sqrt();
        eprintln!(
            "Analyzed {} positions in {} (avg: {}; std: {}; max: {})",
            self.count,
            fmt_duration(self.total_ns as f64),
            fmt_duration(avg as f64),
            fmt_duration(std_dev),
            fmt_duration(self.max_ns as f64),
        );
    }
}

/// Largest-to-smallest unit thresholds used by [`fmt_duration`].
const DURATION_UNITS: &[(f64, &str)] = &[
    (3_600_000_000_000.0, "h"),
    (60_000_000_000.0, "min"),
    (1_000_000_000.0, "s"),
    (1_000_000.0, "ms"),
    (1_000.0, "μs"),
    (1.0, "ns"),
];

/// Formats a duration in nanoseconds with the largest unit (ns/μs/ms/s/min/h)
/// that keeps the value above 1, to one decimal place.
fn fmt_duration(ns: f64) -> String {
    let &(divisor, unit) =
        DURATION_UNITS.iter().find(|&&(d, _)| ns >= d).unwrap_or(&DURATION_UNITS[5]);
    format!("{:.1} {}", ns / divisor, unit)
}

/// Parses a `<FEN> [white|black]` input line. The trailing color sets the
/// intended winner; if omitted, it defaults to the side not to move.
fn parse_line(line: &str) -> Option<(Board, Color)> {
    let (fen, winner_color) = match line.rsplit_once(' ') {
        Some((rest, "white")) => (rest, Some(Color::White)),
        Some((rest, "black")) => (rest, Some(Color::Black)),
        _ => (line, None),
    };

    let board = Board::from_str(fen).ok()?;
    let winner = winner_color.unwrap_or(!board.side_to_move());

    Some((board, winner))
}
