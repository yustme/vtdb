// Operator implementations
// This module contains the execution operators

pub mod scan;
pub mod filter;
pub mod index_scan;
pub mod project;
pub mod modify;

pub use scan::TableScan;
pub use filter::Filter;
pub use index_scan::IndexScan;
pub use project::Project;
pub use modify::{Insert, Update, Delete};

