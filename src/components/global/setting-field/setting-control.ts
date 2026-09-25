/**
 * SOURCE OF TRUTH KEYWORDS: SettingControlProps, useSettingForm, setting control contract, per-setting form, React Hook Form Zod resolver, revert on refused write
 * WHAT:  The contract every SettingField control implements (SettingControlProps: its narrowed kind and value, the
 *        options offered, whether the last write failed, and `onCommit`) and `useSettingForm`, the one React Hook
 *        Form + Zod form each control edits through, with `commit()` to validate and hand the value up.
 * WHY:   Every input is validated against a declared schema with React Hook Form, the shadcn way (root CLAUDE.md §5),
 *        and the schema is built from the registry spec (lib/setting-schema.ts), so no control writes its own rules.
 *        The form follows Rust: `values` resets it whenever the effective value changes (SettingsChanged refetch;
 *        compared deeply, so an equal refetch does not disturb typing), and a refused write resets it to the value
 *        still in effect, so a rejected hotkey shows the binding that stayed (05 W7). Controls commit only valid
 *        values; nothing is written while the schema fails.
 * WHERE: Every control in components/global/setting-field (Bool, Int, Enum, Device, Hotkey, Text, Pairs).
 */
import { zodResolver } from "@hookform/resolvers/zod";
import { useEffect, useMemo } from "react";
import { useForm, type UseFormReturn } from "react-hook-form";
import type { EnumOption } from "@/bindings";
import {
  settingFormSchema,
  type SettingFormValues,
  type SettingKindName,
  type SettingKindOf,
  type SettingValueOf,
} from "@/lib/setting-schema";

export interface SettingControlProps<K extends SettingKindName> {
  /** Id of the control element, which the row's label points at. */
  readonly id: string;
  /** Id of the row's help text. */
  readonly describedBy: string;
  /** The setting's label, for controls that open their own surface (the dictionary sheet). */
  readonly label: string;
  readonly kind: SettingKindOf<K>;
  /** The value in effect. */
  readonly value: SettingValueOf<K>;
  /** What an Enum or Device offers now; empty for the other kinds. */
  readonly options: readonly EnumOption[];
  /** The last write of this setting failed (the row shows why). */
  readonly invalid: boolean;
  readonly onCommit: (value: SettingValueOf<K>) => void;
  /** The option list was opened (a Device list reads the microphones again). */
  readonly onOptionsOpen?: () => void;
}

export interface SettingForm<K extends SettingKindName> {
  readonly form: UseFormReturn<SettingFormValues<K>>;
  /** Validates the form and hands a valid value to `onCommit`. */
  readonly commit: () => void;
}

export function useSettingForm<K extends SettingKindName>(
  name: K,
  { kind, value, options, invalid, onCommit }: SettingControlProps<K>,
): SettingForm<K> {
  const schema = useMemo(() => settingFormSchema(name, kind, { options }), [name, kind, options]);
  const values = useMemo<SettingFormValues<K>>(() => ({ value }), [value]);
  const form = useForm<SettingFormValues<K>>({ resolver: zodResolver(schema), values, mode: "onChange" });

  useEffect(() => {
    if (invalid) {
      form.reset(values);
    }
  }, [form, invalid, values]);

  return {
    form,
    commit: () => {
      void form.handleSubmit((submitted) => {
        onCommit(submitted.value);
      })();
    },
  };
}
