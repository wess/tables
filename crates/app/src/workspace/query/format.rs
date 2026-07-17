//! A lightweight SQL beautifier: collapses whitespace, uppercases recognized
//! keywords, and breaks the major clauses onto their own lines. String literals
//! are tokenized so their contents are never touched.

use super::complete::KEYWORDS;

/// Clause keywords that start a new line.
const CLAUSE: &[&str] = &[
    "SELECT", "FROM", "WHERE", "GROUP", "ORDER", "HAVING", "LIMIT", "OFFSET", "UNION", "VALUES",
    "SET", "JOIN", "INNER", "LEFT", "RIGHT", "FULL", "CROSS",
];

enum Tok {
    Word(String),
    Str(String),
    Sym(char),
}

fn tokenize(sql: &str) -> Vec<Tok> {
    let mut toks = Vec::new();
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '\'' {
            // A single-quoted string; '' is an embedded quote.
            let mut s = String::from(c);
            i += 1;
            while i < chars.len() {
                s.push(chars[i]);
                if chars[i] == '\'' {
                    if chars.get(i + 1) == Some(&'\'') {
                        s.push('\'');
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
            toks.push(Tok::Str(s));
        } else if c.is_alphanumeric() || c == '_' {
            let mut w = String::new();
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                w.push(chars[i]);
                i += 1;
            }
            toks.push(Tok::Word(w));
        } else {
            toks.push(Tok::Sym(c));
            i += 1;
        }
    }
    toks
}

/// Append `text` to `out` with a separating space unless the boundary forbids
/// one (start of line, after `(`/`.`, or before `,`/`)`/`;`/`.`).
fn push_token(out: &mut String, text: &str) {
    let no_space_before = matches!(text, "," | ")" | ";" | ".");
    let suppress = out.is_empty()
        || out.ends_with('\n')
        || out.ends_with('(')
        || out.ends_with('.')
        || no_space_before;
    if !suppress {
        out.push(' ');
    }
    out.push_str(text);
}

pub(super) fn format_sql(sql: &str) -> String {
    let mut out = String::new();
    for tok in tokenize(sql) {
        match tok {
            Tok::Word(w) => {
                let upper = w.to_uppercase();
                let is_kw = KEYWORDS.contains(&upper.as_str());
                if is_kw && CLAUSE.contains(&upper.as_str()) && !out.is_empty() {
                    out.push('\n');
                }
                push_token(&mut out, if is_kw { &upper } else { &w });
            }
            Tok::Str(s) => push_token(&mut out, &s),
            Tok::Sym(c) => push_token(&mut out, &c.to_string()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uppercases_keywords_and_breaks_clauses() {
        let out = format_sql("select id, name from users where id = 1 order by name");
        assert_eq!(out, "SELECT id, name\nFROM users\nWHERE id = 1\nORDER BY name");
    }

    #[test]
    fn preserves_string_literals() {
        let out = format_sql("select * from t where name = 'from where SELECT'");
        assert_eq!(out, "SELECT *\nFROM t\nWHERE name = 'from where SELECT'");
    }
}
