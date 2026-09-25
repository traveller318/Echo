/**
 * SOURCE OF TRUTH KEYWORDS: BoolControl, Bool setting, Switch setting control, toggle setting
 * WHAT:  The control of a `Bool` setting: a Switch that saves as soon as it flips.
 * WHY:   A toggle has no half-typed state, so it commits at once (04 §5 rows are live, no Save button).
 * WHERE: SettingField, for SettingKind `bool`.
 */
import { Controller } from "react-hook-form";
import { Switch } from "@/components/ui";
import { useSettingForm, type SettingControlProps } from "./setting-control";

export function BoolControl(props: SettingControlProps<"bool">) {
  const { form, commit } = useSettingForm("bool", props);
  return (
    <Controller
      name="value"
      control={form.control}
      render={({ field }) => (
        <Switch
          id={props.id}
          aria-describedby={props.describedBy}
          checked={field.value}
          onBlur={field.onBlur}
          onCheckedChange={(checked) => {
            field.onChange(checked);
            commit();
          }}
        />
      )}
    />
  );
}
