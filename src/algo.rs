//! Parallel graph algorithms
//!
//! For convenience, [`petgraph::algo`] is re-exported.

pub use petgraph::algo::*;

pub mod delta_stepping;
pub use delta_stepping::delta_stepping;
