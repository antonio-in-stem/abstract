# Installs the Abstract CLI on Windows.
#
# From a repository checkout (builds with cargo):
#   powershell -ExecutionPolicy Bypass -File scripts\install.ps1
#
# From a downloaded abstract.exe (no Rust toolchain needed):
#   powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -Prebuilt path\to\abstract-windows-x64.exe
#
# The binary is placed in %LOCALAPPDATA%\Programs\Abstract and that folder is
# added to the user PATH (new terminals pick it up automatically).

param(
    [string]$Prebuilt = ""
)

$ErrorActionPreference = "Stop"

$installDir = Join-Path $env:LOCALAPPDATA "Programs\Abstract"
$target = Join-Path $installDir "abstract.exe"

New-Item -ItemType Directory -Force -Path $installDir | Out-Null

if ($Prebuilt -ne "") {
    if (-not (Test-Path $Prebuilt)) {
        Write-Error "Prebuilt binary not found: $Prebuilt"
    }
    Copy-Item -Force $Prebuilt $target
    Write-Host "Installed prebuilt binary to $target"
} else {
    $projectDir = Split-Path -Parent $PSScriptRoot
    if (-not (Test-Path (Join-Path $projectDir "Cargo.toml"))) {
        Write-Error "Run this script from the repository (scripts\install.ps1), or pass -Prebuilt <exe>."
    }
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if ($null -eq $cargo) {
        Write-Error "cargo was not found. Install Rust from https://rustup.rs, or use -Prebuilt <exe>."
    }
    Write-Host "Building abstract (release)..."
    Push-Location $projectDir
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    } finally {
        Pop-Location
    }
    Copy-Item -Force (Join-Path $projectDir "target\release\abstract.exe") $target
    Write-Host "Installed to $target"
}

# Add the install directory to the user PATH when missing.
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($null -eq $userPath) { $userPath = "" }
$parts = $userPath -split ";" | Where-Object { $_ -ne "" }
if ($parts -notcontains $installDir) {
    [Environment]::SetEnvironmentVariable("Path", ($userPath.TrimEnd(";") + ";" + $installDir), "User")
    Write-Host "Added $installDir to your user PATH."
    Write-Host "Open a NEW terminal, then run: abstract --version"
} else {
    Write-Host "PATH already contains $installDir."
    Write-Host "Run: abstract --version"
}
