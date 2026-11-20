/** Room between tabs (`gap-1`). */
const GAP = 4;

/**
 * Which tabs fit in `room` in one line, by index: all of them, else as many
 * from the start as fit beside the "More" button, with the selected one
 * always among them (in its place, the ones before it giving way).
 */
export function fitTabs(widths: number[], selected: number, room: number, more: number): number[] {
  const width = (ids: number[]) =>
    ids.reduce((sum, i) => sum + widths[i], 0) + GAP * Math.max(0, ids.length - 1);
  const all = widths.map((_, i) => i);
  if (width(all) <= room) return all;
  const shown: number[] = [];
  const withSelected = (ids: number[]) => (selected >= 0 ? [...ids, selected] : ids);
  for (const i of all) {
    if (i === selected) continue;
    if (width(withSelected([...shown, i])) + GAP + more > room) break;
    shown.push(i);
  }
  return withSelected(shown).sort((a, b) => a - b);
}
