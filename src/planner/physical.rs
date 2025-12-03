use crate::parser::ast::*;

/// Physical execution plan
#[derive(Debug, Clone)]
pub enum PhysicalPlan {
    CreateTable {
        name: String,
        columns: Vec<(String, crate::catalog::types::DataType)>,
    },
    Select {
        table: String,
        columns: Vec<SelectItem>,
        filter: Option<Expr>,
        group_by: Option<Vec<String>>,
        limit: Option<u64>,
    },
    Insert {
        table: String,
        columns: Vec<String>,
        values: Vec<Vec<Expr>>,
    },
    Update {
        table: String,
        assignments: Vec<(usize, Expr)>, // (column_index, value_expr)
        filter: Option<Expr>,
    },
    Delete {
        table: String,
        filter: Option<Expr>,
    },
}

