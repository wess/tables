# Architecture

Tables is a Cargo workspace with a shared service core. Dependencies point downward so the reusable core remains independent of gpui.

```text
app → host → db → model
       ↓      ↓
     store → model
```

## Model

`model` contains serde domain and wire types: connections, rows, schema metadata, query results, filters, settings, transfer results, health, plugins, and analysis. Field names mirror on-disk JSON to preserve round trips.

## Store

`store` persists local metadata as JSON. `TABLES_DIR` overrides the default location. Each domain owns its corruption behavior and tests. The crate has no UI or database driver dependency.

## Database engine

`db` defines the async `Adapter` trait and sqlx implementations for SQLite, PostgreSQL, and MySQL. A registry owns shared live adapters. The health monitor starts one Tokio task per connection, and the tunnel helper owns system SSH child processes.

Arbitrary cells are decoded to `serde_json::Value` by matching driver type information. Engine-specific introspection queries implement tables, columns, indexes, foreign keys, DDL, version, and database lists.

## Host

`host::Host` is the service facade. It owns the registry, health monitor, and active connection cursor, then exposes query, data, schema, import, export, compare, mock, history, favorites, and settings operations. Pure helpers cover SQL literals, CSV parsing, schema diffs, and fake data.

## App

`app` is the only gpui-aware crate. A single bridge runs database futures on a process-wide multithread Tokio runtime and returns outcomes to the gpui thread through a oneshot channel.

The root routes between Home and Workspace. Workspace state is shared across the sidebar, Data, SQL editor, and Structure
surfaces. Per-table view snapshots preserve staged changes without retaining
complete inactive result sets. Tabs share the titlebar; each panel owns its toolbar. Modals provide settings, comparison, diagrams, filters, insertion, review, inspection, charts, and database switching.

## MCP and assistant

`tablesmcp` is a gpui-free stdio binary over `host`, using the saved connection
store and credential resolution. It keeps one read-only connection and one
in-flight database operation. Its four tools have explicit input, response,
and execution-time limits.

`ai` handles assistant HTTP streaming. A bounded channel delivers parsed events
to the UI; cancellation aborts the request. Local metadata writes use temporary
files and atomic replacement, with in-process locks around mutable collections.

## Important invariants

- Every UI database call crosses `app::bridge::run`.
- Core crates never import gpui.
- One live adapter exists per saved connection id.
- Disconnect stops health monitoring and closes any SSH tunnel.
- Pending cell updates and deletes are reviewed before commit.
- Stored JSON preserves unknown connection fields.
