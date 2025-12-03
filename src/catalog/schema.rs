use super::types::DataType;
use std::collections::HashMap;

/// Database schema
pub struct Schema {
    pub name: String,
    tables: HashMap<String, Table>,
}

impl Schema {
    pub fn new(name: String) -> Self {
        Self {
            name,
            tables: HashMap::new(),
        }
    }

    pub fn add_table(&mut self, name: String, table: Table) {
        self.tables.insert(name, table);
    }

    pub fn get_table(&self, name: &str) -> Option<&Table> {
        self.tables.get(name)
    }

    pub fn has_table(&self, name: &str) -> bool {
        self.tables.contains_key(name)
    }

    /// List all table names in this schema
    pub fn list_tables(&self) -> Vec<String> {
        self.tables.keys().cloned().collect()
    }
}

/// Table metadata
#[derive(Debug)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
}

impl Table {
    pub fn new(name: String, column_defs: Vec<(String, DataType)>) -> Self {
        let columns = column_defs
            .into_iter()
            .enumerate()
            .map(|(idx, (name, data_type))| Column {
                name,
                data_type,
                ordinal: idx,
            })
            .collect();

        Self { name, columns }
    }

    pub fn get_column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }

    pub fn column_count(&self) -> usize {
        self.columns.len()
    }
}

/// Column metadata
#[derive(Debug)]
pub struct Column {
    pub name: String,
    pub data_type: DataType,
    pub ordinal: usize,
}

