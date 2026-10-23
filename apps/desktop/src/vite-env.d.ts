/// <reference types="vite/client" />

// occt-import-js ships no type declarations. Only its reader entry points
// (BREP/STEP) are used, and the wrapper in src/lib/occt.ts narrows the result
// itself, so the factory is declared loosely here.
declare module "occt-import-js/dist/occt-import-js.js" {
  const factory: (config?: {
    locateFile?: (path: string, scriptDirectory: string) => string;
  }) => Promise<unknown>;
  export default factory;
}
