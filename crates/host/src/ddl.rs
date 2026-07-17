//! Structure-editing DDL: dialect-correct builders for create/alter/drop plus
//! the host methods that run them. The builders are pure so the exact SQL is
//! unit-tested per engine; the `Host` wrappers execute and drop the cached
//! column types (the schema changed).

use db::Dialect;
use model::NewColumn;

use crate::facade::Host;

/// `{name} {type}[ NOT NULL][ DEFAULT {expr}]` — one column definition, shared
/// by CREATE TABLE and ADD COLUMN. `default` is raw SQL the user typed.
fn column_def(d: Dialect, name: &str, ty: &str, nullable: bool, default: Option<&str>) -> String {
    let mut sql = format!("{} {}", d.quote_ident(name), ty.trim());
    if !nullable {
        sql.push_str(" NOT NULL");
    }
    if let Some(expr) = default.map(str::trim).filter(|s| !s.is_empty()) {
        sql.push_str(&format!(" DEFAULT {expr}"));
    }
    sql
}

fn ddl_create_table(d: Dialect, table: &str, cols: &[NewColumn]) -> String {
    let mut defs: Vec<String> = cols
        .iter()
        .map(|c| column_def(d, &c.name, &c.data_type, c.nullable, c.default_value.as_deref()))
        .collect();
    let pk: Vec<String> =
        cols.iter().filter(|c| c.primary_key).map(|c| d.quote_ident(&c.name)).collect();
    if !pk.is_empty() {
        defs.push(format!("PRIMARY KEY ({})", pk.join(", ")));
    }
    format!("CREATE TABLE {} (\n  {}\n)", d.quote_ident(table), defs.join(",\n  "))
}

fn ddl_add_column(
    d: Dialect,
    table: &str,
    name: &str,
    ty: &str,
    nullable: bool,
    default: Option<&str>,
) -> String {
    format!(
        "ALTER TABLE {} ADD COLUMN {}",
        d.quote_ident(table),
        column_def(d, name, ty, nullable, default)
    )
}

fn ddl_drop_column(d: Dialect, table: &str, column: &str) -> String {
    format!("ALTER TABLE {} DROP COLUMN {}", d.quote_ident(table), d.quote_ident(column))
}

fn ddl_rename_column(d: Dialect, table: &str, old: &str, new: &str) -> String {
    format!(
        "ALTER TABLE {} RENAME COLUMN {} TO {}",
        d.quote_ident(table),
        d.quote_ident(old),
        d.quote_ident(new)
    )
}

fn ddl_create_index(d: Dialect, table: &str, name: &str, columns: &[String], unique: bool) -> String {
    let cols = columns.iter().map(|c| d.quote_ident(c)).collect::<Vec<_>>().join(", ");
    let unique = if unique { "UNIQUE " } else { "" };
    format!(
        "CREATE {unique}INDEX {} ON {} ({cols})",
        d.quote_ident(name),
        d.quote_ident(table)
    )
}

/// MySQL scopes `DROP INDEX` to a table; Postgres and SQLite drop it by name.
fn ddl_drop_index(d: Dialect, table: &str, name: &str) -> String {
    match d {
        Dialect::Mysql => {
            format!("DROP INDEX {} ON {}", d.quote_ident(name), d.quote_ident(table))
        }
        _ => format!("DROP INDEX {}", d.quote_ident(name)),
    }
}

fn ddl_drop_table(d: Dialect, table: &str) -> String {
    format!("DROP TABLE {}", d.quote_ident(table))
}

impl Host {
    /// Run a DDL statement on the active connection, then drop cached column
    /// types (the schema just changed).
    async fn run_ddl(&self, sql: &str) -> Result<(), String> {
        let adapter = self.active_adapter()?;
        adapter.query(sql).await?;
        self.invalidate_schema_cache();
        Ok(())
    }

