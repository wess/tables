//! Schema diffing. The generated SQL migrates target toward source.

use model::{ColumnInfo, SchemaDiff};

/// JS `new Map(entries)` semantics: duplicate keys keep the first position but
/// take the last value.
fn dedupe<'a, T: Copy>(items: impl Iterator<Item = (&'a str, T)>) -> Vec<(&'a str, T)> {
    let mut out: Vec<(&'a str, T)> = Vec::new();
    for (key, value) in items {
        match out.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => out.push((key, value)),
        }
    }
    out
}

fn has<T: Copy>(map: &[(&str, T)], key: &str) -> bool {
    map.iter().any(|(k, _)| *k == key)
}

fn get<T: Copy>(map: &[(&str, T)], key: &str) -> Option<T> {
    map.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

/// JS truthiness for a defaultValue: present and non-empty.
fn truthy(v: &Option<String>) -> bool {
    v.as_deref().is_some_and(|s| !s.is_empty())
}

pub fn compare_schemas(
    source: &[(String, Vec<ColumnInfo>)],
    target: &[(String, Vec<ColumnInfo>)],
) -> Vec<SchemaDiff> {
    let mut diffs = Vec::new();
    let source_map = dedupe(source.iter().map(|(n, c)| (n.as_str(), c)));
    let target_map = dedupe(target.iter().map(|(n, c)| (n.as_str(), c)));

    // Tables in source but not target — CREATE
    for (name, table) in &source_map {
        if !has(&target_map, name) {
            let cols = table
                .iter()
                .map(|c| {
                    let mut def = format!("  \"{}\" {}", c.name, c.data_type);
                    if !c.nullable {
                        def.push_str(" NOT NULL");
                    }
                    if truthy(&c.default_value) {
                        def.push_str(&format!(" DEFAULT {}", c.default_value.as_deref().unwrap()));
                    }
                    def
                })
                .collect::<Vec<_>>()
                .join(",\n");
            let pks = table
                .iter()
                .filter(|c| c.is_primary_key)
                .map(|c| format!("\"{}\"", c.name))
                .collect::<Vec<_>>()
                .join(", ");
            let pk_clause = if pks.is_empty() {
                String::new()
            } else {
                format!(",\n  PRIMARY KEY ({pks})")
            };
            diffs.push(SchemaDiff {
                table: name.to_string(),
                kind: "added".into(),
                details: format!("Table missing in target ({} columns)", table.len()),
                sql: format!("CREATE TABLE \"{name}\" (\n{cols}{pk_clause}\n);"),
            });
        }
    }

    // Tables in target but not source — DROP
    for (name, _) in &target_map {
        if !has(&source_map, name) {
            diffs.push(SchemaDiff {
                table: name.to_string(),
                kind: "removed".into(),
                details: "Table exists in target but not in source".into(),
                sql: format!("DROP TABLE IF EXISTS \"{name}\";"),
            });
        }
    }

    // Tables in both — compare columns
    for (name, source_table) in &source_map {
        let Some(target_table) = get(&target_map, name) else {
            continue;
        };
        let source_cols = dedupe(source_table.iter().map(|c| (c.name.as_str(), c)));
        let target_cols = dedupe(target_table.iter().map(|c| (c.name.as_str(), c)));

        // Columns in source but not target — ADD COLUMN
        for (col_name, col) in &source_cols {
            if !has(&target_cols, col_name) {
                let mut def = format!("\"{col_name}\" {}", col.data_type);
                if !col.nullable {
                    def.push_str(" NOT NULL");
                }
                if truthy(&col.default_value) {
                    def.push_str(&format!(" DEFAULT {}", col.default_value.as_deref().unwrap()));
                }
                diffs.push(SchemaDiff {
                    table: name.to_string(),
                    kind: "modified".into(),
                    details: format!("Add column \"{col_name}\" ({})", col.data_type),
                    sql: format!("ALTER TABLE \"{name}\" ADD COLUMN {def};"),
                });
            }
        }

        // Columns in target but not source — DROP COLUMN
        for (col_name, _) in &target_cols {
            if !has(&source_cols, col_name) {
                diffs.push(SchemaDiff {
                    table: name.to_string(),
                    kind: "modified".into(),
                    details: format!("Drop column \"{col_name}\""),
                    sql: format!("ALTER TABLE \"{name}\" DROP COLUMN \"{col_name}\";"),
                });
            }
        }

        // Columns in both — type / nullability / default changes
        for (col_name, source_col) in &source_cols {
            let Some(target_col) = get(&target_cols, col_name) else {
                continue;
            };

            let mut changes: Vec<String> = Vec::new();
            if source_col.data_type != target_col.data_type {
                changes.push(format!(
                    "type {} → {}",
                    target_col.data_type, source_col.data_type
                ));
            }
            if source_col.nullable != target_col.nullable {
                changes.push(
                    if source_col.nullable {
                        "make nullable"
                    } else {
                        "make not null"
                    }
                    .into(),
                );
            }
            if source_col.default_value != target_col.default_value {
                changes.push(format!(
                    "default {} → {}",
                    target_col.default_value.as_deref().unwrap_or("none"),
                    source_col.default_value.as_deref().unwrap_or("none"),
                ));
            }

            if !changes.is_empty() {
                // The TYPE line is always emitted, even when only
                // nullability/default changed.
                let mut sql = format!(
                    "ALTER TABLE \"{name}\" ALTER COLUMN \"{col_name}\" TYPE {};",
                    source_col.data_type
                );
                if source_col.nullable != target_col.nullable {
                    sql.push_str(&format!(
                        "\nALTER TABLE \"{name}\" ALTER COLUMN \"{col_name}\" {};",
                        if source_col.nullable {
                            "DROP NOT NULL"
                        } else {
                            "SET NOT NULL"
                        }
                    ));
                }
                if source_col.default_value != target_col.default_value {
                    sql.push_str(&if truthy(&source_col.default_value) {
                        format!(
                            "\nALTER TABLE \"{name}\" ALTER COLUMN \"{col_name}\" SET DEFAULT {};",
                            source_col.default_value.as_deref().unwrap()
                        )
                    } else {
                        format!("\nALTER TABLE \"{name}\" ALTER COLUMN \"{col_name}\" DROP DEFAULT;")
                    });
                }
                diffs.push(SchemaDiff {
                    table: name.to_string(),
                    kind: "modified".into(),
                    details: format!("Column \"{col_name}\": {}", changes.join(", ")),
                    sql,
                });
            }
        }
    }

    diffs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            default_value: None,
            is_primary_key: false,
            comment: None,
        }
    }

    fn table(name: &str, columns: Vec<ColumnInfo>) -> (String, Vec<ColumnInfo>) {
        (name.to_string(), columns)
    }

    #[test]
    fn emits_create_table_for_tables_only_in_source() {
        let mut id = col("id");
        id.data_type = "integer".into();
        id.nullable = false;
        id.is_primary_key = true;
        let diffs = compare_schemas(&[table("users", vec![id])], &[]);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].kind, "added");
        assert!(diffs[0].sql.contains("CREATE TABLE \"users\""));
        assert!(diffs[0].sql.contains("PRIMARY KEY (\"id\")"));
    }

    #[test]
    fn emits_drop_table_for_tables_only_in_target() {
        let diffs = compare_schemas(&[], &[table("old", vec![col("id")])]);
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].kind, "removed");
        assert_eq!(diffs[0].sql, "DROP TABLE IF EXISTS \"old\";");
    }

    #[test]
    fn emits_add_column_for_a_column_only_in_source() {
        let mut b = col("b");
        b.data_type = "integer".into();
        let diffs = compare_schemas(
            &[table("t", vec![col("a"), b])],
            &[table("t", vec![col("a")])],
        );
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].sql, "ALTER TABLE \"t\" ADD COLUMN \"b\" integer;");
    }

    #[test]
    fn emits_drop_column_for_a_column_only_in_target() {
        let diffs = compare_schemas(
            &[table("t", vec![col("a")])],
            &[table("t", vec![col("a"), col("gone")])],
        );
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].sql, "ALTER TABLE \"t\" DROP COLUMN \"gone\";");
    }

    #[test]
    fn detects_type_and_nullability_changes_on_shared_columns() {
        let mut source_a = col("a");
        source_a.data_type = "bigint".into();
        source_a.nullable = false;
        let mut target_a = col("a");
        target_a.data_type = "integer".into();
        target_a.nullable = true;
        let diffs = compare_schemas(&[table("t", vec![source_a])], &[table("t", vec![target_a])]);
        assert_eq!(diffs.len(), 1);
        assert!(diffs[0].sql.contains("TYPE bigint"));
        assert!(diffs[0].sql.contains("SET NOT NULL"));
    }

    #[test]
    fn reports_no_diffs_for_identical_schemas() {
        let schema = vec![table("t", vec![col("a"), col("b")])];
        assert!(compare_schemas(&schema, &schema).is_empty());
    }

    #[test]
    fn details_use_unicode_arrow_and_comma_join() {
        let mut source_a = col("a");
        source_a.data_type = "bigint".into();
        source_a.default_value = Some("1".into());
        let mut target_a = col("a");
        target_a.data_type = "integer".into();
        let diffs = compare_schemas(&[table("t", vec![source_a])], &[table("t", vec![target_a])]);
        assert_eq!(
            diffs[0].details,
            "Column \"a\": type integer → bigint, default none → 1"
        );
        assert_eq!(
            diffs[0].sql,
            "ALTER TABLE \"t\" ALTER COLUMN \"a\" TYPE bigint;\nALTER TABLE \"t\" ALTER COLUMN \"a\" SET DEFAULT 1;"
        );
    }
}
