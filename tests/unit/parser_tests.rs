use vtdb::parser::parse_sql;

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

#[test]
fn test_parse_insert() {
    let sql = "INSERT INTO users VALUES (1, 'Alice')";
    let result = parse_sql(sql);
    assert!(result.is_ok());
}

#[test]
fn test_parse_update() {
    let sql = "UPDATE users SET name = 'Bob' WHERE id = 1";
    let result = parse_sql(sql);
    assert!(result.is_ok());
}

#[test]
fn test_parse_delete() {
    let sql = "DELETE FROM users WHERE id = 1";
    let result = parse_sql(sql);
    assert!(result.is_ok());
}

