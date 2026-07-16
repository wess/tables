# Windows packaging

Tables' Windows artifacts, produced by `scripts/windows.ps1` (mirrors
`scripts/linux.sh`). **Windows support is beta**: the binary compiles and links
on Windows via CI but has not yet been runtime-tested on a real machine, and the
installer is **unsigned** — expect a SmartScreen "unknown publisher" prompt
until an Authenticode certificate is wired in.

## Artifacts

`scripts/windows.ps1 [-Arch x86_64]` writes to `dist/windows/`:

- **`tables-<version>-windows-<arch>.zip`** — portable build: `tables.exe`.
  Unzip and run `tables.exe`.
- **`tables-<version>-windows-<arch>.msi`** — WiX v4 installer (per-machine,
  Program Files + Start-menu shortcut). Best-effort: a WiX failure is
  non-fatal, so the zip always ships.

The release workflow builds these on `windows-latest` and uploads them to the
GitHub release on every version bump.

## Package managers

Both manifests point at the release `.zip`. Their `version`, download `url`, and
checksum are placeholders (`0.0.0` / zeroed hash) in git and are rewritten per
release.

- **Scoop** (`scoop/tables.json`): the release workflow rewrites the version,
  URL, `extract_dir`, and SHA-256 and commits it back. Install once published:
  `scoop install https://raw.githubusercontent.com/wess/tables/main/packaging/scoop/tables.json`
- **Chocolatey** (`chocolatey/tables.nuspec` + `tools/chocolateyinstall.ps1`):
  the workflow rewrites the version and checksum and runs `choco pack`, then
  uploads the `.nupkg` to the release. Pushing to the community feed
  (`choco push`) needs an API key and passes moderation — it is **not**
  automated; publish manually when ready.

## Signing (deferred)

To sign the MSI, add an Authenticode certificate as a CI secret and a
`signtool sign` step after `wix build`. An EV certificate is what clears
SmartScreen reputation prompts.
