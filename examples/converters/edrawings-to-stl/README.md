# edrawings-to-stl

A reference **OpenPDM external converter** that turns files OpenPDM cannot parse
itself into STL, using the **free eDrawings Viewer API** — no SOLIDWORKS (or NX,
CATIA, …) license required.

OpenPDM calls it as:

```
edrawings-to-stl <input> <output.stl>
```

It writes STL to `<output>`, exits `0` on success, and exits non-zero with a
message on `stderr` on failure — matching OpenPDM's converter contract (see the
[handler seam](../../../README.md#the-handler-seam)).

## What it can convert

eDrawings Viewer opens these natively, so they all work here:

| Family | Extensions |
| --- | --- |
| SolidWorks | `.sldprt`, `.sldasm`, `.slddrw` |
| eDrawings | `.eprt`, `.easm`, `.edrw` |
| Pro/ENGINEER · Creo | `.prt`, `.asm`, `.xpr`, `.xas` |
| DXF / DWG | `.dxf`, `.dwg` |
| 3DXML | `.3dxml` |
| STL / CALS | `.stl`, `.cal`, `.ct1` |

It does **not** open CATIA, Siemens NX, Autodesk Inventor or Solid Edge natively —
those need the matching eDrawings Publisher plug-in (run inside that CAD app) or
a commercial multi-CAD SDK. Note that `.prt`/`.asm` are shared by Creo, NX and
Solid Edge, so eDrawings will interpret them as Pro/ENGINEER.

## Requirements

- Windows (the eDrawings control is Windows-only and 64-bit)
- .NET 8 SDK — to *build*; not needed to *run* the published exe
- [eDrawings Viewer](https://www.edrawingsviewer.com/download-edrawings) installed
  (free; no license needed). A newer file needs an eDrawings build at least as new
  as the CAD version that wrote it.

## Building vs. distributing

The `eDrawings.Interop.EModelViewControl.dll` interop assembly is a **build-time
dependency only**. The compiler needs it to know the shape of `EModelViewControl`,
but at runtime the control is resolved from the registry by its CLSID — so the
interop DLL is embedded into this assembly (`EmbedInteropTypes`) and never shipped
next to the exe.

That means end users of a published exe configure **nothing**: they just need
eDrawings Viewer installed. Only the *build machine* has to locate the interop
assembly, which is why its path is a configurable MSBuild property rather than a
hardcoded `HintPath`.

## Build

The build must run on Windows with eDrawings Viewer installed. The interop
assembly is located automatically, so a stock install needs no configuration: it
is resolved, in order, from the control's COM registration, the legacy
`%ProgramFiles%\SOLIDWORKS Corp\eDrawings` layout, and any versioned
`%ProgramFiles%\Common Files\eDrawings<year>` layout (eDrawings 2024 and newer —
for example `eDrawings2026`).

If eDrawings lives somewhere unusual, override the search with a property **or**
an environment variable of the same name:

```bat
dotnet build -c Release -p:EDrawingsInteropDir="D:\eDrawings"
```

```bat
set EDrawingsInteropDir=D:\eDrawings
dotnet build -c Release
```

If the interop assembly cannot be found the build fails with a clear message
telling you which path it looked in.

## Publish a distributable exe

The csproj defaults to a **self-contained, single-file `win-x64`** publish, so
end users need no .NET runtime. Use a wrapper script (which also copies the result
to `dist/`):

```bat
publish.cmd
```

```powershell
./publish.ps1
```

```sh
./publish.sh            # Git Bash / WSL
```

or invoke MSBuild directly:

```sh
dotnet publish -c Release
```

`publish.cmd` is the simplest option on Windows: it runs `dotnet publish` directly
and is unaffected by the PowerShell execution policy. `publish.ps1` is equivalent
but needs script execution to be allowed — see [Windows notes](#windows-notes).

The output is `bin/Release/net8.0-windows/win-x64/publish/edrawings-to-stl.exe`
(and a copy at `dist/edrawings-to-stl.exe`), roughly 60–70 MB. To override the
eDrawings location, pass it through the script or MSBuild:

```sh
EDrawingsInteropDir='D:\eDrawings' ./publish.sh
```

```bat
set "EDrawingsInteropDir=D:\eDrawings" && publish.cmd
```

Consider **code-signing** the exe so Windows SmartScreen does not warn users:

```bat
signtool sign /fd SHA256 /a dist\edrawings-to-stl.exe
```

## Windows notes

`publish.cmd` avoids the issues below. They are worth knowing if you use
`publish.ps1` or `dotnet` directly.

- **"running scripts is disabled on this system"** — Windows blocks `.ps1` files
  under the default `Restricted` execution policy. Either use the batch wrapper
  (`publish.cmd`), bypass the policy for a single command:

  ```powershell
  powershell -ExecutionPolicy Bypass -File .\publish.ps1
  ```

  or allow locally-authored scripts for your account (no admin required):

  ```powershell
  Set-ExecutionPolicy -Scope CurrentUser -ExecutionPolicy RemoteSigned
  ```

- **`dotnet` is not recognized** — the .NET SDK is installed but its directory is
  not on `PATH`. Add `C:\Program Files\dotnet` (or wherever you installed it), or
  use `publish.cmd`, which falls back to that location automatically.

- **Target framework** — the project targets `net8.0-windows`. The .NET 8 SDK
  builds it directly. A newer SDK (for example .NET 10) also works as long as it
  can restore the .NET 8 reference and runtime packs from NuGet, which needs
  network access the first time; otherwise install the .NET 8 SDK.

## Wire it into OpenPDM

Install eDrawings Viewer and put the published exe somewhere on `PATH` (or use an
absolute path). Then create `<project>/.openpdm/converters.json`:

```json
{
  "converters": {
    "sldprt": { "program": "edrawings-to-stl", "args": ["{in}", "{out}"] },
    "sldasm": { "program": "edrawings-to-stl", "args": ["{in}", "{out}"] },
    "prt":    { "program": "edrawings-to-stl", "args": ["{in}", "{out}"] },
    "asm":    { "program": "edrawings-to-stl", "args": ["{in}", "{out}"] }
  }
}
```

`program` may be a bare executable name resolved from `PATH` or an absolute path.
`{in}` and `{out}` are substituted with scratch paths for each conversion; the
program is run directly (no shell). Converted output is cached by content, so each
unique part converts once.

If the program cannot be launched, OpenPDM surfaces a clear error pointing at
`PATH` and `converters.json`.

You can also try it from the CLI without the app:

```sh
openpdm mesh /path/to/repo path/to/part.sldprt
```

## How it works

The eDrawings `EModelViewControl` ActiveX control is hosted in a hidden WinForms
window (`EDrawingsHost.cs`), because its `OpenDoc` and `Save` calls are
asynchronous and report completion through events. `Program.cs` chains
`OpenDoc` → `OnFinishedLoadingDocument` → `Save` → `OnFinishedSavingDocument`,
then exits. `Save` infers the output format from the `.stl` extension.

To run unattended it enables `eMVSuppressSavePrompt` and `eMVEnableSilentMode`
before loading: without them eDrawings pops a modal **STL Export Options** dialog
on `Save`, and a headless process hangs until the caller's timeout kills it. On
completion it exits with `Environment.Exit` rather than `Application.ExitThread`,
because unwinding the control can block even after the output file is written.

The event-driven flow follows the well-known
[CodeStack/Xarial eDrawings example](https://www.codestack.net/edrawings-api/output/export/).

## Caveats

- Windows-only, needs an interactive desktop session (it hosts a COM control).
- STL flattens assemblies — no part names, colours or units.
- Running one process per part carries eDrawings' startup cost; OpenPDM's
  content-addressed cache means each unique part only pays it once.
- OpenPDM does **not** bundle this binary. It is a reference you build and install
  yourself; a future OpenPDM plugin manager may automate that.
