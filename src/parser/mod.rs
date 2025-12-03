pub mod ast;
pub mod snowflake;

use anyhow::{anyhow, Result};
use ast::Statement;
use sqlparser::dialect::SnowflakeDialect;
use sqlparser::parser::Parser;

/// Parse SQL statement into AST
pub fn parse_sql(sql: &str) -> Result<Statement> {
    let dialect = SnowflakeDialect {};
    let mut parser = Parser::new(&dialect).try_with_sql(sql)?;

    let ast = parser.parse_statement()?;
    snowflake::convert_statement(ast)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_create_table() {
        let sql = "CREATE TABLE users (id INTEGER, name VARCHAR)";
        let result = parse_sql(sql);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_select() {
        let sql = r#"SELECT "name" FROM "users" WHERE "id" = 1"#;
        let result = parse_sql(sql);
        assert!(result.is_ok());
    }
}

