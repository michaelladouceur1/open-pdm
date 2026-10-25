/**
 * Centralised persistent storage for the desktop app.
 *
 * Every localStorage key used by the app lives in this module. Callers read and
 * write values through the typed helpers below using the short keys from
 * {@link StoreValues}; the underlying localStorage key names stay private here.
 */

// Values persisted by the app, keyed by a short, code-friendly name.
export type StoreValues = {
  /** UI theme preference. */
  theme: "light" | "dark";
  /** Height of the Files section as a percentage (0-100) of the left column. */
  filesHeightPct: number;
  /** Whether the Files section is collapsed. */
  filesCollapsed: boolean;
  /** Whether the Revisions section is collapsed. */
  revisionsCollapsed: boolean;
  /** Row density for the Files section. */
  filesView: "detailed" | "compact";
  /** Whether the Files section lists every project file, not just changes. */
  filesShowAll: boolean;
  /** Row density for the Revisions section. */
  revisionsView: "detailed" | "compact";
  /** Path of the most recently opened project, reopened on the next launch. */
  lastProjectPath: string;
  /** Recently opened project paths, most recent first. */
  recentProjects: string[];
};

// A key that identifies a persisted value.
export type StoreKey = keyof StoreValues;

// Maps the code-friendly keys above to their localStorage key names.
const STORAGE_KEYS: Record<StoreKey, string> = {
  theme: "openpdm-theme",
  filesHeightPct: "openpdm-files-height-pct",
  filesCollapsed: "openpdm-files-collapsed",
  revisionsCollapsed: "openpdm-revisions-collapsed",
  filesView: "openpdm-files-view",
  filesShowAll: "openpdm-files-show-all",
  revisionsView: "openpdm-revisions-view",
  lastProjectPath: "openpdm-last-project-path",
  recentProjects: "openpdm-recent-projects",
};

/**
 * Read a persisted value. Returns `fallback` when nothing is stored, when the
 * stored value is unreadable, or when its type no longer matches the fallback
 * (for example after a value's shape has changed between releases).
 */
export function getStored<K extends StoreKey>(
  key: K,
  fallback: StoreValues[K],
): StoreValues[K] {
  try {
    const raw = localStorage.getItem(STORAGE_KEYS[key]);
    if (raw === null) return fallback;
    const value: unknown = JSON.parse(raw);
    return typeof value === typeof fallback
      ? (value as StoreValues[K])
      : fallback;
  } catch {
    return fallback;
  }
}

/** Write a persisted value. Storage failures (private mode, quota) are ignored. */
export function setStored<K extends StoreKey>(
  key: K,
  value: StoreValues[K],
): void {
  try {
    localStorage.setItem(STORAGE_KEYS[key], JSON.stringify(value));
  } catch {
    // Persistence is best-effort; the app keeps working without it.
  }
}
