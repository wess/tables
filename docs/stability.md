# Stability review — September 9, 2026

## Changes

- Fixed connection cards that wrapped names and paths into narrow vertical text.
  Added a transparent macOS titlebar following Sinclair's drag/control pattern,
  with Guise Lucide action icons and named tooltips.
- Added table tabs with per-table pages, sort, applied/draft filters, selection,
  hidden columns, and staged edits. Inactive tabs retain view settings and edits,
  not full result sets. SQL editing remains a workspace-wide tab. Closing a table
  with staged edits requires committing or discarding them first.
- Scoped Data/Structure controls to the selected table. Guarded stale row/schema
  completions and commit callbacks across tab switches. Staged cell values now
  display immediately; an empty edited string remains distinct from SQL NULL.
- Kept Guise's update prompt, verified install, and restart flow. Added a titlebar
  check control, immediate automatic-check preference changes, duplicate manual
  check suppression, a 30-second feed deadline, and required release checksums.
  Checks run at launch and hourly when enabled. Installation requires a click.
- Added the standalone [read-only MCP server](mcp.md) and included it in packaging.
- Bounded AI event delivery, reused its HTTP client, added connection/read/total
  deadlines, and replaced the stream parser with incremental UTF-8/CRLF handling.
  Stop, clear, panel dismissal, and workspace departure cancel generation; stale
  callbacks cannot append to a newer conversation. History and prompts have size
  limits. Removed keychain access from rendering and corrected usage accumulation.
- Serialized connection lifecycle changes, stopped health tasks on drop, bounded
  SSH diagnostics, and tied SSH subprocess lifetime to its owner. The GUI bridge
  uses one two-worker Tokio runtime.
- Scoped host cursors to their workspace; enforced database read-only connection
  setup and wired the existing confirmation setting to actual write approvals.
- Bounded ordinary SQL read results to 10,000 rows/16 MiB and grid pages to 1,000.
  Export and backup stream all rows through a small bounded queue and publish
  their destination atomically after success.
- Made metadata writes atomic and serialized in-process read/modify/write
  operations. Enabled native keychain backends on Windows and Linux as well as macOS.
- Updated `h2` and `event-listener` to address available dependency advisories,
  and moved off the yanked `chacha20` release.

## Validation

- UI refinement: graphite surfaces, saved light/dark/system appearance, flat table
  tabs, fixed-height toolbars with overflow menus, a separate pending-edit strip,
  full-width grid rows, and a compact connection list. Verified the running app at
  720 px and wide window sizes, including edits and the narrow assistant drawer.
  All 21 app tests and strict app Clippy passed after the layout changes.
- Follow-up visual direction uses compact editor proportions: a unified 34 px
  titlebar/document strip, 40 px toolbars, and 30 px sidebar rows. Selected tabs use
  background contrast only. Borders and stripes are quieter; backup and restore
  remain in the menu without duplicate titlebar controls. Checked light/dark
  appearance and narrow/wide windows in the running app; build and Clippy pass.
- The unified strip follows Sinclair's layout, with tabs beside native window
  controls, bounded title widths, hover-revealed close controls, and a draggable
  filler. Data/Structure lives in the toolbar below. Direct action icons replace
  generic overflow menus; copy formats retain a dedicated menu.

- 155 workspace tests passing, including real SQLite adapter/host tests and an actual stdio
  MCP subprocess handshake, discovery, schema lookup, pagination, and invalid inputs.
- Regression coverage for AI stream framing/truncation/errors, connection lifecycle,
  read-only setup, result limits, concurrent metadata updates, full exports, failed
  destination preservation, write confirmation, and release-feed validation.
- Strict Clippy across all workspace targets; native macOS development builds.
- Live isolated macOS app: home cards, SQL execution, readable grid, two table tabs,
  page/selection/edit restoration, AI layout, and Guise's “up to date” dialog.
  The test database contains invented sample data, including a 10,000-row table.
- A debug workspace spot check used about 127 MiB RSS. This is not a comparative
  benchmark or a release-build resource guarantee.

## Remaining release verification

Live PostgreSQL/MySQL decoding, TLS/SSH failure scenarios, a real Anthropic stream,
and Windows/Linux execution have not been verified in this environment. The Docker
daemon was unavailable. Signed installation, upgrade/restart, and release installers
need their platform release checks; no release was published or installed here.

The SQL backup is a table/schema export, not an engine-native consistent snapshot.
It does not guarantee complete recovery of every database object's metadata or
consistency across tables while other clients write. Use engine-native backups for
that purpose. Metadata locks serialize this process; simultaneous independent GUI
processes can still overwrite each other's logical updates.

`cargo audit` still reports five vulnerability entries: two advisories for each of
two `quick-xml` versions, plus one for `rsa`. No advisories were suppressed.

- `quick-xml` 0.30.0 is in `xcb` build dependencies; 0.39.4 is in the
  `wayland-scanner` procedural macro. Both inspected callers use plain `Reader`,
  while the namespace-allocation advisory specifically affects `NsReader`.
  These are code-generation dependencies, not Tables parsing user XML at runtime.
  Both advisories require 0.41.0, outside the current parents' version ranges.
  See [RUSTSEC-2026-0195](https://rustsec.org/advisories/RUSTSEC-2026-0195.html)
  and [RUSTSEC-2026-0194](https://rustsec.org/advisories/RUSTSEC-2026-0194.html).
- `rsa` 0.9.10 arrives through `sqlx-mysql`. Its inspected authentication path
  encrypts with the server's public key; it does not perform private-key decryption.
  The advisory concerns private-key timing leakage and has no patched version.
  That narrows the apparent exposure here but does not remove the audit finding.
  See [RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071.html).

The audit also reports unmaintained transitive dependencies. Follow upstream
GPUI/SQLx compatibility updates rather than patching or vendoring those libraries.
