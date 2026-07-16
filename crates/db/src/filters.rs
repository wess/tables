//! Pure SQL text builders: the row-browsing filter clause and the
//! multi-statement splitter. Identifier and string quoting are engine-aware
//! (see `Dialect`).

use crate::dialect::Dialect;
use model::FilterCondition;

fn clause(dialect: Dialect, filter: &FilterCondition) -> String {
    let col = dialect.quote_ident(&filter.column);
    let quoted = |v: &str| dialect.quote_string(v);
    match filter.operator.as_str() {
        "=" => format!("{col} = {}", quoted(&filter.value)),
        "!=" => format!("{col} != {}", quoted(&filter.value)),
        "contains" => format!("{col} LIKE {}", quoted(&format!("%{}%", filter.value))),
        "not_contains" => format!("{col} NOT LIKE {}", quoted(&format!("%{}%", filter.value))),
        "starts_with" => format!("{col} LIKE {}", quoted(&format!("{}%", filter.value))),
        "ends_with" => format!("{col} LIKE {}", quoted(&format!("%{}", filter.value))),
        ">" => format!("{col} > {}", quoted(&filter.value)),
        "<" => format!("{col} < {}", quoted(&filter.value)),
        ">=" => format!("{col} >= {}", quoted(&filter.value)),
        "<=" => format!("{col} <= {}", quoted(&filter.value)),
        "is_null" => format!("{col} IS NULL"),
        "is_not_null" => format!("{col} IS NOT NULL"),
        "in" => {
            let items = filter
                .value
                .split(',')
                .map(|s| quoted(s.trim()))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{col} IN ({items})")
        }
        "between" => {
            let v2 = filter.value2.as_deref().unwrap_or("");
            format!("{col} BETWEEN {} AND {}", quoted(&filter.value), quoted(v2))
        }
        _ => String::new(),
    }
}

/// The full `WHERE …` prefix for row browsing, or an empty string.
pub fn build_filter_clause(dialect: Dialect, filters: &[FilterCondition], logic: &str) -> String {
    let clauses: Vec<String> = filters
        .iter()
        .map(|f| clause(dialect, f))
        .filter(|c| !c.is_empty())
        .collect();
    if clauses.is_empty() {
        return String::new();
    }
    let joiner = if logic == "or" { " OR " } else { " AND " };
    format!("WHERE {}", clauses.join(joiner))
}

