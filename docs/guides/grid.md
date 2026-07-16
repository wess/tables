# Data grid

## Browse rows

Selecting a table loads a page of rows from the active database. Paging, sorting, and filters are applied on the server. The status bar shows the active connection, table, and available row count when known.

## Sort and resize

Select a column heading to change sorting. Drag column boundaries to resize them, and use horizontal scrolling for wide tables. Sorting refetches the current page rather than rearranging only the visible rows.

## Filter

Open the filter panel and build conditions with AND or OR logic. Supported operations include equality, comparison, contains and pattern variants, null checks, ranges, and lists.

Filter values become SQL literals and identifiers are quoted. Review filters carefully when switching database engines because pattern matching and type coercion differ between PostgreSQL, MySQL, and SQLite.

## Edit cells

An inline edit becomes a pending change. Pending updates and deletes are not written until reviewed and committed. The review view renders the SQL that will be executed.

Use a primary key whenever possible. Stable row identity is essential for precise updates and deletes. Tables containing duplicate rows without a useful key are inherently risky to edit.

## Insert rows

The insert modal builds a new record from the table columns. Inserts are applied immediately and the table is refetched. Omit auto-generated columns when the database provides defaults or sequences.

## Delete rows

Deletes are staged with the other pending changes. Confirm the generated predicate identifies exactly the intended row before committing.

## Import

CSV and TSV imports map the header row to table columns and execute generated inserts in order. Empty fields and the literal `null` become SQL `NULL`; numeric-looking values are emitted as numbers.

For large imports, use the database engine’s bulk loader. The current importer builds statements in memory and executes rows sequentially, which favors transparency over throughput.

## Export

Export a table or query result as CSV, JSON, or SQL. CSV export supports a delimiter, headers, and a custom null representation. Exports currently materialize the complete result in memory, so constrain very large datasets with a query.

## Mock rows

Mock generation inspects column names and types to produce plausible values. It understands common identifiers, emails, booleans, numbers, and string lengths. Generated rows are inserted immediately and stop at the first database error.
