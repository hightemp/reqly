// Export file names with the date and time, so exports do not overwrite
// each other: `my-api.reqly.json` → `my-api-2026-11-27_14-05.reqly.json`.

const pad = (n: number) => String(n).padStart(2, '0');

/** `name` with the local `date` and time before its extensions. */
export function stampFileName(name: string, date: Date): string {
  const stamp =
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `_${pad(date.getHours())}-${pad(date.getMinutes())}`;
  const slash = Math.max(name.lastIndexOf('/'), name.lastIndexOf('\\')) + 1;
  const dir = name.slice(0, slash);
  const base = name.slice(slash);
  // A leading dot belongs to the name, not to an extension.
  const dot = base.indexOf('.', 1);
  return dot < 0
    ? `${dir}${base}-${stamp}`
    : `${dir}${base.slice(0, dot)}-${stamp}${base.slice(dot)}`;
}
