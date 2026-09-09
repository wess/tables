# Data and privacy

Tables is a local desktop application. It does not include analytics or telemetry. Network traffic includes the database servers and SSH hosts you configure,
GitHub release checks when enabled, and optional assistant requests you initiate.
Assistant requests include the conversation and schema context; query results
can also be included when you enable that setting.

## Local data

Application metadata is stored as plain JSON beneath `~/.tables/`:

- saved connections
- query history and favorites
- editor tabs and macros
- settings
- plugin state

Use `TABLES_DIR` to relocate this directory.

## Credentials

Connection passwords are saved to the OS credential store when available. A
legacy plaintext password is migrated after a successful credential-store write.
If the credential store is unavailable, connection passwords can remain in the
JSON record. Protect the account and filesystem accordingly. Assistant credentials
use the OS credential store. For sensitive environments, use a restricted database role, short-lived credentials where possible, and full-disk encryption.

## MCP clients

The optional `tablesmcp` process exposes saved connections, schemas, and rows to
the client that starts it. It uses stdio and opens no HTTP listener. Database
sessions are read-only and skip startup SQL. The desktop app need not be open.
Use a dedicated database role and `TABLES_DIR` profile to limit what a client can
access. See the [MCP guide](../mcp.md).

## Backups

Back up `~/.tables/` to preserve saved connections and local workflow metadata. Do not treat it as a backup of any database. Use engine-specific backup tools such as `pg_dump`, `mysqldump`, or SQLite file copies made under safe locking conditions.

## Removing local state

Quit Tables and remove the selected Tables directory to reset all local metadata. If `TABLES_DIR` is set, remove that directory instead of `~/.tables/`.
