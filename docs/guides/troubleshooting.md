# Troubleshooting

## The app does not build

Run `cargo build` from the workspace root and read the first compiler error. Ensure the Rust toolchain and macOS development tools are installed. A clean dependency build can take several minutes.

## Connection test fails

- Confirm the host resolves and the port is reachable.
- Verify the database name and credentials with the engine’s command-line client.
- Check server listen addresses, user grants, and firewall rules.
- Confirm TLS requirements match the saved connection.
- For SSH, verify the same host and key work with the system `ssh` command.

## SSH tunnel exits

Tables depends on noninteractive key or agent authentication. Establish the host key and validate forwarding in a terminal first. A busy randomly selected local port can also cause a rare failure; retrying selects another port.

## No tables appear

The connection may have opened successfully while the account lacks schema visibility. Check the selected database, current schema, and grants. Views appear alongside tables when the engine reports them.

## A query script splits incorrectly

The multi-statement splitter is line-oriented, not a full parser. Embedded semicolons in strings or procedural SQL can split unexpectedly. Run the statement independently or use the engine’s native client for complex scripts.

## History or favorites disappear

Inspect the relevant JSON under `~/.tables/`. Corrupt history and favorites files return an error; corrupt connection data is backed up and reset. Restore a known-good backup when available.

## Export uses too much memory

Exports currently load the full result before serialization. Add a restrictive query, export in logical ranges, or use the database engine’s streaming export tool for large datasets.

## The interface shows stale data

After a write, refetch the table or reselect it. After external schema changes, reconnect. If a request finishes after a fast table switch, reselect the intended table to ensure the latest response owns the view.
