# Data and privacy

Tables is a local desktop application. It does not include analytics or telemetry. Network traffic is limited to the database servers and SSH hosts you configure, plus any operating-system behavior of the native TLS and SSH tools.

## Local data

Application metadata is stored as plain JSON beneath `~/.tables/`:

- saved connections
- query history and favorites
- editor tabs and macros
- settings
- plugin state

Use `TABLES_DIR` to relocate this directory.

## Credentials

Connection records currently persist with the rest of the connection JSON. Protect the account and filesystem accordingly. For sensitive environments, use a restricted database role, short-lived credentials where possible, and full-disk encryption.

## Backups

Back up `~/.tables/` to preserve saved connections and local workflow metadata. Do not treat it as a backup of any database. Use engine-specific backup tools such as `pg_dump`, `mysqldump`, or SQLite file copies made under safe locking conditions.

## Removing local state

Quit Tables and remove the selected Tables directory to reset all local metadata. If `TABLES_DIR` is set, remove that directory instead of `~/.tables/`.
