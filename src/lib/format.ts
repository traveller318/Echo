/**
 * SOURCE OF TRUTH KEYWORDS: format, formatCount, formatDuration, formatClock, formatWpm, formatBytes, formatTakeTime, formatMetricValue, formatSettingUnit, formatLanguage
 * WHAT:  Every number the UI shows, turned into text: counts, human durations (`2 h 5 min`), the pill clock
 *        (`m:ss`), words per minute, milliseconds, days, byte sizes, word counts, when a take happened
 *        (`formatTakeTime`), and the unit-driven entry points `formatMetricValue(unit, value)` (dashboard),
 *        `formatSettingInt(unit, value)` and `formatSettingUnit(unit)` (Settings); plus language names
 *        (`formatLanguage`) and NUMERIC_CLASS, the class that makes numerals tabular.
 * WHY:   One place decides how a number reads, so the dashboard, history, models and settings agree. The unit
 *        tables are keyed by the generated MetricUnit / SettingUnit unions, so a new unit in Rust fails tsc until
 *        it has a format here, and no component switches on a metric or setting key (root CLAUDE.md §3). Grouping
 *        follows the Windows locale through Intl (the locale is a parameter so tests are deterministic); the unit
 *        words are Echo's English copy (04 §1). A missing value reads as an em dash, never as 0, so "no data" and
 *        "zero" stay distinct. Numbers are always tabular (04 §3.6): components put NUMERIC_CLASS on the element.
 * WHERE: Dashboard stat cards and chart axes (step 20), History rows (step 16), Models sizes and progress (step 21),
 *        SettingField int controls (step 18), the pill timer (step 15).
 */
import type { MetricUnit, SettingUnit } from "@/bindings";

/** Tailwind class that switches numerals to tabular figures (`font-variant-numeric: tabular-nums`). */
export const NUMERIC_CLASS = "tabular-nums";

/** What a value that does not exist yet reads as. */
export const MISSING_VALUE = "—";

const MS_PER_SECOND = 1000;
const SECONDS_PER_MINUTE = 60;
const MINUTES_PER_HOUR = 60;
const SECONDS_PER_HOUR = SECONDS_PER_MINUTE * MINUTES_PER_HOUR;
const BYTES_PER_UNIT = 1024;
const BYTE_UNITS = ["bytes", "KB", "MB", "GB", "TB"] as const;
/** Sizes below this many units get one decimal (`1.4 GB`), larger ones none (`670 MB`). */
const DECIMAL_BELOW = 10;
const MINUS = "−";

/** `locale` defaults to the system locale (WebView2 follows Windows). */
function numberFormat(locale: string | undefined, fractionDigits = 0): Intl.NumberFormat {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: fractionDigits });
}

function plural(value: number, one: string, other: string): string {
  return Math.abs(value) === 1 ? one : other;
}

/** A whole count with locale grouping: `12,345`. */
export function formatCount(value: number, locale?: string): string {
  return numberFormat(locale).format(Math.round(value));
}

/**
 * SOURCE OF TRUTH KEYWORDS: formatDuration, human duration, hours minutes seconds, negative duration
 * WHAT:  Milliseconds as the two largest non-zero units of h / min / s, rounded to whole seconds: `2 h 5 min`,
 *        `3 min 12 s`, `45 s`, `0 s`; a negative value gets a minus sign.
 * WHY:   Time saved (02 §7.4) is negative when someone speaks slower than they type, and that must read as such.
 *        Two units are precise enough for a glance and never grow into a sentence.
 * WHERE: formatMetricValue (`duration`), History durations.
 */
export function formatDuration(ms: number, locale?: string): string {
  const totalSeconds = Math.round(Math.abs(ms) / MS_PER_SECOND);
  const sign = ms < 0 && totalSeconds > 0 ? MINUS : "";
  const hours = Math.floor(totalSeconds / SECONDS_PER_HOUR);
  const minutes = Math.floor((totalSeconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE);
  const seconds = totalSeconds % SECONDS_PER_MINUTE;
  const count = numberFormat(locale);
  const parts = [
    { value: hours, unit: "h" },
    { value: minutes, unit: "min" },
    { value: seconds, unit: "s" },
  ];
  const first = parts.findIndex((part) => part.value > 0);
  if (first === -1) {
    return "0 s";
  }
  const shown = parts
    .slice(first, first + 2)
    .filter((part) => part.value > 0)
    .map((part) => `${count.format(part.value)} ${part.unit}`);
  return `${sign}${shown.join(" ")}`;
}

/** Elapsed time as a clock: `0:07`, `12:45`, `1:02:03` (the pill timer, 04 §4). */
export function formatClock(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / MS_PER_SECOND));
  const hours = Math.floor(totalSeconds / SECONDS_PER_HOUR);
  const minutes = Math.floor((totalSeconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE);
  const seconds = String(totalSeconds % SECONDS_PER_MINUTE).padStart(2, "0");
  if (hours > 0) {
    return `${String(hours)}:${String(minutes).padStart(2, "0")}:${seconds}`;
  }
  return `${String(minutes)}:${seconds}`;
}

/** Words per minute: `142 wpm`. */
export function formatWpm(value: number, locale?: string): string {
  return `${formatCount(value, locale)} wpm`;
}

