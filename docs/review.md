# Stability, performance, and usability review

Reviewed July 16, 2026 against the Rust workspace at version 2.0.0. The review included source inspection, the full workspace test suite, and Clippy across all targets and features.

## Executive summary

Tables has a strong architectural foundation: strict crate layering, one async bridge between gpui and Tokio, engine adapters behind a small trait, local persistence with corruption tests, and meaningful SQLite end-to-end coverage. All 96 workspace tests passed. The main risks are concentrated in SQL construction, lifecycle cleanup, unbounded in-memory operations, and UI request races rather than broad structural problems.

The application is suitable for continued development and controlled daily use. Before calling it production-hardened for large or sensitive databases, prioritize identifier escaping and parameterization, transactional batch operations, cancellation or response ownership, secure credential storage, and live PostgreSQL/MySQL integration coverage.

## Findings by priority

### Critical: identifier quoting does not escape embedded quote characters

`db::filters::quote_ident` wraps names but does not escape an embedded double quote. Table and column names originate primarily from database metadata, yet crafted identifiers could produce invalid SQL or alter generated statements. Use engine-aware identifier quoting and double embedded quote characters. Add cases for quotes, dots, reserved words, and Unicode.

### High: writes and filters are assembled as SQL strings

Filters escape single quotes, and value helpers cover common JSON values, but string assembly remains harder to secure and type correctly than driver parameters. It also makes binary, decimal, temporal, JSON, and engine-specific values fragile. Introduce parameterized statement builders per adapter, retaining SQL rendering only for the review preview.

### High: batch changes and imports are not transactional

CSV import, mock insertion, and reviewed changes execute statements sequentially. A failure leaves earlier rows committed. Wrap a reviewed batch in one transaction where supported, report the failing statement, and allow rollback. For large CSV files, use chunked transactions or native bulk protocols.

### High: exports and imports materialize complete data in memory

Table export runs `SELECT *`, retains all rows, then builds another complete serialized string. CSV input is also read fully and expanded into statements. This can exhaust memory on large tables. Stream rows to a buffered writer and parse CSV incrementally. Add progress and cancellation.

### High: asynchronous UI responses have no cancellation or ownership token

The bridge safely returns work to the UI thread, but detached tasks cannot be cancelled and callbacks do not prove they still correspond to the selected table, page, or filter. Fast navigation can let an older response overwrite newer state. Track request generations or cancellation handles per surface and ignore stale completions.

### High: credentials are stored as plain JSON

Saved passwords share the local connection file. Move secrets to the macOS Keychain and store only a reference in JSON. Until then, document the behavior prominently and encourage restricted database roles.

### Medium: connection setup can leak an SSH tunnel on adapter failure

The registry opens a tunnel before connecting the adapter. If adapter creation or connection fails, the function returns without closing the newly opened child. Ensure every failure path closes the tunnel, ideally through a guard.

### Medium: SSH tunnel verification and host-key policy need hardening

The tunnel selects an unchecked random local port, waits for early process exit, and disables strict host-key checking. Bind a local socket to reserve a port, capture and surface stderr, probe readiness, and default to normal known-host verification. Make any relaxed policy an explicit user choice.

### Medium: startup command failures are silent

Failed startup statements are discarded, so session safety settings may not apply even though the connection appears ready. Return a structured warning or fail the connection when a command is marked required.

### Medium: health begins as healthy before the first probe

The monitor labels a connection healthy immediately and waits 30 seconds before checking. Use a connecting or unknown state until the first probe, run the first probe immediately, and expose the last error and check time.

### Medium: SQL script splitting is deliberately incomplete

The splitter does not protect literals and cannot reliably handle procedural SQL, comments, or dialect-specific bodies. Replace it with a dialect-aware parser or clearly offer a “run as one script” mode.

### Medium: active connection is global host state

Many host operations rely on one active connection cursor. This simplifies the current single workspace but makes concurrent windows or operations vulnerable to cross-connection mistakes. Pass connection ids explicitly through service calls before adding multiwindow workflows.

### Medium: large modules increase regression risk

Several UI and adapter files are 400–700 lines. Split data workflows by browsing, pending changes, transfer, and inspector concerns; split adapters by decoding, introspection, and query execution. This aligns with the project’s small-file convention and makes focused tests easier.

### Low: errors are sometimes converted to empty states

Table and database loading use `unwrap_or_default`, and history or favorites reads do the same in UI callbacks. A failed request can look like “no tables” or “no history.” Preserve the last good state and show a retryable error with technical details.

### Low: Clippy is not clean across all targets

The audit found one needless lifetime warning and one denied approximate-constant lint in a test using `3.14`. These do not affect runtime behavior but should be fixed so linting can remain a useful CI gate.

## Performance assessment

Positive traits include native rendering, pooled async drivers, a shared multithread runtime, server-side paging and filtering, limited mutex hold duration, and thin LTO for release. Likely bottlenecks are complete-result decoding into JSON, full in-memory transfer operations, one-statement-per-row writes, repeated schema calls, and rendering every visible table or result element without explicit virtualization.

Recommended measurement plan:

1. Add tracing spans for connect, introspection, fetch, decode, render, import, and export.
2. Benchmark 10 thousand, 100 thousand, and 1 million row transfers.
3. Profile grid rendering at 50, 200, and 1,000 visible rows with 10–100 columns.
4. Record time to first table and time to first row on high-latency connections.
5. Add pool, query timeout, page size, and fetch cancellation settings.

## Usability assessment

The core model is coherent: connections lead to a workspace; the workspace separates Data, Query, and Structure; destructive grid edits stage for review; and the command palette supports quick navigation. Empty states, load indicators, and query errors exist.

The largest usability gaps are discoverability and feedback. The sidebar lacks search for large schemas outside the command palette, destructive safety modes are not visibly reinforced in the workspace, some failures collapse into empty states, inserts/imports/mock rows bypass staged review, and connection fields do not expose the full saved SSL/SSH configuration in the inspected form. Add explicit environment and safe-mode badges, searchable/grouped schemas, first-run guidance, progress and cancel controls, retry actions, and a consistent write-confirmation policy.

## Test coverage

Strengths:

- 96 passing tests across app, db, host, and store
- SQLite adapter introspection and query behavior
- Host end-to-end create, browse, update, and delete path
- Corrupt and missing persistence files
- Filters, CSV, schema comparison, values, and mock generation

Important missing coverage:

- Live PostgreSQL and MySQL decoding and introspection
- SSH lifecycle and failed-connect cleanup
- TLS combinations
- Concurrent connect and disconnect calls for one id
- Transaction rollback during reviewed changes and imports
- Request race behavior in the UI
- Large dataset memory and render benchmarks
- Keyboard focus, screen reader semantics, contrast, and reduced motion

## Recommended roadmap

1. Secure SQL construction and identifier quoting.
2. Add transactions, cancellation, timeouts, and stale-response protection.
3. Stream transfers and add progress.
4. Move credentials to Keychain and harden SSH defaults.
5. Add live engine CI and concurrency tests.
6. Improve errors, environment visibility, schema search, and onboarding.
7. Split the largest files and add tracing plus performance benchmarks.
