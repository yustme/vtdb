use crate::catalog::types::DataType;
use crate::Value;

/// Abstract Syntax Tree node for SQL statements
#[derive(Debug, Clone)]
pub enum Statement {
    CreateTable(CreateTable),
    Select(Select),
    Insert(Insert),
    Update(Update),
    Delete(Delete),
}

/// CREATE TABLE statement
#[derive(Debug, Clone)]
pub struct CreateTable {
    pub name: String,
    pub columns: Vec<ColumnDef>,
}

/// Column definition
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: DataType,
}

/// SELECT statement
#[derive(Debug, Clone)]
pub struct Select {
    pub columns: Vec<SelectItem>,
    pub from: Option<TableRef>,
    pub where_clause: Option<Expr>,
    pub group_by: Option<Vec<String>>,
    pub limit: Option<u64>,
}

/// SELECT item (column or expression)
#[derive(Debug, Clone)]
pub enum SelectItem {
    Column(String),
    All,
    FunctionCall {
        name: String,
        function: AggregateFunction,
    },
}

/// Aggregate function
#[derive(Debug, Clone)]
pub enum AggregateFunction {
    Count { distinct: bool, expr: Option<Box<Expr>> }, // None = COUNT(*)
    Sum { distinct: bool, expr: Box<Expr> },
    Avg { distinct: bool, expr: Box<Expr> },
    Min { expr: Box<Expr> },
    Max { expr: Box<Expr> },
}

/// Table reference (can be a table or a JOIN)
#[derive(Debug, Clone)]
pub enum TableRef {
    Table {
        name: String,
        alias: Option<String>,
    },
    Join {
        left: Box<TableRef>,
        right: Box<TableRef>,
        join_type: JoinType,
        condition: Option<JoinCondition>,
    },
}

/// JOIN type
#[derive(Debug, Clone, PartialEq)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    FullOuter,
    Cross,
}

/// JOIN condition
#[derive(Debug, Clone)]
pub enum JoinCondition {
    On(Expr),  // ON condition
    Using(Vec<String>),  // USING (col1, col2, ...)
}

/// Expression
#[derive(Debug, Clone)]
pub enum Expr {
    Column(String),  // Simple column name
    QualifiedColumn { table: String, column: String },  // table.column
    Literal(Value),
    BinaryOp {
        left: Box<Expr>,
        op: BinaryOperator,
        right: Box<Expr>,
    },
}

/// Binary operator
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    Eq,      // =
    Ne,      // !=
    Lt,      // <
    Gt,      // >
    Le,      // <=
    Ge,      // >=
    And,     // AND
    Or,      // OR
}

/// INSERT statement
#[derive(Debug, Clone)]
pub struct Insert {
    pub table: String,
    pub columns: Vec<String>,
    pub values: Vec<Vec<Expr>>,
}

/// UPDATE statement
#[derive(Debug, Clone)]
pub struct Update {
    pub table: String,
    pub set: Vec<Assignment>,
    pub where_clause: Option<Expr>,
}

/// Assignment (column = value)
#[derive(Debug, Clone)]
pub struct Assignment {
    pub column: String,
    pub value: Expr,
}

/// DELETE statement
#[derive(Debug, Clone)]
pub struct Delete {
    pub table: String,
    pub where_clause: Option<Expr>,
}

