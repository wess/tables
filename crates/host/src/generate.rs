//! SQL generators for the "Generate SQL" actions (sidebar / grid). Dialect
//! quoting and value-literal rules live in the host so the UI never has to know
//! an engine's escaping conventions.

use model::Row;

use crate::facade::Host;

impl Host {
    /// `SELECT * FROM <table> LIMIT 100;` for the active connection's dialect.
    pub fn generate_select(&self, table: &str) -> Result<String, String> {
        let adapter = self.active_adapter()?;
        Ok(format!("SELECT * FROM {} LIMIT 100;", adapter.dialect().quote_ident(table)))
    }

    /// `DROP TABLE <table>;`.
    pub fn generate_drop(&self, table: &str) -> Result<String, String> {
        let adapter = self.active_adapter()?;
        Ok(format!("DROP TABLE {};", adapter.dialect().quote_ident(table)))
    }

    /// A parameter-marked `INSERT` template listing the table's columns, for the
    /// user to fill in.
    pub async fn generate_insert_template(&self, table: &str) -> Result<String, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();
        let cols = adapter.get_columns(table).await?;
        if cols.is_empty() {
            return Err("Table has no columns".into());
        }
        let names =
            cols.iter().map(|c| dialect.quote_ident(&c.name)).collect::<Vec<_>>().join(", ");
        let values = cols
            .iter()
            .enumerate()
            .map(|(i, _)| dialect.placeholder(i + 1))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!("INSERT INTO {} ({names})\nVALUES ({values});", dialect.quote_ident(table)))
    }

    /// One literal `INSERT` per row (the row's own keys as columns, values
    /// inlined with dialect escaping) — the grid's "Copy as INSERT".
    pub fn rows_to_insert(&self, table: &str, rows: &[Row]) -> Result<String, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();
        let quoted = dialect.quote_ident(table);
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if row.is_empty() {
                continue;
            }
            let cols =
                row.keys().map(|k| dialect.quote_ident(k)).collect::<Vec<_>>().join(", ");
            let vals = row
                .values()
                .map(|v| crate::values::literal(dialect, v))
                .collect::<Vec<_>>()
                .join(", ");
            out.push(format!("INSERT INTO {quoted} ({cols}) VALUES ({vals});"));
        }
        Ok(out.join("\n"))
    }
}
