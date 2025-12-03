use anyhow::Result;
use crate::Value;
use serde::{Deserialize, Serialize};

/// Write-Ahead Log for durability
pub struct WAL {
    entries: Vec<WALEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum WALEntry {
    Insert {
        table: String,
        rows: Vec<Vec<Value>>,
    },
    Update {
        table: String,
        column_idx: usize,
        value: Value,
    },
    Delete {
        table: String,
    },
}

impl WAL {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn append_insert(&mut self, table: &str, rows: Vec<Vec<Value>>) -> Result<()> {
        self.entries.push(WALEntry::Insert {
            table: table.to_string(),
            rows,
        });
        Ok(())
    }

    pub fn append_update(&mut self, table: &str, column_idx: usize, value: &Value) -> Result<()> {
        self.entries.push(WALEntry::Update {
            table: table.to_string(),
            column_idx,
            value: value.clone(),
        });
        Ok(())
    }

    pub fn append_delete(&mut self, table: &str) -> Result<()> {
        self.entries.push(WALEntry::Delete {
            table: table.to_string(),
        });
        Ok(())
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

impl Default for WAL {
    fn default() -> Self {
        Self::new()
    }
}

// Deserialization for Value
impl<'de> Deserialize<'de> for Value {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Visitor;
        use std::fmt;

        struct ValueVisitor;

        impl<'de> Visitor<'de> for ValueVisitor {
            type Value = Value;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a Value")
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
                Ok(Value::Integer(v))
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(Value::Varchar(v.to_string()))
            }

            fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
                Ok(Value::Boolean(v))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(Value::Null)
            }
        }

        deserializer.deserialize_any(ValueVisitor)
    }
}

