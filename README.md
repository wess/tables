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

Tables checks for a new release at launch and installs it in place when you say
so; turn the check off in Settings → Updates.

## Run from source

```sh
cargo run -p app
```

Connections, history, favorites, and settings persist as plain files under
`~/.tables/`. Nothing phones home — the update check and the optional AI
assistant are the only network calls, and both are yours to turn off.

## Features

- **Editable data grid** — inline cell editing, multi-select, column sort,
  drag-to-resize, horizontal scroll, and pagination. Edits stage as pending
  changes you review as SQL and commit as a batch.
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
- **Command palette** — ⌘P to jump to a table or action.

## Architecture

A Cargo workspace layered bottom-up: `model` (shared types) → `store` (local
JSON persistence) → `db` (async `sqlx` engine layer) → `host` (service facade) →
`app` (the gpui UI).

Release notes are in [`CHANGELOG.md`](CHANGELOG.md).

## License

MIT © Wess Cope

♥ [Sponsor this project](https://github.com/sponsors/wess)
