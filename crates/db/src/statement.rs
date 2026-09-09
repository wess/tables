pub fn head(mut sql: &str) -> &str {
  loop {
    sql = sql.trim_start();
    if sql.starts_with("--") {
      sql = sql.find('\n').map(|end| &sql[end + 1..]).unwrap_or("");
    } else if sql.starts_with("/*") {
      let bytes = sql.as_bytes();
      let (mut depth, mut end) = (1, 2);
      while depth > 0 && end + 1 < bytes.len() {
        match &bytes[end..end + 2] {
          b"/*" => {
            depth += 1;
            end += 2;
          }
          b"*/" => {
            depth -= 1;
            end += 2;
          }
          _ => end += 1,
        }
      }
      if depth > 0 {
        return "";
      }
      sql = &sql[end..];
    } else {
      return sql;
    }
  }
}
