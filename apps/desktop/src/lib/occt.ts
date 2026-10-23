/**
 * Client-side exact-geometry tessellation.
 *
 * FreeCAD `.FCStd` documents store exact B-rep geometry rather than triangles,
 * and STEP files are exact B-rep exchange files, so the Rust core hands the
 * viewer the raw payloads (`.brp` shapes, or the `.step` file itself) instead of
 * a mesh. This module feeds those payloads to `occt-import-js` (OpenCASCADE
 * compiled to WebAssembly) and flattens the result into the flat
 * `{ positions, indices }` layout the three.js {@link Viewer} expects.
 *
 * The ~7.6 MiB wasm is loaded lazily, on the first such preview, so mesh formats
 * (STL/OBJ) never pay for it.
 */
import type { ShapePayload } from "@/lib/api";

/** Flat vertex positions `[x,y,z,...]` and triangle indices `[i,j,k,...]`. */
export interface TessellatedMesh {
  positions: number[];
  indices: number[];
}

interface OcctMesh {
  attributes?: {
    position?: { array: ArrayLike<number> };
  };
  // In occt-import-js the triangle index array sits on the mesh, not inside
  // `attributes`.
  index?: { array: ArrayLike<number> } | null;
}

interface OcctResult {
  success: boolean;
  meshes?: OcctMesh[];
}

interface OcctModule {
  ReadBrepFile(content: Uint8Array, params: unknown): OcctResult;
  ReadStepFile(content: Uint8Array, params: unknown): OcctResult;
}

type OcctFactory = (config?: {
  locateFile?: (path: string, scriptDirectory: string) => string;
}) => Promise<OcctModule>;

let occtPromise: Promise<OcctModule> | null = null;

function loadOcct(): Promise<OcctModule> {
  if (!occtPromise) {
    occtPromise = (async () => {
      // Both imports are dynamic so the wasm bundle stays out of the initial
      // payload and is only fetched when an exact-geometry file is opened.
      const [module, wasm] = await Promise.all([
        import("occt-import-js/dist/occt-import-js.js"),
        import("occt-import-js/dist/occt-import-js.wasm?url"),
      ]);
      const factory = module.default as OcctFactory;
      // Emscripten resolves the wasm relative to the loading script, which is
      // unreliable under a bundler; point it at the URL Vite emitted instead.
      return factory({
        locateFile: (path) => (path.endsWith(".wasm") ? wasm.default : path),
      });
    })();
  }
  return occtPromise;
}

function base64ToBytes(encoded: string): Uint8Array {
  const binary = atob(encoded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

/** The occt-import-js reader for a payload format, if the viewer supports it. */
function readerFor(
  occt: OcctModule,
  format: string,
): ((content: Uint8Array, params: unknown) => OcctResult) | null {
  switch (format) {
    case "brep":
      return (content, params) => occt.ReadBrepFile(content, params);
    case "step":
      return (content, params) => occt.ReadStepFile(content, params);
    default:
      return null;
  }
}

/**
 * Tessellate one or more exact-geometry payloads (B-rep or STEP) into a single
 * mesh.
 *
 * Payloads that fail to parse are skipped; a single bad shape must not sink the
 * whole preview. Returns `null` when nothing produced triangles.
 */
export async function tessellateShapes(payloads: ShapePayload[]): Promise<TessellatedMesh | null> {
  const occt = await loadOcct();
  const positions: number[] = [];
  const indices: number[] = [];
  let vertexOffset = 0;

  for (const payload of payloads) {
    if (!payload.data) continue;
    const read = readerFor(occt, payload.format);
    if (!read) continue;

    let result: OcctResult;
    try {
      result = read(base64ToBytes(payload.data), null);
    } catch {
      continue;
    }
    if (!result.success) continue;

    for (const mesh of result.meshes ?? []) {
      const source = mesh.attributes?.position?.array;
      if (!source) continue;
      for (let i = 0; i < source.length; i += 1) {
        positions.push(source[i]);
      }

      const triangles = mesh.index?.array;
      if (triangles) {
        for (let i = 0; i < triangles.length; i += 1) {
          indices.push(vertexOffset + triangles[i]);
        }
      }
      vertexOffset += source.length / 3;
    }
  }

  return indices.length > 0 ? { positions, indices } : null;
}
