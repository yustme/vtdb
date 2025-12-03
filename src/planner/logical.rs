use crate::parser::ast::*;

/// Logical query plan representation
#[derive(Debug, Clone)]
pub enum LogicalPlan {
    CreateTable {
        name: String,
        columns: Vec<(String, crate::catalog::types::DataType)>,
    },
    Select {
        table: String,
        columns: Vec<SelectItem>,
        filter: Option<Expr>,
    },
    Insert {
        table: String,
        columns: Vec<String>,
        values: Vec<Vec<Expr>>,
    },
    Update {
        table: String,
        assignments: Vec<(String, Expr)>,
        filter: Option<Expr>,
    },
    Delete {
        table: String,
        filter: Option<Expr>,
    },
}

