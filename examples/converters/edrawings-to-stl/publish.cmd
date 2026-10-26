@echo off
setlocal
REM Publishes the eDrawings -> STL converter as a single self-contained win-x64 exe.
REM
REM Windows alternative to ./publish.ps1 that is not affected by the PowerShell
REM execution policy (running scripts is disabled by default on some machines).
REM
REM Run on Windows with the .NET 8 SDK and eDrawings Viewer installed (the interop
REM reference is resolved at build time). Override the eDrawings location if needed:
REM   set "EDrawingsInteropDir=D:\eDrawings" && publish.cmd
REM
REM Set CONFIGURATION to override the build configuration (default: Release).

cd /d "%~dp0"

if "%CONFIGURATION%"=="" set "CONFIGURATION=Release"

REM Fall back to the default install location when dotnet is not on PATH.
set "DOTNET=dotnet"
where dotnet >nul 2>nul || set "DOTNET=%ProgramFiles%\dotnet\dotnet.exe"

if "%EDrawingsInteropDir%"=="" (
    "%DOTNET%" publish -c "%CONFIGURATION%"
) else (
    "%DOTNET%" publish -c "%CONFIGURATION%" -p:EDrawingsInteropDir="%EDrawingsInteropDir%"
)
if errorlevel 1 exit /b %errorlevel%

if not exist "dist" mkdir "dist"
copy /y "bin\%CONFIGURATION%\net8.0-windows\win-x64\publish\edrawings-to-stl.exe" "dist\edrawings-to-stl.exe" >nul
if errorlevel 1 exit /b %errorlevel%

echo Published -^> dist\edrawings-to-stl.exe
