/**
 * SOURCE OF TRUTH KEYWORDS: DeviceControl, Device setting, microphone picker, system default microphone, pinned device, disconnected device
 * WHAT:  The control of a `Device` setting: a Select of "System default" plus the microphones offered now, saved as
 *        soon as one is picked. A pinned microphone that is not connected stays listed as such, so the choice is
 *        visible and can be changed. Opening the list asks the caller for a fresh device list.
 * WHY:   A Device value is a device id or none (follow the Windows default, 02 §3.3). Radix Select cannot hold an
 *        empty or null value, so the default is the one text Rust can never store as a device id (blank ids fail
 *        `check_token`); it never leaves this file. Devices come and go, so the list is read again whenever it opens
 *        instead of being cached.
 * WHERE: SettingField, for SettingKind `device`; the options are the microphones from `audio_list_devices`.
 */
import { Controller } from "react-hook-form";
import { FieldError, Select, SelectContent, SelectItem, SelectSeparator, SelectTrigger, SelectValue } from "@/components/ui";
import { useSettingForm, type SettingControlProps } from "./setting-control";

/** Select value of "System default": blank, which is never a valid stored device id. */
const SYSTEM_DEFAULT = " ";

export function DeviceControl(props: SettingControlProps<"device">) {
  const { form, commit } = useSettingForm("device", props);
  const connected = (id: string) => props.options.some((option) => option.value === id);

  return (
    <Controller
      name="value"
      control={form.control}
      render={({ field, fieldState }) => (
        <div className="flex w-full flex-col gap-1">
          <Select
            value={field.value ?? SYSTEM_DEFAULT}
            onValueChange={(next) => {
              field.onChange(next === SYSTEM_DEFAULT ? null : next);
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
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={SYSTEM_DEFAULT}>System default</SelectItem>
              {props.options.length > 0 || (field.value !== null && !connected(field.value)) ? (
                <SelectSeparator />
              ) : null}
              {props.options.map((option) => (
                <SelectItem key={option.value} value={option.value}>
                  {option.label}
                </SelectItem>
              ))}
              {field.value !== null && !connected(field.value) ? (
                <SelectItem value={field.value}>Saved microphone (not connected)</SelectItem>
              ) : null}
            </SelectContent>
          </Select>
          <FieldError errors={[fieldState.error]} />
        </div>
      )}
    />
  );
}