/// Split multi-statement SQL on top-level semicolons. A small tokenizer skips
/// semicolons inside single-quoted strings, double-quoted / backtick
/// identifiers, `--` line comments, and `/* */` block comments. (Postgres
/// dollar-quoted bodies are not tracked; run those as a single script.)
pub fn split_statements(sql: &str) -> Vec<String> {
    #[derive(PartialEq)]
    enum State {
        Normal,
        Single,
        Double,
        Backtick,
        LineComment,
        BlockComment,
    }

    let mut statements = Vec::new();
    let mut current = String::new();
    let mut chars = sql.chars().peekable();
    let mut state = State::Normal;

    let flush = |current: &mut String, statements: &mut Vec<String>| {
        let trimmed = current.trim().to_string();
        if !trimmed.is_empty() {
            statements.push(trimmed);
        }
        current.clear();
    };

    while let Some(c) = chars.next() {
        match state {
            State::Normal => match c {
                '\'' => {
                    state = State::Single;
                    current.push(c);
                }
                '"' => {
                    state = State::Double;
                    current.push(c);
                }
                '`' => {
                    state = State::Backtick;
                    current.push(c);
                }
                '-' if chars.peek() == Some(&'-') => {
                    state = State::LineComment;
                    current.push(c);
                }
                '/' if chars.peek() == Some(&'*') => {
                    state = State::BlockComment;
                    current.push(c);
                }
                ';' => flush(&mut current, &mut statements),
                _ => current.push(c),
            },
            State::Single => {
                current.push(c);
                if c == '\'' {
                    // A doubled quote is an escape, not a terminator.
                    if chars.peek() == Some(&'\'') {
                        current.push(chars.next().unwrap());
                    } else {
                        state = State::Normal;
                    }
                }
            }
            State::Double => {
                current.push(c);
                if c == '"' {
                    state = State::Normal;
                }
            }
            State::Backtick => {
                current.push(c);
                if c == '`' {
                    state = State::Normal;
                }
            }
            State::LineComment => {
                current.push(c);
                if c == '\n' {
                    state = State::Normal;
                }
            }
            State::BlockComment => {
                current.push(c);
                if c == '*' && chars.peek() == Some(&'/') {
                    current.push(chars.next().unwrap());
                    state = State::Normal;
                }
            }
        }
    }
    flush(&mut current, &mut statements);
    statements
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(column: &str, operator: &str, value: &str) -> FilterCondition {
        FilterCondition {
            id: "1".into(),
            column: column.into(),
            operator: operator.into(),
            value: value.into(),
            value2: None,
        }
    }

    const PG: Dialect = Dialect::Postgres;
    const MY: Dialect = Dialect::Mysql;

    #[test]
    fn builds_simple_comparisons_string_quoted() {
        assert_eq!(
            build_filter_clause(PG, &[f("age", "=", "30")], "and"),
            "WHERE \"age\" = '30'"
        );
        assert_eq!(
            build_filter_clause(PG, &[f("age", ">=", "18")], "and"),
            "WHERE \"age\" >= '18'"
        );
    }

    #[test]
    fn quotes_identifiers_per_engine() {
        assert_eq!(
            build_filter_clause(MY, &[f("age", "=", "30")], "and"),
            "WHERE `age` = '30'"
        );
    }

    #[test]
    fn joins_with_and_or() {
        let filters = [f("a", "=", "1"), f("b", "!=", "2")];
        assert_eq!(
            build_filter_clause(PG, &filters, "and"),
            "WHERE \"a\" = '1' AND \"b\" != '2'"
        );
        assert_eq!(
            build_filter_clause(PG, &filters, "or"),
            "WHERE \"a\" = '1' OR \"b\" != '2'"
        );
    }

    #[test]
    fn builds_like_variants() {
        assert_eq!(
            build_filter_clause(PG, &[f("n", "contains", "x")], "and"),
            "WHERE \"n\" LIKE '%x%'"
        );
        assert_eq!(
            build_filter_clause(PG, &[f("n", "not_contains", "x")], "and"),
            "WHERE \"n\" NOT LIKE '%x%'"
        );
        assert_eq!(
            build_filter_clause(PG, &[f("n", "starts_with", "x")], "and"),
            "WHERE \"n\" LIKE 'x%'"
        );
        assert_eq!(
            build_filter_clause(PG, &[f("n", "ends_with", "x")], "and"),
            "WHERE \"n\" LIKE '%x'"
        );
    }

    #[test]
    fn builds_null_checks_ignoring_value() {
        assert_eq!(
            build_filter_clause(PG, &[f("n", "is_null", "junk")], "and"),
            "WHERE \"n\" IS NULL"
        );
        assert_eq!(
            build_filter_clause(PG, &[f("n", "is_not_null", "")], "and"),
            "WHERE \"n\" IS NOT NULL"
        );
    }

    #[test]
    fn builds_in_from_comma_split_trimmed() {
        assert_eq!(
            build_filter_clause(PG, &[f("id", "in", "1, 2 ,3")], "and"),
            "WHERE \"id\" IN ('1', '2', '3')"
        );
    }

    #[test]
    fn builds_between_from_value2() {
        let mut filter = f("age", "between", "18");
        filter.value2 = Some("65".into());
        assert_eq!(
            build_filter_clause(PG, &[filter], "and"),
            "WHERE \"age\" BETWEEN '18' AND '65'"
        );
    }

    #[test]
    fn escapes_single_quotes_in_values() {
        assert_eq!(
            build_filter_clause(PG, &[f("name", "=", "O'Brien")], "and"),
            "WHERE \"name\" = 'O''Brien'"
        );
    }

    #[test]
    fn escapes_crafted_identifier_and_value() {
        // Neither a crafted column name nor value can break out of the clause.
        assert_eq!(
            build_filter_clause(PG, &[f("a\"b", "=", "x' OR '1'='1")], "and"),
            "WHERE \"a\"\"b\" = 'x'' OR ''1''=''1'"
        );
    }

    #[test]
    fn drops_unknown_operators() {
        assert_eq!(build_filter_clause(PG, &[f("a", "bogus", "1")], "and"), "");
        assert_eq!(
            build_filter_clause(PG, &[f("a", "bogus", "1"), f("b", "=", "2")], "and"),
            "WHERE \"b\" = '2'"
        );
    }

    #[test]
    fn empty_filters_yield_empty_string() {
        assert_eq!(build_filter_clause(PG, &[], "and"), "");
    }

    #[test]
    fn splits_on_top_level_semicolons() {
        assert_eq!(
            split_statements("SELECT 1;\nSELECT 2;"),
            vec!["SELECT 1", "SELECT 2"]
        );
        // A semicolon splits regardless of a following newline.
        assert_eq!(split_statements("SELECT 1; SELECT 2"), vec!["SELECT 1", "SELECT 2"]);
    }

    #[test]
    fn protects_strings_identifiers_and_comments() {
        assert_eq!(split_statements("SELECT ';' AS x"), vec!["SELECT ';' AS x"]);
        assert_eq!(split_statements("SELECT 'a''b;c'"), vec!["SELECT 'a''b;c'"]);
        assert_eq!(split_statements("SELECT \"a;b\""), vec!["SELECT \"a;b\""]);
        assert_eq!(
            split_statements("SELECT 1 -- x; y\n; SELECT 2"),
            vec!["SELECT 1 -- x; y", "SELECT 2"]
        );
        assert_eq!(
            split_statements("SELECT 1 /* ; */ ; SELECT 2"),
            vec!["SELECT 1 /* ; */", "SELECT 2"]
        );
    }

    #[test]
    fn drops_empty_pieces() {
        assert_eq!(split_statements(";\n;\n"), Vec::<String>::new());
        assert_eq!(split_statements(""), Vec::<String>::new());
        assert_eq!(split_statements("a;  \n b;  "), vec!["a", "b"]);
    }
}