    pub async fn create_table(&self, table: &str, cols: &[NewColumn]) -> Result<(), String> {
        if table.trim().is_empty() {
            return Err("Table name is required".into());
        }
        if cols.is_empty() {
            return Err("At least one column is required".into());
        }
        let d = self.active_adapter()?.dialect();
        self.run_ddl(&ddl_create_table(d, table, cols)).await
    }

    pub async fn add_column(
        &self,
        table: &str,
        name: &str,
        data_type: &str,
        nullable: bool,
        default: Option<&str>,
    ) -> Result<(), String> {
        let d = self.active_adapter()?.dialect();
        self.run_ddl(&ddl_add_column(d, table, name, data_type, nullable, default)).await
    }

    pub async fn drop_column(&self, table: &str, column: &str) -> Result<(), String> {
        let d = self.active_adapter()?.dialect();
        self.run_ddl(&ddl_drop_column(d, table, column)).await
    }

    pub async fn rename_column(&self, table: &str, old: &str, new: &str) -> Result<(), String> {
        let d = self.active_adapter()?.dialect();
        self.run_ddl(&ddl_rename_column(d, table, old, new)).await
    }

    pub async fn create_index(
        &self,
        table: &str,
        name: &str,
        columns: &[String],
        unique: bool,
    ) -> Result<(), String> {
        let d = self.active_adapter()?.dialect();
        self.run_ddl(&ddl_create_index(d, table, name, columns, unique)).await
    }

    pub async fn drop_index(&self, table: &str, name: &str) -> Result<(), String> {
        let d = self.active_adapter()?.dialect();
        self.run_ddl(&ddl_drop_index(d, table, name)).await
    }

    pub async fn drop_table(&self, table: &str) -> Result<(), String> {
        let d = self.active_adapter()?.dialect();
        self.run_ddl(&ddl_drop_table(d, table)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, ty: &str, nullable: bool, pk: bool) -> NewColumn {
        NewColumn {
            name: name.into(),
            data_type: ty.into(),
            nullable,
            primary_key: pk,
            default_value: None,
        }
    }

    #[test]
    fn create_table_with_primary_key() {
        let cols = vec![col("id", "INTEGER", false, true), col("name", "TEXT", true, false)];
        let sql = ddl_create_table(Dialect::Sqlite, "users", &cols);
        assert_eq!(
            sql,
            "CREATE TABLE \"users\" (\n  \"id\" INTEGER NOT NULL,\n  \"name\" TEXT,\n  PRIMARY KEY (\"id\")\n)"
        );
    }

    #[test]
    fn add_column_carries_nullability_and_default() {
        let sql = ddl_add_column(Dialect::Postgres, "t", "age", "integer", false, Some("0"));
        assert_eq!(sql, "ALTER TABLE \"t\" ADD COLUMN \"age\" integer NOT NULL DEFAULT 0");
    }

    #[test]
    fn drop_and_rename_column() {
        assert_eq!(
            ddl_drop_column(Dialect::Postgres, "t", "age"),
            "ALTER TABLE \"t\" DROP COLUMN \"age\""
        );
        assert_eq!(
            ddl_rename_column(Dialect::Sqlite, "t", "a", "b"),
            "ALTER TABLE \"t\" RENAME COLUMN \"a\" TO \"b\""
        );
    }

    #[test]
    fn create_unique_index() {
        let sql = ddl_create_index(Dialect::Postgres, "t", "t_email", &["email".into()], true);
        assert_eq!(sql, "CREATE UNIQUE INDEX \"t_email\" ON \"t\" (\"email\")");
    }

    #[test]
    fn drop_index_is_table_scoped_only_on_mysql() {
        assert_eq!(ddl_drop_index(Dialect::Postgres, "t", "idx"), "DROP INDEX \"idx\"");
        assert_eq!(ddl_drop_index(Dialect::Sqlite, "t", "idx"), "DROP INDEX \"idx\"");
        assert_eq!(ddl_drop_index(Dialect::Mysql, "t", "idx"), "DROP INDEX `idx` ON `t`");
    }
}
