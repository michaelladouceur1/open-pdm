#!/usr/bin/env sh
# Publishes the eDrawings -> STL converter as a single self-contained win-x64 exe.
#
# Run on Windows (Git Bash / WSL with the .NET 8 SDK) with eDrawings Viewer
# installed, or point EDrawingsInteropDir at it:
#   EDrawingsInteropDir='D:\eDrawings' ./publish.sh
set -eu

here=$(cd "$(dirname "$0")" && pwd)
cd "$here"

config=${CONFIGURATION:-Release}
extra=""
if [ -n "${EDrawingsInteropDir:-}" ]; then
    extra="-p:EDrawingsInteropDir=$EDrawingsInteropDir"
fi

# shellcheck disable=SC2086
dotnet publish -c "$config" $extra

exe="bin/$config/net8.0-windows/win-x64/publish/edrawings-to-stl.exe"
mkdir -p dist
cp "$exe" dist/edrawings-to-stl.exe
echo "Published -> dist/edrawings-to-stl.exe"
