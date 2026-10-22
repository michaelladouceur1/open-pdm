/** Format a byte count in binary (1024-based) units, e.g. `1.5 MiB`. */
export function formatBytes(n: number): string {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${n} B` : `${value.toFixed(1)} ${units[unit]}`;
}

/** Format an absolute Unix time (seconds) as a coarse "time ago" label. */
export function formatTime(seconds: number): string {
  const delta = Math.floor(Date.now() / 1000) - seconds;
  if (delta < 60) return "just now";
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  return `${Math.floor(delta / 86400)}d ago`;
}

/** Shorten a full git object id to its conventional 8-character form. */
export function shortOid(oid: string): string {
  return oid.slice(0, 8);
}
