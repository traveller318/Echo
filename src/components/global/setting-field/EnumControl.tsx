/**
 * SOURCE OF TRUTH KEYWORDS: EnumControl, Enum setting, Select setting control, runtime options, no options available
 * WHAT:  The control of an `Enum` setting: a Select of the options offered now, saved as soon as one is picked. With
 *        nothing to offer it is a disabled Select that says so; a value that is no longer offered (an engine that
 *        was removed) shows the placeholder instead of a blank.
 * WHY:   Options come from `settings_availability` (fixed lists filtered by caps, runtime sources resolved by Rust),
 *        so a new engine or language appears here with no UI change, and the Zod schema accepts exactly them.
 * WHERE: SettingField, for SettingKind `enum`.
 */
import { Controller } from "react-hook-form";
import { FieldError, Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui";
import { optionLabel } from "./option-label";
import { useSettingForm, type SettingControlProps } from "./setting-control";

export function EnumControl(props: SettingControlProps<"enum">) {
  const { form, commit } = useSettingForm("enum", props);
  const empty = props.options.length === 0;

  return (
    <Controller
      name="value"
      control={form.control}
      render={({ field, fieldState }) => (
        <div className="flex w-full flex-col gap-1">
          <Select
            // "" shows the placeholder when the stored value is not offered now.
            value={props.options.some((option) => option.value === field.value) ? field.value : ""}
            disabled={empty}
            onValueChange={(next) => {
              field.onChange(next);
              commit();
            }}
            onOpenChange={(open) => {
              if (open) {
                props.onOptionsOpen?.();
              }
            }}
          >
            <SelectTrigger
              id={props.id}
              ref={field.ref}
              aria-describedby={props.describedBy}
              aria-invalid={fieldState.invalid || props.invalid}
              onBlur={field.onBlur}
            >
              <SelectValue placeholder={empty ? "Nothing to choose yet" : "Choose an option"} />
            </SelectTrigger>
            <SelectContent>
              {props.options.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {optionLabel(props.kind.options, option)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <FieldError errors={[fieldState.error]} />
        </div>
      )}
    />
  );
}
