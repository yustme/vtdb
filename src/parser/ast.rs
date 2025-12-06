use crate::catalog::types::DataType;
use crate::Value;
use std::hash::{Hash, Hasher};

/// Abstract Syntax Tree node for SQL statements
#[derive(Debug, Clone)]
pub enum Statement {
    CreateTable(CreateTable),
    Select(Select),
    Insert(Insert),
    Update(Update),
    Delete(Delete),
    DropAllTables(DropAllTables),
}

impl Hash for Statement {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Statement::CreateTable(create) => {
                0u8.hash(state);
                create.hash(state);
            }
            Statement::Select(select) => {
                1u8.hash(state);
                select.hash(state);
            }
            Statement::Insert(insert) => {
                2u8.hash(state);
                insert.hash(state);
            }
            Statement::Update(update) => {
                3u8.hash(state);
                update.hash(state);
            }
            Statement::Delete(delete) => {
                4u8.hash(state);
                delete.hash(state);
            }
            Statement::DropAllTables(_) => {
                5u8.hash(state);
            }
        }
    }
}

/// CREATE TABLE statement
#[derive(Debug, Clone)]
pub struct CreateTable {
    pub name: String,
    pub columns: Vec<ColumnDef>,
}

impl Hash for CreateTable {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.columns.hash(state);
    }
}

/// Column definition
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: DataType,
}

impl Hash for ColumnDef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        // DataType doesn't implement Hash, so we hash its discriminant
        match &self.data_type {
            DataType::Integer => 0u8.hash(state),
            DataType::Varchar => 1u8.hash(state),
            DataType::Boolean => 2u8.hash(state),
        }
    }
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

impl Hash for Select {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.columns.hash(state);
        self.from.hash(state);
        self.where_clause.hash(state);
        self.group_by.hash(state);
        self.limit.hash(state);
    }
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

impl Hash for SelectItem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            SelectItem::Column(col) => {
                0u8.hash(state);
                col.hash(state);
            }
            SelectItem::All => {
                1u8.hash(state);
            }
            SelectItem::FunctionCall { name, function } => {
                2u8.hash(state);
                name.hash(state);
                function.hash(state);
            }
        }
    }
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

impl Hash for AggregateFunction {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            AggregateFunction::Count { distinct, expr } => {
                0u8.hash(state);
                distinct.hash(state);
                expr.hash(state);
            }
            AggregateFunction::Sum { distinct, expr } => {
                1u8.hash(state);
                distinct.hash(state);
                expr.hash(state);
            }
            AggregateFunction::Avg { distinct, expr } => {
                2u8.hash(state);
                distinct.hash(state);
                expr.hash(state);
            }
            AggregateFunction::Min { expr } => {
                3u8.hash(state);
                expr.hash(state);
            }
            AggregateFunction::Max { expr } => {
                4u8.hash(state);
                expr.hash(state);
            }
        }
    }
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

impl Hash for TableRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            TableRef::Table { name, alias } => {
                0u8.hash(state);
                name.hash(state);
                alias.hash(state);
            }
            TableRef::Join { left, right, join_type, condition } => {
                1u8.hash(state);
                left.hash(state);
                right.hash(state);
                join_type.hash(state);
                condition.hash(state);
            }
        }
    }
}

/// JOIN type
#[derive(Debug, Clone, PartialEq, Hash)]
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

impl Hash for JoinCondition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            JoinCondition::On(expr) => {
                0u8.hash(state);
                expr.hash(state);
            }
            JoinCondition::Using(cols) => {
                1u8.hash(state);
                cols.hash(state);
            }
        }
    }
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

impl Hash for Expr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Expr::Column(col) => {
                0u8.hash(state);
                col.hash(state);
            }
            Expr::QualifiedColumn { table, column } => {
                1u8.hash(state);
                table.hash(state);
                column.hash(state);
            }
            Expr::Literal(val) => {
                2u8.hash(state);
                val.hash(state);
            }
            Expr::BinaryOp { left, op, right } => {
                3u8.hash(state);
                left.hash(state);
                op.hash(state);
                right.hash(state);
            }
        }
    }
}

/// Binary operator
#[derive(Debug, Clone, PartialEq, Hash)]
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

impl Hash for Insert {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.table.hash(state);
        self.columns.hash(state);
        self.values.hash(state);
    }
}

/// UPDATE statement
#[derive(Debug, Clone)]
pub struct Update {
    pub table: String,
    pub set: Vec<Assignment>,
    pub where_clause: Option<Expr>,
}

impl Hash for Update {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.table.hash(state);
        self.set.hash(state);
        self.where_clause.hash(state);
    }
}

/// Assignment (column = value)
#[derive(Debug, Clone)]
pub struct Assignment {
    pub column: String,
    pub value: Expr,
}

impl Hash for Assignment {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.column.hash(state);
        self.value.hash(state);
    }
}

/// DELETE statement
#[derive(Debug, Clone)]
pub struct Delete {
    pub table: String,
    pub where_clause: Option<Expr>,
}

impl Hash for Delete {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.table.hash(state);
        self.where_clause.hash(state);
    }
}

/// DROP ALL TABLES statement
#[derive(Debug, Clone)]
pub struct DropAllTables;

impl Hash for DropAllTables {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Empty struct, just hash a constant
        0u8.hash(state);
    }
}

/// Compute hash of a statement for use as cache key
pub fn hash_statement(stmt: &Statement) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    stmt.hash(&mut hasher);
    hasher.finish()
}

