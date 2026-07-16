# Tables

A fast, native desktop database client for **PostgreSQL**, **MySQL/MariaDB**,
and **SQLite** — an editable data grid, a keyboard-friendly SQL editor, and
schema tools in one app. Built in Rust with [`gpui`](https://github.com/zed-industries/zed)
and [`guise`](https://github.com/wess/guise), backed by async `sqlx` on `tokio`.

## Run

```sh
cargo run -p app
```

Connections, history, favorites, and settings persist as plain files under
`~/.tables/`. Nothing phones home.

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
- **Command palette** — ⌘P to jump to a table or action.

## Architecture

A Cargo workspace layered bottom-up: `model` (shared types) → `store` (local
JSON persistence) → `db` (async `sqlx` engine layer) → `host` (service facade) →
`app` (the gpui UI). See [`CLAUDE.md`](CLAUDE.md) for the full map.

## License

MIT © Wess Cope
