# Getting started

## Requirements

- Rust toolchain with Cargo
- macOS for the current gpui desktop build
- Network access to the database you want to use, or a local SQLite file
- The system `ssh` command when using an SSH tunnel

## Build and launch

From the repository root:

```sh
cargo run -p app
```

The development binary is named `tablesdev`, keeping it separate from an installed release named `tables`.

## Create a connection

1. Select **New connection** on the home screen.
2. Choose PostgreSQL, MySQL, or SQLite.
3. Enter a clear name. For server databases, add the host, port, database, username, and password. For SQLite, choose the database file.
4. Select **Test**. A successful test reports the database version.
5. Select **Create**, then open the connection card.

Tables opens the workspace and loads the available tables and views. Select a table in the sidebar to browse its rows.

## First data workflow

1. Open a table.
2. Select a column heading to sort it.
3. Open **Filters** to add server-side conditions.
4. Double-click or activate a cell to edit it.
5. Review the staged SQL before committing changes.

Edits and deletes are staged. Inserts, imports, and generated mock rows are written immediately, so use a development database while learning the interface.

## First query

1. Open the **Query** tab.
2. Enter `SELECT 1 AS ready;`.
3. Press `⌘↵` or select **Run**.
4. Inspect the result below the editor.

Query history is stored locally. A useful query can be named and saved as a favorite from the query side panel.

## Where Tables stores data

Tables stores application state as readable JSON under `~/.tables/`. Set `TABLES_DIR` to use another directory, which is especially helpful for tests or isolated profiles.
