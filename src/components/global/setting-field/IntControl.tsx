/**
 * SOURCE OF TRUTH KEYWORDS: IntControl, Int setting, Slider setting control, number field, unit step, bounded integer setting
 * WHAT:  The control of an `Int` setting: a Slider over the spec's range next to a number field with the unit
 *        (`3000 ms`, `7 days`). Dragging or arrow keys save when released; the field saves on Enter or when it loses
 *        focus.
 * WHY:   A slider is quick for "about this much" and a number field is exact (0–3650 days cannot be hit precisely by
 *        dragging); both edit the same form value, so they never disagree, and an out-of-range number shows the Rust
 *        message inline instead of saving. The keyboard step comes from the unit, so arrow keys move a countdown in
 *        tenths of a second, not in single milliseconds; Rust accepts any whole number in range, so the step is only
 *        a convenience.
 * WHERE: SettingField, for SettingKind `int`.
 */
import { Controller } from "react-hook-form";
import type { SettingUnit } from "@/bindings";
import { FieldError, Input, Slider } from "@/components/ui";
import { formatSettingUnit, NUMERIC_CLASS } from "@/lib/format";
import { cn } from "@/lib/cn";
import { useSettingForm, type SettingControlProps } from "./setting-control";

/** How far one arrow key moves a value, per unit; a unit-less int moves by one. */
const UNIT_STEP: Readonly<Record<SettingUnit, number>> = {
  milliseconds: 100,
  minutes: 1,
  days: 1,
  words_per_minute: 1,
};

export function IntControl(props: SettingControlProps<"int">) {
  const { form, commit } = useSettingForm("int", props);
  const { min, max, unit } = props.kind;
  const step = unit === null ? 1 : UNIT_STEP[unit];
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
          <div className="flex w-full items-center gap-3">
            <Slider
              aria-label={props.label}
              min={min}
              max={max}
              step={step}
              value={[Number.isFinite(field.value) ? Math.min(max, Math.max(min, field.value)) : min]}
              onValueChange={([next]) => {
                if (next !== undefined) {
                  field.onChange(next);
                }
              }}
              onValueCommit={commitIfChanged}
            />
            <Input
              id={props.id}
              ref={field.ref}
              type="number"
              inputMode="numeric"
              min={min}
              max={max}
              step={step}
              aria-describedby={props.describedBy}
              aria-invalid={fieldState.invalid || props.invalid}
              className={cn("w-number-field shrink-0 px-2 text-right", NUMERIC_CLASS)}
              value={Number.isFinite(field.value) ? field.value : ""}
              onChange={(event) => {
                field.onChange(event.currentTarget.valueAsNumber);
              }}
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
            {unit === null ? null : (
              <span className="shrink-0 text-footnote text-fg-secondary">{formatSettingUnit(unit)}</span>
            )}
          </div>
          <FieldError errors={[fieldState.error]} />
        </div>
      )}
    />
  );
}
