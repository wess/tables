//! Per-engine SQL text rules: identifier quoting and string-literal escaping.
//!
//! Identifiers and string literals cannot be parameter-bound, so they are
//! quoted through one engine-specific implementation that escapes the embedded
//! delimiter. MySQL uses backtick identifiers and treats backslash as an escape
//! character in string literals; Postgres and SQLite use double-quote
//! identifiers and standard-conforming strings (backslash is literal).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dialect {
    Postgres,
    Mysql,
    Sqlite,
}

impl Dialect {
    pub fn from_kind(kind: &str) -> Dialect {
        match kind {
            "mysql" => Dialect::Mysql,
            "sqlite" => Dialect::Sqlite,
            _ => Dialect::Postgres,
        }
    }

    /// Quote an identifier, escaping the embedded delimiter and quoting each
    /// dotted segment (`schema.table`) separately so a crafted name cannot
    /// terminate the identifier or change the statement structure.
    pub fn quote_ident(self, name: &str) -> String {
        name.split('.')
            .map(|seg| self.quote_segment(seg))
            .collect::<Vec<_>>()
            .join(".")
    }

    fn quote_segment(self, seg: &str) -> String {
        match self {
            Dialect::Mysql => format!("`{}`", seg.replace('`', "``")),
            _ => format!("\"{}\"", seg.replace('"', "\"\"")),
        }
    }

    /// Quote a string literal, escaping per engine. MySQL doubles both the
    /// single quote and the backslash (its default escape character); the
    /// others double only the single quote.
    pub fn quote_string(self, v: &str) -> String {
        match self {
            Dialect::Mysql => {
                format!("'{}'", v.replace('\\', "\\\\").replace('\'', "''"))
            }
            _ => format!("'{}'", v.replace('\'', "''")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_identifiers_per_engine() {
        assert_eq!(Dialect::Postgres.quote_ident("users"), "\"users\"");
        assert_eq!(Dialect::Sqlite.quote_ident("users"), "\"users\"");
        assert_eq!(Dialect::Mysql.quote_ident("users"), "`users`");
    }

    #[test]
    fn escapes_embedded_identifier_delimiter() {
        // A crafted name cannot terminate the identifier.
        assert_eq!(Dialect::Postgres.quote_ident("we\"ird"), "\"we\"\"ird\"");
        assert_eq!(Dialect::Mysql.quote_ident("we`ird"), "`we``ird`");
        // A double quote is inert inside a MySQL backtick identifier.
        assert_eq!(Dialect::Mysql.quote_ident("a\"b"), "`a\"b`");
    }

    #[test]
    fn quotes_dotted_segments_separately() {
        assert_eq!(Dialect::Postgres.quote_ident("public.users"), "\"public\".\"users\"");
        assert_eq!(Dialect::Mysql.quote_ident("app.users"), "`app`.`users`");
    }

    #[test]
    fn quotes_reserved_and_unicode_names() {
        assert_eq!(Dialect::Postgres.quote_ident("select"), "\"select\"");
        assert_eq!(Dialect::Postgres.quote_ident("naïve"), "\"naïve\"");
    }

    #[test]
    fn escapes_string_literals() {
        assert_eq!(Dialect::Postgres.quote_string("O'Brien"), "'O''Brien'");
        // MySQL escapes backslashes so a trailing backslash cannot escape the
        // closing quote.
        assert_eq!(Dialect::Mysql.quote_string("a\\'; DROP"), "'a\\\\''; DROP'");
        assert_eq!(Dialect::Postgres.quote_string("a\\b"), "'a\\b'");
    }
}
