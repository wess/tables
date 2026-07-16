//! Live cross-engine round-trip tests for parameterized execution (AUD-002).
//!
//! SQLite runs unconditionally (in a temp file). Postgres and MySQL run only
//! when `TABLES_TEST_PG` / `TABLES_TEST_MYSQL` are set — point them at a live
//! server via the `TABLES_TEST_{PG,MYSQL}_{HOST,PORT,DB,USER,PASS}` vars
//! (defaults match `docker run postgres`/`mysql` on ports 55432 / 33060).
//!
//! Each suite exercises the same path the host builds: values are bound (never
//! interpolated), casts on Postgres let a text bind land in a typed column, and
//! a failing batch rolls back. Injection strings must round-trip verbatim.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::filters::build_filter_params;
use crate::{create, Adapter, Dialect};
use model::{ConnectionConfig, FilterCondition};

struct Types {
    int: &'static str,
    text: &'static str,
    dec: &'static str,
    boolean: &'static str,
    ts: &'static str,
}

fn types(d: Dialect) -> Types {
    match d {
        Dialect::Postgres => Types { int: "integer", text: "text", dec: "numeric(12,2)", boolean: "boolean", ts: "timestamp" },
        Dialect::Mysql => Types { int: "int", text: "text", dec: "decimal(12,2)", boolean: "tinyint(1)", ts: "datetime" },
        Dialect::Sqlite => Types { int: "integer", text: "text", dec: "real", boolean: "integer", ts: "text" },
    }
}

