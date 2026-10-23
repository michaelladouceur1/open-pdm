#!/usr/bin/env pwsh
# Publishes the eDrawings -> STL converter as a single self-contained win-x64 exe.
#
# Run on Windows with the .NET 8 SDK and eDrawings Viewer installed (the interop
# reference is resolved at build time). Override the eDrawings location if needed:
#   ./publish.ps1 -EDrawingsInteropDir 'D:\eDrawings'
param(
    [string]$EDrawingsInteropDir = $env:EDrawingsInteropDir,
    [string]$Configuration = 'Release'
)

$ErrorActionPreference = 'Stop'

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $here
try {
    $dotnetArgs = @('publish', '-c', $Configuration)
    if ($EDrawingsInteropDir) { $dotnetArgs += "-p:EDrawingsInteropDir=$EDrawingsInteropDir" }

    dotnet @dotnetArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    $exe = Join-Path $here "bin/$Configuration/net8.0-windows/win-x64/publish/edrawings-to-stl.exe"
    $dist = Join-Path $here 'dist'
    New-Item -ItemType Directory -Force -Path $dist | Out-Null
    Copy-Item $exe (Join-Path $dist 'edrawings-to-stl.exe') -Force
    Write-Host "Published -> $(Join-Path $dist 'edrawings-to-stl.exe')"
}
finally {
    Pop-Location
}
