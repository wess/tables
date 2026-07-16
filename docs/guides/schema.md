# Schema tools

## Structure view

The Structure tab shows columns, types, nullability, defaults, indexes, foreign keys, and database-provided DDL for the selected table.

## Column profiles

Profiling summarizes a column with counts and frequent values. Profiling queries can scan substantial data; use them carefully on large production tables without suitable indexes.

## Schema comparison

Schema comparison loads tables and columns from two live connections, then reports additions, removals, and changed definitions. The generated SQL is a starting point for review, not a migration system.

Engine differences matter. Type names, defaults, quoting, auto-increment behavior, and `ALTER TABLE` support vary. Always test generated statements against a disposable copy and use your normal migration tooling for production changes.

## ER diagram

The ER diagram visualizes tables and foreign-key relationships. Edges are drawn from live schema metadata. Databases without declared foreign keys cannot infer relationships from naming conventions alone.

## Refresh behavior

Schema information is fetched from the active connection. After external DDL changes, reconnect or reselect the relevant surface to ensure cached interface state is refreshed.
