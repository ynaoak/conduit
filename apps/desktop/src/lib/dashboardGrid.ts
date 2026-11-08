import type { SearchResult } from "./types";

/** Plugin whose block the empty-query dashboard lays out as a grid
 *  (the "frequent apps" section) */
export const DASHBOARD_GRID_PLUGIN = "conduit.app-launcher";

/** Columns of the frequent-apps grid. Rendering and keyboard navigation both
 *  read this, so the row step never drifts from what is on screen. */
export const DASHBOARD_GRID_COLUMNS = 2;

/** Half-open index range [start, end) of the results drawn as a grid */
export interface GridRange {
  start: number;
  end: number;
  columns: number;
}

/**
 * Locate the frequent-apps block on the dashboard. Results are score-ordered
 * so each plugin forms one contiguous block; only the first block is used.
 * Returns null outside the dashboard or when the block is absent.
 */
export function findDashboardGrid(
  results: SearchResult[],
  isDashboard: boolean,
): GridRange | null {
  if (!isDashboard) return null;
  const start = results.findIndex((r) => r.plugin_id === DASHBOARD_GRID_PLUGIN);
  if (start < 0) return null;
  let end = start;
  while (end < results.length && results[end].plugin_id === DASHBOARD_GRID_PLUGIN) end++;
  return { start, end, columns: DASHBOARD_GRID_COLUMNS };
}

/** Next selection for a vertical step, honouring the grid's row layout */
export function stepVertical(
  index: number,
  direction: 1 | -1,
  total: number,
  grid: GridRange | null,
): number {
  const last = total - 1;
  if (!grid) return Math.min(Math.max(index + direction, 0), last);
  const { start, end, columns } = grid;
  const inGrid = index >= start && index < end;
  const lastRowStart = start + Math.floor((end - 1 - start) / columns) * columns;

  if (direction === 1) {
    if (!inGrid) return Math.min(index + 1, last);
    const below = index + columns;
    if (below < end) return below;
    // A short last row: drop onto its final item rather than skipping it
    if (index < lastRowStart) return end - 1;
    return end <= last ? end : index;
  }

  if (inGrid) {
    const above = index - columns;
    if (above >= start) return above;
    return start > 0 ? start - 1 : index;
  }
  // Entering the grid from below lands on the first column of its last row
  if (index === end) return lastRowStart;
  return Math.max(index - 1, 0);
}

/** Next selection for a horizontal step, or null when the grid doesn't own it */
export function stepHorizontal(
  index: number,
  direction: 1 | -1,
  grid: GridRange | null,
): number | null {
  if (!grid || index < grid.start || index >= grid.end) return null;
  return Math.min(Math.max(index + direction, grid.start), grid.end - 1);
}
