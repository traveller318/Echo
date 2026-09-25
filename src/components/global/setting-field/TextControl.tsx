/**
 * SOURCE OF TRUTH KEYWORDS: TextControl, Text setting, free text setting, save on blur
 * WHAT:  The control of a `Text` setting: an Input that saves on Enter or when it loses focus, and only when the
 *        text changed and passes the schema (length, characters).
 * WHY:   Saving per keystroke would write a half-typed value and announce it to every window; the limit is the
 *        spec's `max_len`, shown inline with Rust's message before anything is sent.
 * WHERE: SettingField, for SettingKind `text`.
 */
import { Controller } from "react-hook-form";
import { FieldError, Input } from "@/components/ui";
import { useSettingForm, type SettingControlProps } from "./setting-control";

export function TextControl(props: SettingControlProps<"text">) {
  const { form, commit } = useSettingForm("text", props);
  const commitIfChanged = () => {
    if (form.getValues("value") !== props.value) {
      commit();
    }
  };
  return (
    <Controller
      name="value"
      control={form.control}
      render={({ field, fieldState }) => (
        <div className="flex w-full flex-col gap-1">
          <Input
            {...field}
            id={props.id}
            aria-describedby={props.describedBy}
            aria-invalid={fieldState.invalid || props.invalid}
            autoComplete="off"
            onBlur={() => {
              field.onBlur();
              commitIfChanged();
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                commitIfChanged();
              }
            }}
          />
          <FieldError errors={[fieldState.error]} />
        </div>
      )}
    />
  );
}
