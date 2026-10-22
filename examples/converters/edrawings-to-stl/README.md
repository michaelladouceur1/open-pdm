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
- .NET 8 SDK
- [eDrawings Viewer](https://www.edrawingsviewer.com/download-edrawings) installed
  (free; no license needed). A newer file needs an eDrawings build at least as new
  as the CAD version that wrote it.

## Build

Point the `HintPath` in `edrawings-to-stl.csproj` at the interop assembly that
ships with your eDrawings install (typically
`C:\Program Files\SOLIDWORKS Corp\eDrawings\eDrawings.Interop.EModelViewControl.dll`),
then:

```
dotnet build -c Release
```

The executable lands at
`bin/Release/net8.0-windows/edrawings-to-stl.exe`.

If you prefer to generate the interop from the registered type library, delete the
`Reference` in the csproj and add a `COMReference` to the eDrawings control
(CLSID `22945A69-1191-4DCF-9E6F-409BDE94D101`).

## Wire it into OpenPDM

Create `<project>/.openpdm/converters.json`:

```json
{
  "converters": {
    "sldprt": { "program": "C:\\tools\\edrawings-to-stl.exe", "args": ["{in}", "{out}"] },
    "sldasm": { "program": "C:\\tools\\edrawings-to-stl.exe", "args": ["{in}", "{out}"] },
    "prt":    { "program": "C:\\tools\\edrawings-to-stl.exe", "args": ["{in}", "{out}"] },
    "asm":    { "program": "C:\\tools\\edrawings-to-stl.exe", "args": ["{in}", "{out}"] }
  }
}
```

`{in}` and `{out}` are substituted with scratch paths for each conversion; the
program is run directly (no shell). Converted output is cached by content, so each
unique part converts once.

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

The event-driven flow follows the well-known
[CodeStack/Xarial eDrawings example](https://www.codestack.net/edrawings-api/output/export/).

## Caveats

- Windows-only, needs an interactive desktop session (it hosts a COM control).
- STL flattens assemblies — no part names, colours or units.
- Running one process per part carries eDrawings' startup cost; OpenPDM's
  content-addressed cache means each unique part only pays it once.
