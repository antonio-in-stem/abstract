# README.md:27-32 tells the user to run, from the repo root:
#   powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -Prebuilt abstract-windows-x64.exe
# install.ps1:23-26 does: if (-not (Test-Path $Prebuilt)) { Write-Error "Prebuilt binary not found: $Prebuilt" }
Write-Host ("cwd = " + (Get-Location).Path)
Write-Host ("Test-Path 'abstract-windows-x64.exe' -> " + (Test-Path 'abstract-windows-x64.exe'))
Write-Host ("Test-Path 'site\downloads\abstract-windows-x64.exe' -> " + (Test-Path 'site\downloads\abstract-windows-x64.exe'))
