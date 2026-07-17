//! Parse a database connection string into a `StoredConnection` so pasting a
//! URL prefills a new-connection form.

use model::StoredConnection;

/// Decode `%XX` escapes in a URL component (userinfo/password). Invalid escapes
/// are left as-is.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn blank(kind: &str) -> StoredConnection {
    StoredConnection {
        id: String::new(),
        name: String::new(),
        kind: kind.to_string(),
        host: String::new(),
        port: 0,
        database: String::new(),
        username: String::new(),
        password: String::new(),
        color: String::new(),
        filepath: None,
        ssl: None,
        ssh: None,
        startup_commands: None,
        safe_mode: None,
        group: None,
        tags: None,
        extra: serde_json::Map::new(),
    }
}

/// Parse `scheme://[user[:pass]@]host[:port]/db[?params]` (or a sqlite file URL)
/// into a connection. Returns `None` for an unrecognized scheme.
pub(super) fn parse_conn_url(input: &str) -> Option<StoredConnection> {
    let input = input.trim();
    let (scheme, rest) = input.split_once("://")?;
    let kind = match scheme.to_ascii_lowercase().as_str() {
        "postgres" | "postgresql" => "postgres",
        "mysql" | "mariadb" => "mysql",
        "sqlite" | "sqlite3" | "file" => {
            let path = rest.split('?').next().unwrap_or(rest);
            let mut conn = blank("sqlite");
            conn.filepath = Some(path.to_string());
            conn.name = path.rsplit('/').next().unwrap_or(path).to_string();
            return Some(conn);
        }
        _ => return None,
    };

    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (userinfo, hostport) = match authority.rsplit_once('@') {
        Some((u, h)) => (Some(u), h),
        None => (None, authority),
    };
    let (username, password) = match userinfo {
        Some(ui) => match ui.split_once(':') {
            Some((u, p)) => (percent_decode(u), percent_decode(p)),
            None => (percent_decode(ui), String::new()),
        },
        None => (String::new(), String::new()),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse().unwrap_or(0)),
        None => (hostport.to_string(), 0),
    };
    let database = path.split('?').next().unwrap_or(path).to_string();

    let mut conn = blank(kind);
    conn.name = if database.is_empty() { host.clone() } else { database.clone() };
    conn.host = host;
    conn.port = port;
    conn.database = database;
    conn.username = username;
    conn.password = password;
    Some(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_postgres_url() {
        let c = parse_conn_url("postgres://bob:s%40cret@db.example.com:5433/shop").unwrap();
        assert_eq!(c.kind, "postgres");
        assert_eq!(c.host, "db.example.com");
        assert_eq!(c.port, 5433);
        assert_eq!(c.database, "shop");
        assert_eq!(c.username, "bob");
        assert_eq!(c.password, "s@cret"); // %40 decoded
        assert_eq!(c.name, "shop");
    }

    #[test]
    fn parses_mysql_without_port_or_auth() {
        let c = parse_conn_url("mysql://localhost/app").unwrap();
        assert_eq!(c.kind, "mysql");
        assert_eq!(c.host, "localhost");
        assert_eq!(c.port, 0);
        assert_eq!(c.database, "app");
        assert!(c.username.is_empty());
    }

    #[test]
    fn parses_sqlite_file_and_rejects_unknown() {
        let c = parse_conn_url("sqlite:///data/app.db").unwrap();
        assert_eq!(c.kind, "sqlite");
        assert_eq!(c.filepath.as_deref(), Some("/data/app.db"));
        assert!(parse_conn_url("redis://x").is_none());
    }
}
