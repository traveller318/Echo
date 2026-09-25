/**
 * SOURCE OF TRUTH KEYWORDS: HotkeyControl, Hotkey setting, rebind shortcut, capture and save hotkey
 * WHAT:  The control of a `Hotkey` setting: a HotkeyInput that saves each combination it captures.
 * WHY:   Rust binds the new combination before storing it and refuses a conflict while the old binding stays
 *        (05 W7); the form then resets to the combination still in effect, so the chips never show a hotkey that
 *        is not bound, and the row shows why in --color-record.
 * WHERE: SettingField, for SettingKind `hotkey`.
 */
import { Controller } from "react-hook-form";
import { FieldError } from "@/components/ui";
import { HotkeyInput } from "../hotkey-input";
import { useSettingForm, type SettingControlProps } from "./setting-control";

export function HotkeyControl(props: SettingControlProps<"hotkey">) {
  const { form, commit } = useSettingForm("hotkey", props);
  return (
    <Controller
      name="value"
      control={form.control}
      render={({ field, fieldState }) => (
        <div className="flex w-full flex-col gap-1">
          <HotkeyInput
            id={props.id}
            ref={field.ref}
            aria-describedby={props.describedBy}
            invalid={fieldState.invalid || props.invalid}
            value={field.value}
            onBlur={field.onBlur}
            onChange={(accelerator) => {
              field.onChange(accelerator);
              if (accelerator !== props.value) {
                commit();
              }
            }}
          />
          <FieldError errors={[fieldState.error]} />
        </div>
      )}
    />
  );
}
