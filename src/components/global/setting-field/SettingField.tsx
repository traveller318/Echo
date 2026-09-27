/**
 * SOURCE OF TRUTH KEYWORDS: SettingField, setting row, registry setting control, generated settings UI, reset to default, restart required, inline setting error
 * WHAT:  Renders any registry SettingSpec as one Settings row (04 §5): label and help on the left, the control its
 *        kind calls for on the right (Bool, Int, Enum, Hotkey, Device, Text, Pairs), a reset button when the value
 *        differs from the default, a "Needs restart" badge when the spec says so, and the last write's error inline.
 * WHY:   Adding a setting is a registry entry, never a new component (root CLAUDE.md §7): the kind picks the control
 *        through one exhaustive switch (a new SettingKind fails tsc here), and everything else (label, help, bounds,
 *        options, default) is the spec's. The field is controlled and data-agnostic: the caller passes the value in
 *        effect, the options offered now and what to do on commit or reset, so Settings, onboarding or any later
 *        surface reuse it with their own data source. A refused write shows beside the control with a
 *        --color-record glyph and border, and readable text (04 §7); the control itself returns to the value in effect.
 *        A control that needs the row's width (an Enum drawn as preview cards) goes under the label and help instead
 *        of beside them; the spec says so through its display, never through its key.
 * WHERE: routes/settings (every visible setting); onboarding's hotkey and microphone steps (step 24).
 */
