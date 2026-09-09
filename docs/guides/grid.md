# Data grid

## Browse rows

Selecting a table loads a page of rows from the active database. Paging, sorting, and filters are applied on the server. The status bar shows the active connection, table, and available row count when known.

Each open table preserves its page, sort, draft and applied filters, selection,
hidden columns, and pending edits while switching tabs. Use **Data** in the
selected table's toolbar to return from Structure. Toolbar icons expose refresh,
insert, filters, inspection, copy formats, staged deletion, import, export, and
sample-row generation; hover for labels.

## Sort and resize

Select a column heading to change sorting. Drag column boundaries to resize them, and use horizontal scrolling for wide tables. Sorting refetches the current page rather than rearranging only the visible rows.

## Filter

Open the filter panel and build conditions with AND or OR logic. Supported operations include equality, comparison, contains and pattern variants, null checks, ranges, and lists.

Filter values are bound as parameters and identifiers are quoted. Review filters carefully when switching database engines because pattern matching and type coercion differ between PostgreSQL, MySQL, and SQLite.

## Edit cells

Double-click a cell to edit it. An inline edit becomes a pending change and its
staged value stays visible. Empty text remains an empty string, distinct from NULL. Pending updates and deletes are not written until reviewed and committed. The review view renders the SQL that will be executed.

Use a primary key whenever possible. Stable row identity is essential for precise updates and deletes. Tables containing duplicate rows without a useful key are inherently risky to edit.

## Insert rows

The insert modal builds a new record from the table columns. Inserts are applied immediately and the table is refetched. Omit auto-generated columns when the database provides defaults or sequences.

## Delete rows

Deletes are staged with the other pending changes. Confirm the generated predicate identifies exactly the intended row before committing.

## Import

CSV and TSV imports map the header row to table columns and execute generated inserts in order. Empty fields and the literal `null` become SQL `NULL`; numeric-looking values are emitted as numbers.

For large imports, use the database engine’s bulk loader. The current importer builds statements in memory and executes rows sequentially, which favors transparency over throughput.

## Export

Export a table or query result as CSV, JSON, or SQL. CSV export supports a delimiter, headers, and a custom null representation. Table exports stream all rows through a bounded queue to a temporary file, then
replace the destination after success. Query exports contain the results already
returned to the editor, subject to its 10,000-row / 16 MiB limit. SQL backups are
table/schema exports, not engine-native consistent snapshots.

## Mock rows

Mock generation inspects column names and types to produce plausible values. It understands common identifiers, emails, booleans, numbers, and string lengths. Generated rows are inserted immediately and stop at the first database error.
