/**
 * SOURCE OF TRUTH KEYWORDS: Field, FieldLabel, FieldDescription, FieldError, form field, React Hook Form, shadcn field, validation message
 * WHAT:  The shadcn Field parts restyled to Echo tokens: `Field` (a labelled group, `data-invalid` when its value
 *        fails), `FieldLabel`, `FieldDescription` (help text) and `FieldError` (the first validation message, read
 *        out as it appears).
 * WHY:   Every input is validated against a declared Zod schema with React Hook Form, the shadcn way (root CLAUDE.md
 *        §5): a `Controller` renders the control inside a Field and hands `fieldState.error` to FieldError, and the
 *        control carries `aria-invalid`, which Input styles. One set of parts keeps every form's label, help and
 *        error identical. Error text stays --color-fg because --color-record would fail 4.5:1 as small text on
 *        glass (04 §7); the invalid colour is the control's border. A visually hidden label uses `sr-only`.
 * WHERE: DataList search (components/global/data-list), Settings rows (step 18), dictionary editor. Exported through
 *        components/ui/index.ts.
 */
import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/cn";

export function Field({ className, ...props }: ComponentProps<"div">) {
  return <div role="group" data-slot="field" className={cn("flex w-full flex-col gap-1", className)} {...props} />;
}

export function FieldLabel({ className, ...props }: ComponentProps<"label">) {
  return <label data-slot="field-label" className={cn("text-callout text-fg", className)} {...props} />;
}

export function FieldDescription({ className, ...props }: ComponentProps<"p">) {
  return <p data-slot="field-description" className={cn("text-footnote text-fg-secondary", className)} {...props} />;
}

export type FieldErrorProps = ComponentProps<"p"> & {
  /** The validation errors of the field (React Hook Form's `fieldState.error`); the first message is shown. */
  readonly errors?: readonly ({ readonly message?: string } | undefined)[];
};

export function FieldError({ className, errors = [], children, ...props }: FieldErrorProps) {
  const message: ReactNode = children ?? errors.find((error) => error?.message !== undefined)?.message;
  if (message === undefined || message === "") {
    return null;
  }
  return (
    <p role="alert" data-slot="field-error" className={cn("text-footnote text-fg", className)} {...props}>
      {message}
    </p>
  );
}
