# Build Tables (release) for Windows and produce a portable .zip and an MSI
# installer under dist/windows. Mirrors scripts/linux.sh. Intended for the
# windows-latest runner; run locally with PowerShell 7+ (pwsh).
#
# The binary is the `tablesdev` bin from crates/app, installed as `tables.exe`.
# The version is read from the workspace Cargo.toml. Builds natively for the
# host architecture — pass x86_64 or aarch64 only to label artifacts and pick
# the target triple.
#
# The MSI is built with the WiX v4 toolset (installed on demand as a dotnet
# global tool). An MSI failure is non-fatal: the portable .zip is always
# produced so a release still ships something installable.
#
# Usage: pwsh scripts/windows.ps1 [-Arch x86_64]
param(
    [ValidateSet("x86_64", "aarch64")]
    [string]$Arch = "x86_64"
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$triple = switch ($Arch) {
    "x86_64" { "x86_64-pc-windows-msvc" }
    "aarch64" { "aarch64-pc-windows-msvc" }
}
$wixArch = if ($Arch -eq "aarch64") { "arm64" } else { "x64" }

$version = (Select-String -Path Cargo.toml -Pattern '^version = "([0-9][^"]*)"' |
    Select-Object -First 1).Matches.Groups[1].Value
if (-not $version) { throw "could not read version from Cargo.toml" }
Write-Host "[windows] Tables $version for $triple"

$out = Join-Path $root "dist\windows"
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $out | Out-Null

# --- build ----------------------------------------------------------------
rustup target add $triple 2>&1 | Out-Null
cargo build --release -p app --target $triple
$bin = "target\$triple\release\tablesdev.exe"

# --- staging tree (shared by the zip and the MSI harvest) ------------------
$stem = "tables-$version-windows-$Arch"
$stage = Join-Path $out $stem
New-Item -ItemType Directory -Force -Path $stage | Out-Null
Copy-Item $bin (Join-Path $stage "tables.exe")
Copy-Item LICENSE, README.md $stage -ErrorAction SilentlyContinue

# --- .zip ------------------------------------------------------------------
$zip = Join-Path $out "$stem.zip"
Compress-Archive -Path $stage -DestinationPath $zip -Force
Write-Host "[windows] -> $stem.zip"

# --- .msi (WiX v4) ---------------------------------------------------------
try {
    dotnet tool install --global wix --version 4.* 2>&1 | Out-Null
    $env:PATH = "$env:PATH;$env:USERPROFILE\.dotnet\tools"
    wix build "packaging\windows\tables.wxs" `
        -define "Version=$version" `
        -define "StageDir=$stage" `
        -arch $wixArch `
        -out (Join-Path $out "$stem.msi")
    Write-Host "[windows] -> $stem.msi"
} catch {
    Write-Warning "[windows] MSI build failed (zip still produced): $_"
}

# --- cleanup intermediates, leave only shippable artifacts -----------------
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
Write-Host "[windows] artifacts in dist/windows:"
Get-ChildItem $out | Select-Object -ExpandProperty Name

# The zip is the guaranteed deliverable; a best-effort MSI failure above must
# not leave a non-zero exit code that fails the job (getting here means the
# build + zip succeeded — a cargo failure would have thrown earlier).
exit 0
