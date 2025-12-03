/// Data types supported by the database
#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Integer,
    Varchar,
    Boolean,
}

impl DataType {
    pub fn name(&self) -> &'static str {
        match self {
            DataType::Integer => "INTEGER",
            DataType::Varchar => "VARCHAR",
            DataType::Boolean => "BOOLEAN",
        }
    }
}

