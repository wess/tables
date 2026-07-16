$ErrorActionPreference = 'Stop'

# Version and checksum are rewritten per release (see packaging/README.md and
# the release workflow). Chocolatey shims tables.exe from the unzip location.
$version = '0.0.0'
$url64 = "https://github.com/wess/tables/releases/download/v$version/tables-$version-windows-x86_64.zip"
$checksum64 = '0000000000000000000000000000000000000000000000000000000000000000'

$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Definition

Install-ChocolateyZipPackage `
    -PackageName 'tables' `
    -Url64bit $url64 `
    -UnzipLocation $toolsDir `
    -Checksum64 $checksum64 `
    -ChecksumType64 'sha256'
