# Tables documentation

Tables is a native desktop client for PostgreSQL, MySQL, MariaDB, and SQLite. It combines an editable data grid, SQL editor, schema inspection, import and export tools, diagrams, and charts in one local application.

## Start here

- [Getting started](guides/getting/started.md) — build, launch, and make a first connection.
- [Connections](guides/connections.md) — database engines, SSL, SSH, safe modes, and saved connection data.
- [Data grid](guides/grid.md) — browse, filter, sort, edit, insert, delete, import, export, and generate mock rows.
- [SQL editor](guides/editor.md) — run statements, inspect results, save favorites, and use history and charts.
- [Schema tools](guides/schema.md) — structure, DDL, profiling, comparison, and ER diagrams.
- [Keyboard reference](guides/keyboard.md) — shortcuts and efficient workflows.
- [Data and privacy](guides/privacy.md) — local files, credentials, network behavior, and backups.
- [Troubleshooting](guides/troubleshooting.md) — common connection, query, and display problems.
- [Architecture](guides/architecture.md) — workspace boundaries, async runtime, persistence, and extension points.
- [Development](guides/development.md) — build, test, lint, and contribution conventions.
- [Project review](review.md) — stability, performance, and usability findings from the July 2026 audit.

## Supported databases

| Engine | Connection | Data grid | Query editor | Schema inspection |
| --- | --- | --- | --- | --- |
| PostgreSQL | TCP, SSL, SSH tunnel | Yes | Yes | Yes |
| MySQL | TCP, SSL, SSH tunnel | Yes | Yes | Yes |
| MariaDB | MySQL protocol | Yes | Yes | Yes |
| SQLite | Local file or `:memory:` | Yes | Yes | Yes |

## Documentation conventions

Keyboard shortcuts use macOS symbols because the current native application targets macOS. SQL examples are intentionally small and should be adapted to the selected database engine. Actions that modify data are called out explicitly.
