// Operator implementations
// This module contains the execution operators

pub mod scan;
pub mod filter;
pub mod project;
pub mod modify;

pub use scan::TableScan;
pub use filter::Filter;
pub use project::Project;
pub use modify::{Insert, Update, Delete};

