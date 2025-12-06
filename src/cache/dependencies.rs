use std::collections::HashSet;
use crate::parser::ast::{Statement, TableRef};

/// Extract table dependencies from a SQL statement
/// Returns a set of table names that the query depends on
pub fn extract_table_dependencies(stmt: &Statement) -> HashSet<String> {
    match stmt {
        Statement::Select(select) => {
            if let Some(ref table_ref) = select.from {
                extract_tables_from_ref(table_ref)
            } else {
                HashSet::new()
            }
        }
        Statement::CreateTable(_) => HashSet::new(),
        Statement::Insert(_) => HashSet::new(),
        Statement::Update(_) => HashSet::new(),
        Statement::Delete(_) => HashSet::new(),
        Statement::DropAllTables(_) => HashSet::new(),
    }
}

/// Recursively extract table names from a TableRef
fn extract_tables_from_ref(table_ref: &TableRef) -> HashSet<String> {
    match table_ref {
        TableRef::Table { name, .. } => {
            let mut set = HashSet::new();
            set.insert(name.clone());
            set
        }
        TableRef::Join { left, right, .. } => {
            let mut set = extract_tables_from_ref(left);
            set.extend(extract_tables_from_ref(right));
            set
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::{Select, TableRef, SelectItem};

    #[test]
    fn test_single_table_dependency() {
        let table_ref = TableRef::Table {
            name: "users".to_string(),
            alias: None,
        };
        let select = Select {
            columns: vec![SelectItem::Column("name".to_string())],
            from: Some(table_ref),
            where_clause: None,
            group_by: None,
            limit: None,
        };
        let stmt = Statement::Select(select);
        
        let deps = extract_table_dependencies(&stmt);
        assert_eq!(deps.len(), 1);
        assert!(deps.contains("users"));
    }

    #[test]
    fn test_join_dependencies() {
        let left = TableRef::Table {
            name: "users".to_string(),
            alias: None,
        };
        let right = TableRef::Table {
            name: "orders".to_string(),
            alias: None,
        };
        let table_ref = TableRef::Join {
            left: Box::new(left),
            right: Box::new(right),
            join_type: crate::parser::ast::JoinType::Inner,
            condition: None,
        };
        let select = Select {
            columns: vec![SelectItem::Column("name".to_string())],
            from: Some(table_ref),
            where_clause: None,
            group_by: None,
            limit: None,
        };
        let stmt = Statement::Select(select);
        
        let deps = extract_table_dependencies(&stmt);
        assert_eq!(deps.len(), 2);
        assert!(deps.contains("users"));
        assert!(deps.contains("orders"));
    }
}

