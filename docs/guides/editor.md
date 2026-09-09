# SQL editor

## Run SQL

Enter SQL in the SQL editor tab and press `⌘↵`, or select **Run SQL**. Multiple statements are supported. Each statement produces its own result record, including returned columns and rows or the affected row count.

The statement splitter recognizes quoted strings, quoted identifiers, and SQL
comments. It is not a complete dialect parser; use engine-native tools for
procedural scripts, dollar-quoted bodies, and client delimiter commands.

## Results

Read results appear in a grid. Write statements report affected rows. A failed run clears the current results and displays the database error.

Read results are bounded to 10,000 rows / 16 MiB. Use table export when you need
all rows. The toolbar provides direct controls for formatting, history, favorites,
explain, transaction execution, charts, export, and copying Markdown. Actions
that require SQL or results are disabled until those are available.

## History

Executed queries are saved locally with execution time and any error. Open **History** to inspect recent statements and load one back into the editor. History is capped to prevent unlimited file growth.

## Favorites

Open **Favorites**, provide a name, and save the current editor text. Selecting a favorite loads its SQL. Removing a favorite deletes it from local metadata only.

## Charts

When a result contains columns and rows, open the chart action to create a bar, line, or pie view. Choose columns with compatible label and numeric data. Charts are exploratory and do not modify the database.

## Safety practices

- Use transactions for related changes when supported by your script.
- Add a `WHERE` clause before running `UPDATE` or `DELETE`.
- Use a database read-only account for investigation.
- Start with a `SELECT` using the same predicate as a planned write.
- Keep backups for schema or bulk changes.
