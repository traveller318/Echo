/**
 * SOURCE OF TRUTH KEYWORDS: settingValueSchema, settingFormSchema, SettingKindName, SettingValueOf, wrapSettingValue, unwrapSettingValue, SettingChoices, registry spec to Zod, setting validation
 * WHAT:  Builds the Zod schema of a registry setting from its SettingKind (plus the options offered now for an
 *        Enum): `settingValueSchema(name, kind, choices)` for the bare value and `settingFormSchema(…)` for the
 *        `{ value }` shape a React Hook Form holds. `wrapSettingValue` / `unwrapSettingValue` convert between the
 *        tagged SettingValue Rust sends and the plain value a control edits.
 * WHY:   Every input is validated against a declared schema (root CLAUDE.md §5), and this one is built from the same
 *        registry spec Rust validates writes with (`SettingSpec::validate` in types/settings.rs, runtime membership
 *        and hotkey conflicts in registry/settings), with the same rules and the same messages, so the form stops a
 *        bad value before IPC and never disagrees with Rust. Limits are never restated: bounds come from the spec,
 *        the id length from SETTING_TOKEN_MAX_CHARS (generated). Lengths count Unicode code points like Rust's
 *        `chars().count()`, and "control character" is Unicode Cc like `char::is_control`. The builders are one
 *        mapped table over the generated SettingKind union, so a new kind in Rust fails tsc until it has a schema.
 *        Enum membership uses the options `settings_availability` offers now (fixed lists already filtered by caps).
 * WHERE: components/global/setting-field (one form per setting row, the dictionary editor); onboarding (step 24).
 */
import { z } from "zod";
import { SETTING_TOKEN_MAX_CHARS, type EnumOption, type SettingKind, type SettingValue } from "@/bindings";

export type SettingKindName = SettingKind["kind"];

/** The SettingKind variant named `K`. */
export type SettingKindOf<K extends SettingKindName> = Extract<SettingKind, { kind: K }>;

/** The plain value a setting of kind `K` holds. */
export type SettingValueOf<K extends SettingKindName> = Extract<SettingValue, { kind: K }>["value"];

/** The `{ value }` shape a setting form holds. */
export interface SettingFormValues<K extends SettingKindName> {
  value: SettingValueOf<K>;
}

/** What a schema needs besides the kind: the options an Enum offers now (ignored by the other kinds). */
export interface SettingChoices {
  readonly options: readonly EnumOption[];
}

/** No choices: every kind but Enum. */
export const NO_CHOICES: SettingChoices = { options: [] };

type Schema<K extends SettingKindName> = z.ZodType<SettingValueOf<K>, SettingValueOf<K>>;

// Unicode general category Cc, the set Rust's `char::is_control` tests.
const CONTROL_CHARACTER = /\p{Cc}/u;
const TEXT_CONTROL_CHARACTER = /[^\P{Cc}\n\r\t]/u;

/** Length in Unicode code points, as Rust's `chars().count()`. */
function codePoints(text: string): number {
  return Array.from(text).length;
}

/** Rust `check_token`: non-blank, at most SETTING_TOKEN_MAX_CHARS, no control characters. */
function tokenSchema(emptyMessage: string): z.ZodType<string, string> {
  return z
    .string()
    .refine((text) => text.trim() !== "", emptyMessage)
    .refine(
      (text) => codePoints(text) <= SETTING_TOKEN_MAX_CHARS && !CONTROL_CHARACTER.test(text),
      "This value is not valid.",
    );
}

/**
 * SOURCE OF TRUTH KEYWORDS: SCHEMA_BUILDERS, per-kind Zod builder, Rust validation parity, pairs rules
 * WHAT:  One Zod builder per SettingKind, each mirroring the Rust rule for that kind with the Rust message.
 * WHY:   The mapped type makes the table exhaustive and hands each builder its own narrowed kind. Pair errors carry
 *        the pair's path (`[2, "from"]`) so the dictionary editor marks the row, while the message is Rust's.
 * WHERE: settingValueSchema.
 */
