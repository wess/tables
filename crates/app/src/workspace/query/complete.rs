//! SQL autocomplete candidates: a keyword list plus the connection's schema
//! identifiers. Pure matching so it can be unit-tested.

/// Common SQL keywords offered as completions.
pub(super) const KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "INSERT", "INTO", "VALUES", "UPDATE", "SET", "DELETE", "CREATE",
    "TABLE", "ALTER", "DROP", "INDEX", "JOIN", "INNER", "LEFT", "RIGHT", "OUTER", "FULL", "CROSS",
    "ON", "GROUP", "ORDER", "BY", "LIMIT", "OFFSET", "HAVING", "DISTINCT", "AND", "OR", "NOT",
    "NULL", "IN", "LIKE", "BETWEEN", "IS", "COUNT", "SUM", "AVG", "MIN", "MAX", "ASC", "DESC",
    "UNION", "ALL", "EXISTS", "CASE", "WHEN", "THEN", "ELSE", "END", "PRIMARY", "KEY", "FOREIGN",
    "REFERENCES", "DEFAULT", "UNIQUE", "CONSTRAINT", "RETURNING", "WITH", "VIEW", "AS",
];

const MAX: usize = 8;

/// Up to `MAX` completions for `word`: keyword prefix matches first, then schema
/// identifiers. Case-insensitive; an exact match yields nothing (nothing to
/// complete). Words shorter than two characters are ignored to cut noise.
pub(super) fn suggestions(word: &str, schema: &[String]) -> Vec<String> {
    let lower = word.to_lowercase();
    if lower.len() < 2 {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    let candidates = KEYWORDS.iter().map(|s| s.to_string()).chain(schema.iter().cloned());
    for cand in candidates {
        if out.len() >= MAX {
            break;
        }
        let cl = cand.to_lowercase();
        if cl.starts_with(&lower) && cl != lower && !out.iter().any(|o| o.eq_ignore_ascii_case(&cand))
        {
            out.push(cand);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_keywords_case_insensitively() {
        let s = suggestions("sel", &[]);
        assert_eq!(s, vec!["SELECT"]);
    }

    #[test]
    fn includes_schema_and_skips_exact_and_short() {
        let schema = vec!["users".to_string(), "user_id".to_string()];
        let s = suggestions("user", &schema);
        assert_eq!(s, vec!["users", "user_id"]);
        assert!(suggestions("u", &schema).is_empty()); // too short
        assert!(suggestions("users", &schema).is_empty()); // exact match, nothing to add
    }
}