/// The Postgres cast map for the test table (empty for the other engines).
fn col_types(d: Dialect) -> HashMap<String, String> {
    if d != Dialect::Postgres {
        return HashMap::new();
    }
    [
        ("id", "integer"),
        ("name", "text"),
        ("qty", "integer"),
        ("price", "numeric"),
        ("active", "boolean"),
        ("ts", "timestamp"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

/// Build a parameterized INSERT exactly the way the host's `write_stmt` does:
/// `NULL` keyword for nulls, a bound placeholder (cast to the column type on
/// Postgres) for scalars.
#[allow(clippy::too_many_arguments)] // a fixed six-column test fixture
fn insert_stmt(
    d: Dialect,
    types: &HashMap<String, String>,
    id: i64,
    name: Option<&str>,
    qty: Option<i64>,
    price: Option<f64>,
    active: Option<bool>,
    ts: Option<&str>,
) -> (String, Vec<Value>) {
    let cols = ["id", "name", "qty", "price", "active", "ts"];
    let vals = [
        json!(id),
        name.map(|s| json!(s)).unwrap_or(Value::Null),
        qty.map(|n| json!(n)).unwrap_or(Value::Null),
        price.map(|n| json!(n)).unwrap_or(Value::Null),
        active.map(|b| json!(b)).unwrap_or(Value::Null),
        ts.map(|s| json!(s)).unwrap_or(Value::Null),
    ];
    let mut params: Vec<Value> = Vec::new();
    let tokens: Vec<String> = cols
        .iter()
        .zip(vals.iter())
        .map(|(c, v)| match v {
            Value::Null => "NULL".to_string(),
            scalar => {
                params.push(scalar.clone());
                let ph = d.placeholder(params.len());
                match (d, types.get(*c)) {
                    (Dialect::Postgres, Some(ty)) => format!("CAST({ph} AS {ty})"),
                    _ => ph,
                }
            }
        })
        .collect();
    (format!("INSERT INTO aud (id, name, qty, price, active, ts) VALUES ({})", tokens.join(", ")), params)
}

fn fc(column: &str, operator: &str, value: &str) -> FilterCondition {
    FilterCondition {
        id: "1".into(),
        column: column.into(),
        operator: operator.into(),
        value: value.into(),
        value2: None,
    }
}

/// The `id`s a filter selects, via the parameterized WHERE builder.
async fn filter_ids(
    adapter: &Arc<dyn Adapter>,
    d: Dialect,
    types: &HashMap<String, String>,
    filters: &[FilterCondition],
) -> Vec<i64> {
    let (where_clause, params) = build_filter_params(d, filters, "and", types);
    let result = adapter
        .exec_params(&format!("SELECT id FROM aud {where_clause} ORDER BY id"), &params)
        .await
        .expect("filter query");
    result.rows.iter().filter_map(|r| r.get("id").and_then(Value::as_i64)).collect()
}

async fn run_suite(config: ConnectionConfig) {
    let adapter = create(&config).expect("create adapter");
    adapter.connect().await.expect("connect");
    let d = adapter.dialect();
    let t = types(d);
    let types = col_types(d);

    let _ = adapter.query("DROP TABLE IF EXISTS aud").await;
    adapter
        .query(&format!(
            "CREATE TABLE aud (id {} PRIMARY KEY, name {}, qty {}, price {}, active {}, ts {})",
            t.int, t.text, t.int, t.dec, t.boolean, t.ts
        ))
        .await
        .expect("create table");

    // A crafted string that would drop the table if it were interpolated.
    let injection = "O'Brien'); DROP TABLE aud;--";
    for (sql, params) in [
        insert_stmt(d, &types, 1, Some(injection), Some(30), Some(12.50), Some(true), Some("2026-01-15 10:30:00")),
        insert_stmt(d, &types, 2, Some("12345"), Some(7), Some(0.99), Some(false), Some("2020-06-01 08:00:00")),
        insert_stmt(d, &types, 3, None, None, None, None, None),
    ] {
        adapter.exec_params(&sql, &params).await.expect("insert");
    }

    // Round-trip: the table survived (injection was bound, not executed), the
    // crafted string comes back verbatim, typed values decode, nulls are null.
    let all = adapter.query("SELECT id, name, qty FROM aud ORDER BY id").await.expect("select");
    assert_eq!(all.rows.len(), 3, "{d:?}: bound injection must not drop the table");
    assert_eq!(all.rows[0]["name"], json!(injection), "{d:?}: bound string round-trips verbatim");
    assert_eq!(all.rows[0]["qty"].as_i64(), Some(30), "{d:?}: integer round-trips");
    assert!(all.rows[2]["name"].is_null(), "{d:?}: null round-trips");
    assert!(all.rows[2]["qty"].is_null(), "{d:?}: null integer round-trips");

    // Filters bind their values and (on Postgres) cast to the column type.
    assert_eq!(filter_ids(&adapter, d, &types, &[fc("qty", "=", "30")]).await, vec![1], "{d:?}: numeric column, string value");
    assert_eq!(filter_ids(&adapter, d, &types, &[fc("name", "=", "12345")]).await, vec![2], "{d:?}: text column, numeric-looking value");
    assert_eq!(filter_ids(&adapter, d, &types, &[fc("qty", ">", "10")]).await, vec![1], "{d:?}: numeric range, string value");
    assert_eq!(filter_ids(&adapter, d, &types, &[fc("name", "contains", "Brien")]).await, vec![1], "{d:?}: LIKE pattern");
    assert_eq!(
        filter_ids(&adapter, d, &types, &[fc("ts", ">=", "2025-01-01 00:00:00")]).await,
        vec![1],
        "{d:?}: temporal comparison, string value"
    );
    assert!(
        filter_ids(&adapter, d, &types, &[fc("name", "=", "x' OR '1'='1")]).await.is_empty(),
        "{d:?}: a crafted filter value matches literally nothing"
    );

    // A batch whose second statement violates the primary key rolls back whole.
    let dup = insert_stmt(d, &types, 4, Some("dup"), Some(1), Some(1.0), Some(true), Some("2021-01-01 00:00:00"));
    assert!(
        adapter.exec_batch_params(&[dup.clone(), dup]).await.is_err(),
        "{d:?}: duplicate primary key must error"
    );
    let count = adapter.query("SELECT COUNT(*) AS c FROM aud").await.expect("count");
    assert_eq!(count.rows[0]["c"].as_i64(), Some(3), "{d:?}: the failed batch rolled back");

    let _ = adapter.query("DROP TABLE IF EXISTS aud").await;
    adapter.disconnect().await;
}

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn server_config(prefix: &str, kind: &str, default_port: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: format!("aud-{kind}"),
        kind: kind.into(),
        host: env(&format!("TABLES_TEST_{prefix}_HOST"), "127.0.0.1"),
        port: env(&format!("TABLES_TEST_{prefix}_PORT"), default_port).parse().unwrap_or(0),
        database: env(&format!("TABLES_TEST_{prefix}_DB"), "tables"),
        username: env(&format!("TABLES_TEST_{prefix}_USER"), "tables"),
        password: env(&format!("TABLES_TEST_{prefix}_PASS"), "tables"),
        filepath: None,
        ssl: None,
        ssh: None,
        startup_commands: None,
    }
}

#[tokio::test]
async fn sqlite_roundtrip() {
    let path = std::env::temp_dir().join(format!("tables_aud_{}.sqlite", model::new_uuid()));
    let config = ConnectionConfig {
        id: "aud-sqlite".into(),
        kind: "sqlite".into(),
        host: String::new(),
        port: 0,
        database: String::new(),
        username: String::new(),
        password: String::new(),
        filepath: Some(path.to_string_lossy().into_owned()),
        ssl: None,
        ssh: None,
        startup_commands: None,
    };
    run_suite(config).await;
    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn postgres_roundtrip() {
    if std::env::var("TABLES_TEST_PG").is_err() {
        eprintln!("skipping postgres_roundtrip (set TABLES_TEST_PG to run)");
        return;
    }
    run_suite(server_config("PG", "postgres", "55432")).await;
}

#[tokio::test]
async fn mysql_roundtrip() {
    if std::env::var("TABLES_TEST_MYSQL").is_err() {
        eprintln!("skipping mysql_roundtrip (set TABLES_TEST_MYSQL to run)");
        return;
    }
    run_suite(server_config("MYSQL", "mysql", "33060")).await;
}
