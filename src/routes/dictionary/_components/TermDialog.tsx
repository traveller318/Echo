/**
 * SOURCE OF TRUTH KEYWORDS: TermDialog, add term dialog, edit term dialog, dictionary term form, word written as, React Hook Form Zod dialog
 * WHAT:  The modal that adds or edits one dictionary term: "Word" (what Echo hears) and "Written as" (how Echo should
 *        write it), validated by `termFormSchema` against the other saved terms; Save hands the trimmed term up.
 * WHY:   Every input is validated against a declared schema with React Hook Form, the shadcn way (root CLAUDE.md §5),
 *        and the schema is the registry's own Pairs rules (dictionary-terms.ts), so a term the dialog accepts is one
 *        Rust accepts. Errors show under the field that broke the rule. The form starts from `initial` each time the
 *        dialog opens, so a cancelled edit never leaks into the next one. Leaving "Written as" empty removes the word
 *        from dictations, as the polish rule does (02 §8.3).
 * WHERE: routes/dictionary/_components/DictionaryTerms.tsx.
 */
import { zodResolver } from "@hookform/resolvers/zod";
import { useEffect, useId, useMemo } from "react";
import { Controller, useForm, type Control } from "react-hook-form";
import type { TextPair } from "@/bindings";
import {
  Button,
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Field,
  FieldDescription,
  FieldError,
  FieldLabel,
  Input,
} from "@/components/ui";
import type { SettingKindOf } from "@/lib/setting-schema";
import { termFormSchema, type TermFormValues } from "./dictionary-terms";

export interface TermDialogProps {
  readonly open: boolean;
  /** "add" shows an empty form; "edit" starts from `initial`. */
  readonly mode: "add" | "edit";
  readonly initial: TextPair;
  /** Every saved term except the one being edited. */
  readonly others: readonly TextPair[];
  readonly kind: SettingKindOf<"pairs">;
  readonly onCancel: () => void;
  readonly onSave: (term: TextPair) => void;
}

export function TermDialog({ open, mode, initial, others, kind, onCancel, onSave }: TermDialogProps) {
  const schema = useMemo(() => termFormSchema(kind, others), [kind, others]);
  const form = useForm<TermFormValues>({ resolver: zodResolver(schema), defaultValues: initial, mode: "onChange" });

  useEffect(() => {
    if (open) {
      form.reset(initial);
    }
  }, [form, initial, open]);

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          onCancel();
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{mode === "add" ? "Add term" : "Edit term"}</DialogTitle>
          <DialogDescription>Echo writes the word the way you set it, whatever case you say it in.</DialogDescription>
        </DialogHeader>
        <form
          noValidate
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            void form.handleSubmit((term) => {
              onSave(term);
            })();
          }}
        >
          <TermField control={form.control} name="from" label="Word" placeholder="e.g. type script" />
          <TermField
            control={form.control}
            name="to"
            label="Written as"
            placeholder="e.g. TypeScript"
            help="Leave empty to drop the word from your dictations."
          />
          <DialogFooter>
            <DialogClose asChild>
              <Button>Cancel</Button>
            </DialogClose>
            <Button type="submit" variant="primary">
              {mode === "add" ? "Add term" : "Save"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

interface TermFieldProps {
  readonly control: Control<TermFormValues>;
  readonly name: keyof TermFormValues;
  readonly label: string;
  readonly placeholder: string;
  readonly help?: string;
}

function TermField({ control, name, label, placeholder, help }: TermFieldProps) {
  const id = useId();
  const helpId = `${id}-help`;
  return (
    <Controller
      name={name}
      control={control}
      render={({ field, fieldState }) => (
        <Field data-invalid={fieldState.invalid}>
          <FieldLabel htmlFor={id}>{label}</FieldLabel>
          <Input
            {...field}
            id={id}
            autoComplete="off"
            spellCheck={false}
            placeholder={placeholder}
            aria-invalid={fieldState.invalid}
            aria-describedby={help === undefined ? undefined : helpId}
          />
          {help === undefined ? null : <FieldDescription id={helpId}>{help}</FieldDescription>}
          <FieldError errors={[fieldState.error]} />
        </Field>
      )}
    />
  );
}
