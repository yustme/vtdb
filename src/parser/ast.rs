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
}

/// SELECT item (column or expression)
#[derive(Debug, Clone)]
pub enum SelectItem {
    Column(String),
    All,
}

/// Table reference
#[derive(Debug, Clone)]
pub struct TableRef {
    pub name: String,
}

/// Expression
#[derive(Debug, Clone)]
pub enum Expr {
    Column(String),
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

