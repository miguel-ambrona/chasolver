//! Winnability analysis: [`fast::is_unwinnable`], a fast incomplete check,
//! and [`full::analysis`], a full multi-phase search. Both are designed for
//! White as the intended winner; the public `winnability`/`is_unwinnable_fast`
//! functions in `crate` mirror the board for a Black-intended-winner query
//! before calling into this module, then mirror the result back.
//!
//! Only [`full::analysis`] can confirm winnability; [`fast::is_unwinnable`]
//! only ever proves unwinnability, faster but incompletely.

pub(crate) mod fast;
pub(crate) mod full;
