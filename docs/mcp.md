# Tables MCP server

`tablesmcp` is a separate, read-only MCP server over stdio. It uses the same saved
connections and OS keychain as Tables. The desktop app does not need to be open.
It starts only when an MCP client launches it; it opens no HTTP listener.

Build from source:

```sh
cargo build --release -p tablesmcp
```

Configure your MCP client with an absolute executable path. For a packaged macOS
installation:

```json
{
  "mcpServers": {
    "tables": {
      "command": "/Applications/Tables.app/Contents/MacOS/tablesmcp",
      "args": []
    }
  }
}
```

For a source build, use `/absolute/path/to/tables/target/release/tablesmcp`.
Linux `.deb` and tar packages include `tablesmcp` beside `tables`; Windows packages
include `tablesmcp.exe`. AppImage users can extract the image and use its
`usr/bin/tablesmcp`, or build the standalone server.

The default metadata directory is `~/.tables`. Set `TABLES_DIR` in the MCP client's
`env` configuration to use a separate collection of connections. Save connections
in Tables first. The server does not create connections, accept credentials as tool
arguments, or return stored passwords, hostnames, or file paths in its connection
listing. Database errors and database content can still contain identifying details.

## Tools

| Tool | Arguments | Result |
| --- | --- | --- |
| `list_connections` | None | Saved connection IDs, names, database types |
| `list_tables` | `connection` | Tables and views |
| `table_schema` | `connection`, `table` | Columns, indexes, foreign keys |
| `table_rows` | `connection`, `table`, optional `limit` and `offset` | Row page, columns, `nextOffset`, `truncatedCells` |

Use the returned connection ID as `connection`. Row pages default to 50 and are
limited to 200 rows. Follow `nextOffset` until it is null. Tables with a primary key
are ordered by that key; pagination without a primary key can change between calls.
Strings longer than 4 KiB are truncated and counted in `truncatedCells`. A decoded
result exceeding the byte budget returns an error; use a smaller page.

## Access and limits

Connecting an AI client gives that client access to data in the selected Tables
metadata directory. Use database accounts with the permissions you intend to expose.
Every connection the MCP server opens uses database read-only mode and skips saved
startup SQL. There is no arbitrary SQL or write tool.

The server retains one database connection at a time, accepts one database request
at a time, and uses two Tokio workers. Incoming lines are capped at 64 KiB; tool
payloads at 512 KiB (the response contains both text and structured representations).
Calls time out after 30 seconds, with up to five seconds for cleanup. Cancellation,
EOF, and failed calls trigger connection cleanup. A single driver-decoded cell can
allocate memory before the result budget is checked.

Protocol output goes to stdout; diagnostics go to stderr. Configure the executable
directly, without a shell wrapper that writes banners to stdout.

Run the protocol integration test with:

```sh
cargo test -p tablesmcp --test stdio
```
