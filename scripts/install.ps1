# Installs the kusanagi CLI from a GitHub release, verifying the checksum.
#
# The release notes say it plainly: the checksums prove the download matches
# what the release workflow built, and nothing more. If that is not enough
# trust for you, build from source instead (see README.md).
#
# Usage: .\install.ps1 [-Version v0.0.1-Pre-alpha-260913]
#
# The default is the newest release tag, and it moves with each release: GitHub's
# "latest release" endpoint skips prereleases, and every release so far is one,
# so asking it would answer nothing.
param([string]$Version = "v0.0.1-Pre-alpha-260913")

$ErrorActionPreference = "Stop"
$Repo = "2youg1/kusanagi"
$Target = "x86_64-pc-windows-msvc"
$Name = "kusanagi-$Version-$Target.exe"
$Base = "https://github.com/$Repo/releases/download/$Version/$Name"
$Dest = Join-Path $env:LOCALAPPDATA "kusanagi\bin"
New-Item -ItemType Directory -Force -Path $Dest | Out-Null

$Tmp = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $Tmp | Out-Null
try {
    Invoke-WebRequest -Uri $Base -OutFile (Join-Path $Tmp $Name)
    Invoke-WebRequest -Uri "$Base.sha256" -OutFile (Join-Path $Tmp "$Name.sha256")
    $Want = ((Get-Content (Join-Path $Tmp "$Name.sha256") -Raw) -split '\s+')[0]
    $Got = (Get-FileHash (Join-Path $Tmp $Name) -Algorithm SHA256).Hash.ToLower()
    if ($Got -ne $Want.ToLower()) { throw "checksum mismatch: want $Want, got $Got" }
    Copy-Item (Join-Path $Tmp $Name) (Join-Path $Dest "kusanagi.exe") -Force
} finally {
    Remove-Item -Recurse -Force $Tmp
}

# PATH (user scope, this shell too)
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$Dest*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$Dest", "User")
}
$env:Path = "$env:Path;$Dest"
& (Join-Path $Dest "kusanagi.exe") --help | Out-Null
"installed to $Dest\kusanagi.exe — runs: yes"
