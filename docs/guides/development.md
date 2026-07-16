# Development

## Common commands

```sh
cargo run -p app
cargo build
cargo test
cargo test -p db
cargo test -p host end_to_end
cargo clippy --all-targets
```

## Workspace boundaries

Keep `model`, `store`, `db`, and `host` free of gpui. UI requests belong in `app` and must use the shared runtime bridge. Add reusable pure behavior at the lowest sensible layer.

## Style

- Prefer free functions and plain data over classes or class-like abstractions.
- Keep files small and focused.
- Use lowercase file names without spaces, hyphens, or underscores.
- Split compound concepts into directories rather than compound file names.
- Preserve existing user changes and keep source edits scoped.

## Testing strategy

Pure logic belongs under unit tests. SQLite adapter tests use `:memory:` or temporary files. The host end-to-end test uses a real on-disk SQLite database. PostgreSQL and MySQL pure logic can be unit tested locally, while decode and integration paths require live servers.

When changing query construction, test quoting, nulls, booleans, numeric values, empty inputs, and engine differences. When changing persistence, test missing, corrupt, and unknown-field round trips.

## Release behavior

The development package builds `tablesdev`. Release packaging installs the same source as `tables`. The release profile enables thin LTO.
