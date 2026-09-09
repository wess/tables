# Getting started

## Requirements

- Rust toolchain with Cargo
- macOS, Linux with the required native libraries, or Windows (beta)
- Network access to the database you want to use, or a local SQLite file
- The system `ssh` command when using an SSH tunnel

## Install a release

On macOS: `brew install --cask wess/packages/tables`.
Linux packages and beta Windows installers are available on the
[releases page](https://github.com/wess/tables/releases/latest).
The release includes the optional [MCP server](../../mcp.md).

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
5. Select **Create**, then select **Connect** on its row.

Tables opens the workspace and loads the available tables and views. Select a table in the sidebar to open its tab. Tabs share the titlebar; the
Data/Structure switch and action icons are in the toolbar below. Hover an icon
for its action name.

## First data workflow

1. Open a table.
2. Select a column heading to sort it.
3. Open **Filters** to add server-side conditions.
4. Double-click or activate a cell to edit it.
5. Review the staged SQL before committing changes.

Edits and deletes are staged. Inserts, imports, and generated mock rows are written immediately, so use a development database while learning the interface.

## First query

1. Open the **SQL editor** tab.
2. Enter `SELECT 1 AS ready;`.
3. Press `⌘↵` or select **Run SQL**.
4. Inspect the result below the editor.

Query history is stored locally. A useful query can be named and saved as a favorite from the query side panel.

## Where Tables stores data

Tables stores application state as readable JSON under `~/.tables/`. Set `TABLES_DIR` to use another directory, which is especially helpful for tests or isolated profiles.
