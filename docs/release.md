# Releasing Tables

Tables ships as a signed macOS `.dmg` (plus a Homebrew cask) and Linux
`.tar.gz`, `.deb`, and `.AppImage` packages for x86_64 and aarch64, with beta
Windows `.zip` + `.msi` artifacts. Releases are cut by GitHub Actions
(`.github/workflows/release.yml`); the local scripts under `scripts/` are the
same steps you can run by hand.

## Cutting a release

1. Bump `version` in the workspace `Cargo.toml` (`[workspace.package]`).
2. Merge to `main`.

The workflow notices the new version (no matching `vX.Y.Z` tag yet), tags it,
creates a GitHub Release, then in parallel: builds and notarizes `Tables.dmg`
and updates the `tables` cask in
[`wess/homebrew-packages`](https://github.com/wess/homebrew-packages); builds the
Linux packages (matrix over x86_64 and aarch64 on native runners); and builds
the beta Windows artifacts. Everything uploads to the release. The version check
is idempotent, so re-running is safe.

Because the Linux build includes code that never compiles on the macOS dev host
(the `#[cfg(target_os = "linux")]` blocks), validate it **before** cutting the
release: open a PR, which runs the `Linux Build` workflow
(`.github/workflows/linux.yml`) on both architectures and uploads the artifacts
to the run for inspection. The `Windows Build` workflow compiles the workspace
on `windows-latest` the same way.

## Local build

```sh
scripts/icon.sh      # regenerate assets/icon.{png,icns} + icon512.png (only if the icon changed)
scripts/bundle.sh    # cargo build --release + assemble dist/Tables.app
scripts/dmg.sh       # package dist/Tables.dmg
```

Without `CODESIGN_IDENTITY` set, `bundle.sh` ad-hoc-signs the app: it launches on
the build machine but is not distributable. For a signed local build, set
`CODESIGN_IDENTITY` to a Developer ID Application identity from your keychain.

## Linux build

```sh
scripts/linux.sh            # build + package for the host arch
scripts/linux.sh aarch64    # label the artifacts (still builds natively)
```

`scripts/linux.sh` builds the release binary and produces, in `dist/linux/`, a
`.tar.gz` (FHS tree), a `.deb` (via `cargo-deb`, configured in
`crates/app/Cargo.toml`'s `[package.metadata.deb]`), and an `.AppImage` (via
`linuxdeploy` + `appimagetool`, downloaded on demand). It builds **natively** —
CI uses a separate runner per architecture rather than cross-compiling.

Requirements: a Rust toolchain, `cargo-deb` (installed on demand), `curl`,
`file`, and the gpui system libraries — `clang`, `libasound2-dev`,
`libfontconfig-dev`, `libssl-dev`, `libvulkan1`, `libwayland-dev`,
`libx11-xcb-dev`, `libxkbcommon-x11-dev`. The `.desktop` entry is
`assets/tables.desktop`; the AppImage icon must be a standard size, so packaging
uses `assets/icon512.png` (the 1024px master is rejected by `linuxdeploy`).

## Windows build

See [`packaging/README.md`](../packaging/README.md). Windows support is beta and
the artifacts are unsigned.

## Signing & notarization (CI)

Signing is optional — without secrets the workflow produces an ad-hoc build and
warns. To sign + notarize, set these repository secrets:

| Secret | What it is |
|--------|------------|
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_CERT_P12` | base64 of the exported Developer ID `.p12` |
| `APPLE_CERT_PASSWORD` | password for that `.p12` |
| `KEYCHAIN_PASSWORD` | any password for the throwaway CI keychain |
| `APPLE_ID` | Apple ID email for notarytool |
| `APPLE_TEAM_ID` | Apple Developer Team ID |
| `APPLE_APP_PASSWORD` | app-specific password for that Apple ID |
| `HOMEBREW_TAP_TOKEN` | token with write access to `wess/homebrew-packages` |

The app is signed with a hardened runtime and `assets/tables.entitlements`
(GPUI/Metal needs the JIT / unsigned-executable-memory entitlements), then the
`.app` and `.dmg` are notarized and stapled.

## Icon

`scripts/icon.swift` draws the icon (a data-grid glyph on a dark indigo
squircle) with CoreGraphics — no third-party tooling. `scripts/icon.sh` renders
the 1024px master, compiles the `.icns`, and writes the 512px downscale. The
committed `assets/icon.png` and `assets/icon.icns` are what the macOS bundle
embeds; `assets/icon512.png` is the 512px downscale used for Linux packaging.
Regenerate them only when the design changes.
