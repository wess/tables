# Tables

A fast, native desktop database client for **PostgreSQL**, **MySQL/MariaDB**,
and **SQLite** — an editable data grid, a keyboard-friendly SQL editor, and
schema tools in one app. Built in Rust with [`gpui`](https://github.com/zed-industries/zed)
and [`guise`](https://github.com/wess/guise), backed by async `sqlx` on `tokio`.

## Install

**macOS** — signed and notarized:

```sh
brew install --cask wess/packages/tables
```

**Windows** — via Scoop (builds are beta and unsigned):

```sh
scoop install https://raw.githubusercontent.com/wess/tables/main/packaging/scoop/tables.json
```

**Linux** — a `.deb`, a `.tar.gz` and an AppImage (x86_64 and aarch64) are on
the [releases page](https://github.com/wess/tables/releases/latest).

Tables checks at launch and hourly when enabled. Use the titlebar download icon
or Check for Updates… to check manually. Guise installs signed macOS bundles and
Linux AppImages in place after confirmation; other installs open the download
page. Settings → Updates applies immediately.

## Run from source

```sh
cargo run -p app
```

Connections, history, favorites, and settings persist as plain files under
`~/.tables/`. Tables has no telemetry. Network traffic includes your database and SSH
connections, optional update checks, and assistant requests you initiate.

[Website](https://wess.io/tables/) · [Documentation](docs/README.md) ·
[Release notes](CHANGELOG.md)

## Features

- **Compact native workspace** — table tabs share the titlebar. Data/Structure
  and grouped action icons sit below them, with descriptive tooltips and no
  wrapping toolbar. Light, dark, and system appearance are supported.

- **Editable data grid** — inline cell editing, multi-select, column sort,
  drag-to-resize, horizontal scroll, and pagination. Edits stage as pending
  changes you review as SQL and commit as a batch. Table tabs preserve their
  page, filters, selection, and edits, with controls scoped to the selected table.
- **SQL editor** — syntax-highlighted, multi-statement execution (⌘↵), results
  in a grid, query history, and saved favorites.
- **Schema tools** — columns, indexes, foreign keys, DDL, per-column profiling,
  schema comparison against another connection, and an ER diagram.
- **Filtering** — a filter panel with 14 operators and AND/OR logic.
- **Charts** — bar / line / pie over any query result.
- **Import/export** — CSV/TSV import and CSV/JSON/SQL export, plus type-aware
  mock-data generation.
- **Multi-engine** — Postgres, MySQL/MariaDB, and SQLite behind the same grid.
- **AI assistant** — an optional Claude panel that knows your schema and dialect,
  streams its answer, and puts Run / Insert on the SQL it writes. Bring your own
  Anthropic API key or Claude subscription token; it is stored in your OS
  keychain and no assistant runs until you add one.
- **MCP server** — a standalone, read-only stdio server for saved connections,
  schemas, and bounded row pages. See [setup and tools](docs/mcp.md).
- **Command palette** — ⌘P to jump to a table or action.

## Architecture

A Cargo workspace layered bottom-up: `model` (shared types) → `store` (local
JSON persistence) → `db` (async `sqlx` engine layer) → `host` (service facade) →
`app` (the gpui UI). `tablesmcp` uses the same host facade without the UI;
`ai` provides the optional streaming assistant transport.

Release notes are in [`CHANGELOG.md`](CHANGELOG.md).

## License

MIT © Wess Cope

♥ [Sponsor this project](https://github.com/sponsors/wess)
