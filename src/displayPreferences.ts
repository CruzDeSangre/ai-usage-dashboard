export type UsageDisplayMode = "used" | "remaining";
export type DateFormat = "mm-dd" | "dd-mm";

export interface DisplayPreferences {
  usageDisplayMode: UsageDisplayMode;
  dateFormat: DateFormat;
}

const STORAGE_KEY = "ai-usage-dashboard.display-preferences.v1";

export const DEFAULT_DISPLAY_PREFERENCES: DisplayPreferences = {
  usageDisplayMode: "used",
  dateFormat: "mm-dd",
};

export function clampPercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(100, value));
}

export function displayedPercent(value: number, mode: UsageDisplayMode): number {
  const used = clampPercent(value);
  return mode === "remaining" ? 100 - used : used;
}

export function formatDisplayPercent(value: number, mode: UsageDisplayMode): string {
  const pct = displayedPercent(value, mode);
  return pct > 0 && pct < 1 ? pct.toFixed(1) : String(Math.round(pct));
}

/** Swap the leading MM-DD portion emitted by the backend without touching the time. */
export function formatDateLabel(value: string | undefined, format: DateFormat): string | undefined {
  if (!value || format === "mm-dd") return value;
  return value.replace(/^(\d{2})-(\d{2})(?=\s|$)/, "$2-$1");
}

export function loadDisplayPreferences(): DisplayPreferences {
  if (typeof localStorage === "undefined") return DEFAULT_DISPLAY_PREFERENCES;
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}") as Partial<DisplayPreferences>;
    return {
      usageDisplayMode: parsed.usageDisplayMode === "remaining" ? "remaining" : "used",
      dateFormat: parsed.dateFormat === "dd-mm" ? "dd-mm" : "mm-dd",
    };
  } catch {
    return DEFAULT_DISPLAY_PREFERENCES;
  }
}

export function saveDisplayPreferences(value: DisplayPreferences): void {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
}
