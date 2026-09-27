/**
 * SOURCE OF TRUTH KEYWORDS: EnumControl, Enum setting, Select setting control, segmented setting control, preview cards setting control, runtime options, no options available
 * WHAT:  The control of an `Enum` setting, drawn the way its spec's EnumDisplay says: a Select of the options offered
 *        now, a segmented row of buttons, or a row of cards each showing a preview of its option. Every display saves
 *        as soon as an option is picked. With nothing to offer the Select is disabled and says so; a value that is no
 *        longer offered (an engine that was removed) shows the placeholder (or no choice) instead of a blank.
 * WHY:   Options come from `settings_availability` (fixed lists filtered by caps, runtime sources resolved by Rust),
 *        so a new engine or language appears here with no UI change, and the Zod schema accepts exactly them. The
 *        segmented and card displays are Radix radio groups, so they keep one tab stop, arrow keys and
 *        aria-checked; their look is tokens only. The display is data from the registry, so no setting key is
 *        matched here.
 * WHERE: SettingField, for SettingKind `enum`.
 */
import { CheckIcon } from "lucide-react";
import { Controller, type ControllerRenderProps } from "react-hook-form";
import type { EnumDisplay, EnumOption, EnumPreview } from "@/bindings";
import {
  FieldError,
  RadioGroup,
  RadioGroupItem,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui";
import { cn } from "@/lib/cn";
import type { SettingFormValues } from "@/lib/setting-schema";
import { EnumPreviewFor } from "./enum-previews";
import { optionLabel } from "./option-label";
import { useSettingForm, type SettingControlProps } from "./setting-control";

type EnumField = ControllerRenderProps<SettingFormValues<"enum">, "value">;

interface DisplayProps {
  readonly control: SettingControlProps<"enum">;
  readonly field: EnumField;
  readonly invalid: boolean;
  readonly choose: (value: string) => void;
}

/** The stored value when it is offered now, else "" (no choice shown). */
function offeredValue(options: readonly EnumOption[], value: string): string {
  return options.some((option) => option.value === value) ? value : "";
}

/** A Select of the options (plain render helpers, called inside the Controller render, so `field.ref` stays there). */
function selectDisplay({ control, field, invalid, choose }: DisplayProps) {
  const empty = control.options.length === 0;
  return (
    <Select
      // "" shows the placeholder when the stored value is not offered now.
      value={offeredValue(control.options, field.value)}
      disabled={empty}
      onValueChange={choose}
      onOpenChange={(open) => {
        if (open) {
          control.onOptionsOpen?.();
        }
      }}
    >
      <SelectTrigger
        id={control.id}
        ref={field.ref}
        aria-describedby={control.describedBy}
        aria-invalid={invalid}
        onBlur={field.onBlur}
      >
        <SelectValue placeholder={empty ? "Nothing to choose yet" : "Choose an option"} />
      </SelectTrigger>
      <SelectContent>
        {control.options.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {optionLabel(control.kind.options, option)}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function segmentedDisplay({ control, field, invalid, choose }: DisplayProps) {
  return (
    <RadioGroup
      id={control.id}
      ref={field.ref}
      // A radiogroup is not labelable by <label for>, so it carries the row's label itself.
      aria-label={control.label}
      aria-describedby={control.describedBy}
      aria-invalid={invalid}
      value={offeredValue(control.options, field.value)}
      onValueChange={choose}
      onBlur={field.onBlur}
      orientation="horizontal"
      className={cn(
        "h-control w-full gap-1 rounded-control bg-fill p-1",
        invalid && "border-(length:--border-hairline) border-record",
      )}
    >
      {control.options.map((option) => (
        <RadioGroupItem
          key={option.value}
          value={option.value}
          className={cn(
            "min-w-0 flex-1 truncate rounded-sm px-2 text-callout text-fg-secondary hover:bg-fill-hover",
            "data-[state=checked]:bg-fill-pressed data-[state=checked]:font-medium data-[state=checked]:text-fg",
          )}
        >
          {optionLabel(control.kind.options, option)}
        </RadioGroupItem>
      ))}
    </RadioGroup>
  );
}

function cardsDisplay({ control, field, invalid, choose }: DisplayProps, preview: EnumPreview) {
  const chosen = offeredValue(control.options, field.value);
  return (
    <RadioGroup
      id={control.id}
      ref={field.ref}
      // A radiogroup is not labelable by <label for>, so it carries the row's label itself.
      aria-label={control.label}
      aria-describedby={control.describedBy}
      aria-invalid={invalid}
      value={chosen}
      onValueChange={choose}
      onBlur={field.onBlur}
      orientation="horizontal"
      className="w-full gap-3"
    >
      {control.options.map((option) => (
        <RadioGroupItem
          key={option.value}
          value={option.value}
          className={cn(
            "relative flex min-w-0 flex-1 flex-col items-center gap-3 rounded-control p-4",
            "border-(length:--border-hairline) border-separator bg-fill hover:bg-fill-hover",
            "data-[state=checked]:border-accent data-[state=checked]:bg-accent-soft",
            invalid && "border-record",
          )}
        >
          {chosen === option.value ? (
            <span className="absolute top-2 right-2 flex size-icon-md items-center justify-center rounded-pill bg-accent text-accent-fg">
              <CheckIcon aria-hidden="true" className="size-icon-sm" />
            </span>
          ) : null}
          <span className="flex h-control items-center justify-center">
            <EnumPreviewFor preview={preview} value={option.value} />
          </span>
          <span className="truncate text-footnote text-fg-secondary">{optionLabel(control.kind.options, option)}</span>
        </RadioGroupItem>
      ))}
    </RadioGroup>
  );
}

/** The display a spec asks for, over one form field. */
function renderDisplay(display: EnumDisplay, props: DisplayProps) {
  switch (display.as) {
    case "select":
      return selectDisplay(props);
    case "segmented":
      return segmentedDisplay(props);
    case "cards":
      return cardsDisplay(props, display.preview);
    default:
      return display satisfies never;
  }
}

export function EnumControl(props: SettingControlProps<"enum">) {
  const { form, commit } = useSettingForm("enum", props);

  return (
    <Controller
      name="value"
      control={form.control}
      render={({ field, fieldState }) => (
        <div className="flex w-full flex-col gap-1">
          {renderDisplay(props.kind.display, {
            control: props,
            field,
            invalid: fieldState.invalid || props.invalid,
            choose: (next) => {
              field.onChange(next);
              commit();
            },
          })}
          <FieldError errors={[fieldState.error]} />
        </div>
      )}
    />
  );
}