const SCHEMA_BUILDERS: { readonly [K in SettingKindName]: (kind: SettingKindOf<K>, choices: SettingChoices) => Schema<K> } =
  {
    bool: () => z.boolean(),
    int: ({ min, max }) => {
      const message = `Choose a number from ${String(min)} to ${String(max)}.`;
      return z.number(message).int(message).min(min, message).max(max, message);
    },
    enum: (_, { options }) =>
      tokenSchema("Choose an option.").refine(
        (choice) => options.some((option) => option.value === choice),
        "Choose one of the listed options.",
      ),
    hotkey: () => tokenSchema("Press a key combination."),
    device: () => tokenSchema("Choose a microphone.").nullable(),
    text: ({ max_len }) =>
      z
        .string()
        .refine((text) => codePoints(text) <= max_len, `Use at most ${String(max_len)} characters.`)
        .refine((text) => !TEXT_CONTROL_CHARACTER.test(text), "Remove unsupported characters."),
    pairs: ({ max_pairs, max_len }) =>
      z.array(z.object({ from: z.string(), to: z.string() })).superRefine((pairs, context) => {
        if (pairs.length > max_pairs) {
          context.addIssue({ code: "custom", message: `Use at most ${String(max_pairs)} entries.` });
          return;
        }
        const seen = new Set<string>();
        pairs.forEach((pair, index) => {
          const from = pair.from.trim();
          const issue = (message: string, side: "from" | "to" = "from") => {
            context.addIssue({ code: "custom", message, path: [index, side] });
          };
          if (from === "") {
            issue("Every entry needs a word to replace.");
          } else if (codePoints(pair.from) > max_len || codePoints(pair.to) > max_len) {
            issue(`Keep each entry to ${String(max_len)} characters or fewer.`, codePoints(pair.from) > max_len ? "from" : "to");
          } else if (CONTROL_CHARACTER.test(pair.from) || CONTROL_CHARACTER.test(pair.to)) {
            issue("Remove unsupported characters.", CONTROL_CHARACTER.test(pair.from) ? "from" : "to");
          } else if (seen.has(from.toLowerCase())) {
            issue(`"${from}" is listed more than once.`);
          }
          seen.add(from.toLowerCase());
        });
      }),
  };

/** The Zod schema of a setting value of kind `name`. */
export function settingValueSchema<K extends SettingKindName>(
  name: K,
  kind: SettingKindOf<K>,
  choices: SettingChoices = NO_CHOICES,
): Schema<K> {
  const build: (kind: SettingKindOf<K>, choices: SettingChoices) => Schema<K> = SCHEMA_BUILDERS[name];
  return build(kind, choices);
}

/** The schema of a setting form: `{ value }`. */
export function settingFormSchema<K extends SettingKindName>(
  name: K,
  kind: SettingKindOf<K>,
  choices: SettingChoices = NO_CHOICES,
): z.ZodType<SettingFormValues<K>, SettingFormValues<K>> {
  return z.object({ value: settingValueSchema(name, kind, choices) });
}

const WRAP: { readonly [K in SettingKindName]: (value: SettingValueOf<K>) => SettingValue } = {
  bool: (value) => ({ kind: "bool", value }),
  int: (value) => ({ kind: "int", value }),
  enum: (value) => ({ kind: "enum", value }),
  hotkey: (value) => ({ kind: "hotkey", value }),
  device: (value) => ({ kind: "device", value }),
  text: (value) => ({ kind: "text", value }),
  pairs: (value) => ({ kind: "pairs", value }),
};

const UNWRAP: { readonly [K in SettingKindName]: (value: SettingValue) => SettingValueOf<K> | undefined } = {
  bool: (value) => (value.kind === "bool" ? value.value : undefined),
  int: (value) => (value.kind === "int" ? value.value : undefined),
  enum: (value) => (value.kind === "enum" ? value.value : undefined),
  hotkey: (value) => (value.kind === "hotkey" ? value.value : undefined),
  device: (value) => (value.kind === "device" ? value.value : undefined),
  text: (value) => (value.kind === "text" ? value.value : undefined),
  pairs: (value) => (value.kind === "pairs" ? value.value : undefined),
};

/** The tagged SettingValue Rust expects for a plain value of kind `name`. */
export function wrapSettingValue<K extends SettingKindName>(name: K, value: SettingValueOf<K>): SettingValue {
  const wrap: (value: SettingValueOf<K>) => SettingValue = WRAP[name];
  return wrap(value);
}

/** The plain value of `value` when it is of kind `name`; undefined for another kind. */
export function unwrapSettingValue<K extends SettingKindName>(
  name: K,
  value: SettingValue,
): SettingValueOf<K> | undefined {
  const unwrap: (value: SettingValue) => SettingValueOf<K> | undefined = UNWRAP[name];
  return unwrap(value);
}
