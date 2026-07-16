# Connections

## Connection fields

Server connections use a host, port, database, username, and password. PostgreSQL defaults to port 5432 and MySQL/MariaDB commonly use 3306. SQLite connections use a file path instead.

Names, colors, groups, and tags help distinguish environments. Use explicit names such as “billing staging” rather than ambiguous names such as “main”.

## Test before saving

The **Test** action makes a temporary connection and reports its database version. A failure returns the driver message. Check DNS, port, credentials, database name, TLS settings, and firewall rules in that order.

## Safe modes

- **Default** permits reads and writes.
- **Confirm** is intended for workflows that should ask before potentially destructive actions.
- **Read only** is intended for production or audit access where writes should not occur.

Treat safe modes as an application guardrail, not a database security boundary. A database account with read-only permissions remains the strongest protection for production data.

## SSL

Saved connection records can carry SSL configuration. Availability and certificate validation depend on the native TLS stack and database driver. Prefer verified certificates for remote databases.

## SSH tunnels

Tables starts the system `ssh` command with local port forwarding, then connects the database driver through `127.0.0.1`.

Requirements:

- Key or agent authentication must already work in a terminal.
- The remote SSH host must be able to reach the database host and port.
- Host key acceptance should be established before relying on the connection in a sensitive environment.

Current implementation notes:

- Password prompts are not wired into the app.
- A random ephemeral local port is selected.
- The tunnel process is closed when the database disconnects.

## Startup commands

Optional startup commands run after the connection opens, one nonempty line at a time. Examples include setting a schema or session timeout. Keep these commands idempotent. The current implementation continues if one fails, so verify critical session settings with a query.

## Connection files

Connections are persisted under `~/.tables/` as JSON. Unknown fields round-trip so newer or plugin-provided metadata is preserved. A corrupt connections file is backed up and reset; other metadata files may report an error instead.
