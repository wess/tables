# Changelog

Notable changes to Tables. Versions follow [semver](https://semver.org).

## 0.2.0 — 2026-08-21

Built against **guise 1.2.1**, up from 0.13. Most of this release is Tables
adopting things guise now ships that Tables had been hand-rolling — and the
handful of bugs that surfaced while wiring them in.

### The app can update itself

**Tables → Check for Updates…**, and an automatic check at launch and hourly
that **Settings → Updates** can turn off. The check is the only automatic part;
installing is always a click.

On macOS the downloaded `.dmg` is verified against the Developer ID the release
is notarized under before anything is executed — without that requirement guise
refuses the install and opens the release page instead. Releases now also
publish a `.sha256` beside every artifact, which is what the Linux AppImage path
verifies against; an AppImage has no signature to fall back on.

### An About window that doesn't lie about what it is

**Tables → About Tables** — icon, version, links, and a line saying whether this
build is *the* release of its version or a checkout that happens to carry the
number. A new `build.rs` stamps the date and the kind; only the release workflow
sets `TABLES_RELEASE=1`, so every other build says "Development build".

### Settings is a settings screen

The single scrolling sheet is now guise's `SettingsView`: five pages —
Appearance, Data grid, SQL editor, Assistant, Updates — with a description under
every setting and a reset control on the ones that differ from their default.

**⌘, works everywhere now.** Settings were owned by the workspace, so on the home
screen the menu item was greyed out and the shortcut did nothing. They belong to
the app, so the root owns them.

Search is deliberately off. `SettingsView` offers a search field, but it is a
`TextInput` and guise wraps those in a `Field` whose root never sets `w_full`,
so in the view's own sidebar it collapses to about three characters wide.
Nothing in Tables can size it — it belongs to the component.

### Three grid settings that did nothing now do something

`grid_row_height`, `grid_show_row_numbers` and `grid_alternate_rows` were in the
settings file, in the UI, and read by nobody: the grid hard-coded all three.

### The data grid only builds the rows you can see

The body renders through guise's `VirtualList`, so a large page costs what a
small one does instead of building every row every frame. That needs a uniform
row height — which is what made `grid_row_height` real — and a definite viewport
height, measured off the laid-out body.

Everything the grid does that a generic table cannot has been kept: server-side
sort, inline editing into staged changes, per-row tinting for a staged update or
delete, column resize, and the per-cell menu. guise's `TableView` was the
obvious candidate and is the wrong shape for exactly those reasons — it sorts
client-side, has no per-row background, and its cells never see a row index.

### The assistant is built out of guise's AI components

Replies render as **markdown** rather than as the raw text the model sent, with
a proper streaming caret, a real composer (⇧⏎ for a newline, send becomes stop
while a reply is in flight), and a **token and cost meter** fed by the API's own
counts — the stream now reads `usage` off `message_start` and `message_delta`.

Run and Insert still sit on every fenced `sql` block, which is why the transcript
is composed from `AIMessage` rather than handed to `AIChatView`.

### Claude models

The picker offered Opus 4.8, Sonnet 5 and Haiku 4.5, and defaulted to Opus 4.8.
It now leads with **Claude Opus 5**, which is also the new default, and each
entry carries its context window and per-million pricing so the cost meter has
real numbers. Opus 4.8 stays on the list below its successor, so a settings file
pinned to it keeps the model it asked for.

### Fixed

- **The menu bar was mostly dead.** gpui dispatches an action along the focus
  path and nothing in Tables ever took focus, so every action registered on an
  element was unreachable — About and Settings greyed out, and their shortcuts
  swallowed. The window root now holds focus whenever it would otherwise go
  nowhere.

### Build

`guise-ui` comes from **crates.io at 1.2.1** rather than an unpinned git ref, so
a release can be rebuilt from its lockfile. `default-features = false` keeps the
`webview` feature and its `wry` dependency out; Tables has no embedded browser.

## 0.1.0 — 2026-07-17

First release. A native Rust port of the earlier TypeScript app: an editable
data grid, a SQL editor, and schema tools for PostgreSQL, MySQL and SQLite,
built on gpui and guise.