/** A latency: `184 ms`. */
export function formatMilliseconds(value: number, locale?: string): string {
  return `${formatCount(value, locale)} ms`;
}

/** Milliseconds as seconds with at most one decimal: `3 s`, `3.5 s`. */
export function formatSeconds(ms: number, locale?: string): string {
  return `${numberFormat(locale, 1).format(ms / MS_PER_SECOND)} s`;
}

/** A number of minutes: `15 min`. */
export function formatMinutes(value: number, locale?: string): string {
  return `${formatCount(value, locale)} min`;
}

/** A number of days: `1 day`, `7 days`. */
export function formatDays(value: number, locale?: string): string {
  return `${formatCount(value, locale)} ${plural(value, "day", "days")}`;
}

/** A byte size in binary units with Windows' labels: `512 bytes`, `1.4 GB`, `670 MB`. */
export function formatBytes(bytes: number, locale?: string): string {
  let value = Math.max(0, bytes);
  let unit = 0;
  while (value >= BYTES_PER_UNIT && unit < BYTE_UNITS.length - 1) {
    value /= BYTES_PER_UNIT;
    unit += 1;
  }
  if (unit === 0) {
    return `${formatCount(value, locale)} ${plural(value, "byte", "bytes")}`;
  }
  const label = BYTE_UNITS[unit] ?? BYTE_UNITS[0];
  const digits = value < DECIMAL_BELOW ? 1 : 0;
  return `${numberFormat(locale, digits).format(value)} ${label}`;
}

type Formatter = (value: number, locale?: string) => string;

const METRIC_FORMAT: Readonly<Record<MetricUnit, Formatter>> = {
  duration: formatDuration,
  count: formatCount,
  wpm: formatWpm,
  ms: formatMilliseconds,
  days: formatDays,
};

/** A dashboard metric value in its registry unit; `null` (no completed take yet) reads as an em dash. */
export function formatMetricValue(unit: MetricUnit, value: number | null, locale?: string): string {
  return value === null ? MISSING_VALUE : METRIC_FORMAT[unit](value, locale);
}

const SETTING_FORMAT: Readonly<Record<SettingUnit, Formatter>> = {
  milliseconds: formatSeconds,
  minutes: formatMinutes,
  days: formatDays,
  words_per_minute: formatWpm,
};

/** An `Int` setting value in its registry unit; a unit-less int is a plain count. */
export function formatSettingInt(unit: SettingUnit | null, value: number, locale?: string): string {
  return unit === null ? formatCount(value, locale) : SETTING_FORMAT[unit](value, locale);
}

const SETTING_UNIT_SYMBOL: Readonly<Record<SettingUnit, string>> = {
  milliseconds: "ms",
  minutes: "min",
  days: "days",
  words_per_minute: "wpm",
};

/** The short unit written after an `Int` setting's number field (`ms`, `min`); empty for a unit-less int. */
export function formatSettingUnit(unit: SettingUnit | null): string {
  return unit === null ? "" : SETTING_UNIT_SYMBOL[unit];
}

/**
 * SOURCE OF TRUTH KEYWORDS: formatLanguage, language display name, Intl.DisplayNames, language option label
 * WHAT:  A language code as its name in the UI's language (`de` → `German` in English); the code itself when Intl
 *        has no name for it or the code is malformed.
 * WHY:   The registry labels runtime language options with their codes (registry/settings/options.rs) because
 *        naming languages is a UI and locale concern; Intl follows the Windows display language through WebView2.
 * WHERE: SettingField option labels for the `asr_languages` source; later the Models page language list.
 */
export function formatLanguage(code: string, locale?: string): string {
  try {
    return new Intl.DisplayNames(locale, { type: "language" }).of(code) ?? code;
  } catch {
    return code;
  }
}

/** A word count: `1 word`, `1,204 words`. */
export function formatWords(value: number, locale?: string): string {
  return `${formatCount(value, locale)} ${plural(value, "word", "words")}`;
}

/**
 * SOURCE OF TRUTH KEYWORDS: formatTakeTime, take timestamp, today time only, date and time, locale date
 * WHAT:  When a take happened, as short as it can be read: the time alone today (`2:05 PM`), the day and time this
 *        year (`Sep 12, 2:05 PM` in en-US), and the full date and time before that.
 * WHY:   History is scanned by recency; repeating today's date on every row is noise. The order and 12/24-hour
 *        clock follow the Windows locale through Intl (the locale is a parameter so tests are deterministic);
 *        `now` is a parameter so the rule is testable and a list renders with one clock.
 * WHERE: TranscriptRow (History rows, later the Dashboard's recent takes), the History detail sheet.
 */
export function formatTakeTime(createdAt: number, now: number = Date.now(), locale?: string): string {
  const at = new Date(createdAt);
  const today = new Date(now);
  const time: Intl.DateTimeFormatOptions = { hour: "numeric", minute: "2-digit" };
  const sameYear = at.getFullYear() === today.getFullYear();
  if (sameYear && at.getMonth() === today.getMonth() && at.getDate() === today.getDate()) {
    return new Intl.DateTimeFormat(locale, time).format(at);
  }
  return new Intl.DateTimeFormat(locale, {
    ...time,
    day: "numeric",
    month: "short",
    ...(sameYear ? {} : { year: "numeric" }),
  }).format(at);
}