import { CircleAlertIcon, RotateCcwIcon, RotateCwIcon } from "lucide-react";
import { useId } from "react";
import type { AppError, EnumOption, SettingKind, SettingSpec, SettingValue } from "@/bindings";
import {
  Badge,
  Button,
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui";
import { inlineAppError } from "@/lib/app-error";
import { cn } from "@/lib/cn";
import { unwrapSettingValue, wrapSettingValue, type SettingKindName, type SettingValueOf } from "@/lib/setting-schema";
import { BoolControl } from "./BoolControl";
import { DeviceControl } from "./DeviceControl";
import { EnumControl } from "./EnumControl";
import { HotkeyControl } from "./HotkeyControl";
import { IntControl } from "./IntControl";
import { PairsControl } from "./PairsControl";
import type { SettingControlProps } from "./setting-control";
import { TextControl } from "./TextControl";

export interface SettingFieldProps {
  readonly spec: SettingSpec;
  /** The value in effect. */
  readonly value: SettingValue;
  /** What an Enum or Device setting offers now; ignored by the other kinds. */
  readonly options?: readonly EnumOption[];
  readonly onCommit: (value: SettingValue) => void;
  /** Return to the registry default; without it no reset button is shown. */
  readonly onReset?: () => void;
  /** Why the last write failed, shown inline; null when it succeeded. */
  readonly error?: AppError | null;
  /** A write is in flight. */
  readonly pending?: boolean;
  /** The option list was opened (a Device list reads the microphones again). */
  readonly onOptionsOpen?: () => void;
  /** A Hotkey control started (true) or stopped (false) capturing a combination. */
  readonly onCaptureChange?: (capturing: boolean) => void;
  readonly className?: string;
}

const NO_OPTIONS: readonly EnumOption[] = [];

/** The control needs the whole row width: it sits under the label instead of beside it. */
function isWideControl(kind: SettingKind): boolean {
  return kind.kind === "enum" && kind.display.as === "cards";
}

/** Same value, compared by content (Pairs are arrays). */
function sameValue(left: SettingValue, right: SettingValue): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

interface ControlBase {
  readonly id: string;
  readonly describedBy: string;
  readonly label: string;
  readonly options: readonly EnumOption[];
  readonly invalid: boolean;
  readonly onCommit: (value: SettingValue) => void;
  readonly onOptionsOpen?: () => void;
  readonly onCaptureChange?: (capturing: boolean) => void;
}

/** The kind-narrowed props for a control of kind `name`, falling back to the spec default on a kind mismatch. */
function controlProps<K extends SettingKindName>(
  name: K,
  kind: SettingControlProps<K>["kind"],
  value: SettingValue,
  fallback: SettingValue,
  base: ControlBase,
): SettingControlProps<K> | null {
  const withValue = (plain: SettingValueOf<K>): SettingControlProps<K> => ({
    ...base,
    kind,
    value: plain,
    onCommit: (next) => {
      base.onCommit(wrapSettingValue(name, next));
    },
  });
  // Checked against undefined only: a Device value of null (the system default) is a real value, not a missing one.
  const own = unwrapSettingValue(name, value);
  if (own !== undefined) {
    return withValue(own);
  }
  const fallbackValue = unwrapSettingValue(name, fallback);
  return fallbackValue === undefined ? null : withValue(fallbackValue);
}

/**
 * SOURCE OF TRUTH KEYWORDS: renderControl, kind to control, exhaustive SettingKind switch
 * WHAT:  The control element for the spec's kind.
 * WHY:   One switch over the generated SettingKind union; `satisfies never` makes a new kind a compile error here.
 * WHERE: SettingField.
 */
function renderControl(spec: SettingSpec, value: SettingValue, base: ControlBase) {
  const kind = spec.kind;
  switch (kind.kind) {
    case "bool": {
      const props = controlProps("bool", kind, value, spec.default, base);
      return props === null ? null : <BoolControl {...props} />;
    }
    case "int": {
      const props = controlProps("int", kind, value, spec.default, base);
      return props === null ? null : <IntControl {...props} />;
    }
    case "enum": {
      const props = controlProps("enum", kind, value, spec.default, base);
      return props === null ? null : <EnumControl {...props} />;
    }
    case "hotkey": {
      const props = controlProps("hotkey", kind, value, spec.default, base);
      return props === null ? null : <HotkeyControl {...props} />;
    }
    case "device": {
      const props = controlProps("device", kind, value, spec.default, base);
      return props === null ? null : <DeviceControl {...props} />;
    }
    case "text": {
      const props = controlProps("text", kind, value, spec.default, base);
      return props === null ? null : <TextControl {...props} />;
    }
    case "pairs": {
      const props = controlProps("pairs", kind, value, spec.default, base);
      return props === null ? null : <PairsControl {...props} />;
    }
    default:
      return kind satisfies never;
  }
}

export function SettingField({
  spec,
  value,
  options = NO_OPTIONS,
  onCommit,
  onReset,
  error = null,
  pending = false,
  onOptionsOpen,
  onCaptureChange,
  className,
}: SettingFieldProps) {
  const id = useId();
  const helpId = useId();
  const invalid = error !== null;
  const control = renderControl(spec, value, {
    id,
    describedBy: helpId,
    label: spec.label,
    options,
    invalid,
    onCommit,
    onOptionsOpen,
    onCaptureChange,
  });
  const canReset = onReset !== undefined && !sameValue(value, spec.default);
  const wide = isWideControl(spec.kind);

  return (
    <Field
      data-invalid={invalid}
      data-setting={spec.key}
      aria-busy={pending}
      className={cn(
        wide ? "flex-col items-stretch gap-3 py-3" : "flex-row items-start justify-between gap-6 py-3",
        className,
      )}
    >
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex items-center gap-2">
          <FieldLabel htmlFor={id}>{spec.label}</FieldLabel>
          {spec.restart_required ? (
            <Badge variant="warning">
              <RotateCwIcon aria-hidden="true" />
              Needs restart
            </Badge>
          ) : null}
        </div>
        <FieldDescription id={helpId}>
          {spec.help}
          {spec.restart_required ? " Takes effect the next time Echo starts." : null}
        </FieldDescription>
      </div>
      <div className={cn("flex shrink-0 flex-col items-end gap-1", wide ? "w-full" : "w-setting-control")}>
        <div className="flex w-full items-center justify-end gap-1">
          {control}
          {canReset ? (
            <Tooltip>
              <TooltipTrigger asChild>
                <Button variant="ghost" size="icon-sm" aria-label={`Reset ${spec.label} to default`} onClick={onReset}>
                  <RotateCcwIcon aria-hidden="true" />
                </Button>
              </TooltipTrigger>
              <TooltipContent>Reset to default</TooltipContent>
            </Tooltip>
          ) : null}
        </div>
        {error === null ? null : (
          <FieldError className="flex items-start gap-1 self-stretch">
            <CircleAlertIcon aria-hidden="true" className="size-icon-sm shrink-0 text-record" />
            <span>{inlineAppError(error)}</span>
          </FieldError>
        )}
      </div>
    </Field>
  );
}
