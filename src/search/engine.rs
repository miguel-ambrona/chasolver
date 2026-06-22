//! Search engine: state, core recursive search, and iteration strategies.

use chess::{Board, ChessMove, Color, MoveGen, EMPTY};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    heuristics::{classify_move, MoveVariation},
    utils::insufficient_white_material,
};

/// Mutable state threaded through a helpmate search.
///
/// Carries the depth/node budgets and the move stack that every search needs,
/// plus a handful of optional precomputed tables (transposition table,
/// punishable moves, bishop distances) that individual phases opt into by
/// filling them in before searching; left as `None` they're simply unused.
///
/// A "node" is one candidate move considered during the search (i.e. one
/// child position explored), counted once per loop iteration in
/// [`helpmate`], regardless of whether that move is then recursed into.
#[derive(Debug)]
pub(crate) struct Search {
    /// Depth (in plies) the current iterative-deepening pass searches to.
    pub(crate) max_depth: u8,

    /// Per-depth budget on the number of positions explored (nodes); see
    /// [`Search::is_depth_limit_reached`].
    pub(crate) depth_node_limit: u64,

    /// Budget on the number of positions explored (nodes), shared across all
    /// depths of an `iterative_deepening` call.
    pub(crate) global_node_limit: u64,

    /// Positions explored (nodes) in the current depth iteration; reset by
    /// [`Search::reset_iteration`].
    pub(crate) iteration_nodes: u64,

    /// Positions explored (nodes) within the current `iterative_deepening`
    /// call; checked against `global_node_limit`.
    pub(crate) global_nodes: u64,

    /// Set when a node/depth limit cuts a branch short, signalling that the
    /// branch was left unexplored rather than proven to have no mate.
    pub(crate) interrupted: bool,

    /// Move sequence from the search root to the current node.
    pub(crate) moves: Vec<ChessMove>,

    /// Whether the last move played was a [`MoveVariation::Reward`]; if so, the
    /// next move's depth increment is waived too, so the reward isn't
    /// immediately cancelled out.
    pub(crate) past_progress: bool,

    /// Transposition table mapping a position to the deepest `num_moves_left`
    /// it has already been searched at. `None` disables the table.
    pub(crate) transposition_table: Option<FxHashMap<Board, u8>>,

    /// Moves penalized unconditionally during search, classified as
    /// [`MoveVariation::Punish`]; see [`super::utils::compute_punishable`] for
    /// how this set is chosen.
    pub(crate) punishable: Option<FxHashSet<ChessMove>>,

    /// Per-square bishop distances to the mating corner; see
    /// [`super::utils::compute_bishop_distances`].
    pub(crate) bishop_distances: Option<[u8; 64]>,
}

impl Search {
    /// Creates a fresh search with default budgets and no tables.
    pub(crate) fn new() -> Self {
        Self {
            max_depth: 0,
            depth_node_limit: u64::MAX,
            global_node_limit: 1_000_000,
            iteration_nodes: 0,
            global_nodes: 0,
            interrupted: false,
            moves: Vec::with_capacity(128),
            past_progress: false,
            transposition_table: None,
            punishable: None,
            bishop_distances: None,
        }
    }

    /// Folds the current depth iteration's positions-explored count into the
    /// running totals and clears `interrupted`. Call before each depth in
    /// `iterative_deepening`.
    pub(crate) fn reset_iteration(&mut self) {
        self.global_nodes += self.iteration_nodes;
        self.iteration_nodes = 0;
        self.interrupted = false;
    }

    /// Call before each `iterative_deepening` invocation to give it a fresh
    /// budget on the number of positions it may explore.
    pub(crate) fn reset_global_nodes(&mut self) {
        self.global_nodes = 0;
    }

    /// True once either the depth-scoped or the global budget on positions
    /// explored is exhausted; see [`Search::is_depth_limit_reached`] and
    /// [`Search::is_global_limit_reached`].
    fn is_limit_reached(&self) -> bool {
        self.is_depth_limit_reached() || self.is_global_limit_reached()
    }

    /// True once `iteration_nodes` (positions explored this depth iteration)
    /// exceeds `max_depth * depth_node_limit`: the per-depth budget on
    /// positions explored, scaled by how deep this iteration searches.
    ///
    /// `depth_node_limit == u64::MAX` means "no limit"; that case is
    /// special-cased to avoid overflowing the multiplication.
    fn is_depth_limit_reached(&self) -> bool {
        self.depth_node_limit != u64::MAX
            && self.iteration_nodes > (self.max_depth as u64) * self.depth_node_limit
    }

    /// True once `global_nodes` (positions explored in prior depth
    /// iterations) plus `iteration_nodes` (this one) exceed
    /// `global_node_limit`.
    fn is_global_limit_reached(&self) -> bool {
        self.global_nodes + self.iteration_nodes > self.global_node_limit
    }
}

/// Recursive helpmate search. Returns the move sequence leading to checkmate,
/// or `None`.
///
/// `depth` counts plies played so far in this iteration (not remaining). The
/// search terminates a branch when `depth >= search.max_depth` or the budget
/// on positions explored is exhausted.
///
/// The transposition table (when present) stores `num_moves_left` per position.
/// A hit prunes the branch only if the stored value is >= the current
/// `num_moves_left`, meaning the position was already searched at least as
/// deeply.
pub(crate) fn helpmate(board: &Board, search: &mut Search, depth: u8) -> Option<Vec<ChessMove>> {
    let num_moves_left = search.max_depth.saturating_sub(depth);

    // TT probe.
    if let Some(ref tt) = search.transposition_table {
        if let Some(entry_depth) = tt.get(board) {
            if entry_depth >= &num_moves_left {
                return None;
            }
        }
    }

    let mut legal_moves = MoveGen::new_legal(board).peekable();

    if legal_moves.peek().is_none() {
        if board.side_to_move() == Color::Black && *board.checkers() != EMPTY {
            return Some(search.moves.clone()); // Checkmate.
        }
        return None; // Stalemate or White mated.
    }

    if insufficient_white_material(board) {
        return None;
    }

    if depth >= search.max_depth || search.moves.len() >= 128 {
        search.interrupted = true;
        return None;
    }

    // TT store.
    if let Some(ref mut tt) = search.transposition_table {
        tt.insert(*board, num_moves_left);
    }

    for m in legal_moves {
        search.iteration_nodes += 1;

        if search.is_limit_reached() {
            search.interrupted = true;
            return None;
        }

        let variation = classify_move(board, m, &search.punishable, &search.bishop_distances);

        let new_board = board.make_move_new(m);
        let new_depth = match variation {
            MoveVariation::Reward => depth,
            MoveVariation::Punish => depth + 4,
            MoveVariation::Normal => depth + (!search.past_progress as u8),
        };

        search.moves.push(m);
        let saved_past_progress = search.past_progress;
        search.past_progress = variation == MoveVariation::Reward;

        if let Some(helpmate) = helpmate(&new_board, search, new_depth) {
            return Some(helpmate);
        }

        search.past_progress = saved_past_progress;
        search.moves.pop();
    }

    None
}
