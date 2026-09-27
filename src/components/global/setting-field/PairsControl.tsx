/**
 * SOURCE OF TRUTH KEYWORDS: PairsControl, Pairs setting, dictionary editor, replacement pairs, useFieldArray, dictionary sheet
 * WHAT:  The control of a `Pairs` setting (the dictionary): the row shows how many entries there are and an Edit
 *        button; the Sheet it opens edits every pair (word → written as), adds and removes rows, and saves them all
 *        at once. Closing without saving discards the edits.
 * WHY:   Up to 500 two-sided entries do not fit a settings row, and saving per keystroke would rebuild the polish
 *        chain's dictionary on every letter, so edits are drafted in the sheet and committed together. The rows are a
 *        React Hook Form field array validated by the registry schema: each rule (blank word, length, characters,
 *        a word listed twice ignoring case) is reported on the row that breaks it with Rust's message, and nothing is
 *        saved until every row passes. An empty "written as" removes the word, as the polish rule does (02 §8.3).
 * WHERE: SettingField, for any SettingKind `pairs` row. The dictionary (`dictionary.entries`) is not drawn here: it
 *        has its own page with a searchable term list (routes/dictionary).
 */
import { PlusIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { useFieldArray } from "react-hook-form";
import {
  Button,
  FieldError,
  Input,
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui";
import { formatWords } from "@/lib/format";
import { useSettingForm, type SettingControlProps } from "./setting-control";

/** A React Hook Form array index: the digits of a whole number, narrowed by a check rather than a cast. */
function isIndexKey(key: string): key is `${number}` {
  return /^\d+$/.test(key);
}

export function PairsControl(props: SettingControlProps<"pairs">) {
  const { form } = useSettingForm("pairs", props);
  const pairs = useFieldArray({ control: form.control, name: "value" });
  const [open, setOpen] = useState(false);
  const { errors } = form.formState;
  const listError = errors.value?.root?.message ?? errors.value?.message;
  const full = pairs.fields.length >= props.kind.max_pairs;

  const close = () => {
    form.reset();
    setOpen(false);
  };
  const save = () => {
    void form.handleSubmit((submitted) => {
      props.onCommit(submitted.value);
      setOpen(false);
    })();
  };

  return (
    <div className="flex w-full items-center justify-end gap-3">
      <span className="text-footnote text-fg-secondary">
        {props.value.length === 0 ? "No entries yet" : formatWords(props.value.length)}
      </span>
      <Button
        id={props.id}
        size="sm"
        variant="secondary"
        aria-describedby={props.describedBy}
        aria-invalid={props.invalid}
        onClick={() => {
          setOpen(true);
        }}
      >
        Edit
      </Button>
      <Sheet
        open={open}
        onOpenChange={(next) => {
          if (!next) {
            close();
          }
        }}
      >
        <SheetContent>
          <SheetHeader>
            <SheetTitle>{props.label}</SheetTitle>
            <SheetDescription>
              Echo writes each word the way you set it. Leave the second field empty to remove the word.
            </SheetDescription>
          </SheetHeader>
          <form
            noValidate
            className="flex min-h-0 flex-1 flex-col gap-3"
            onSubmit={(event) => {
              event.preventDefault();
              save();
            }}
          >
            <ul className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto" aria-label={props.label}>
              {pairs.fields.map((pair, index) => {
                const key = String(index);
                if (!isIndexKey(key)) {
                  return null;
                }
                const rowErrors = errors.value?.[index];
                const row = String(index + 1);
                return (
                  <li key={pair.id} className="flex flex-col gap-1">
                    <div className="flex items-center gap-2">
                      <Input
                        aria-label={`Word ${row}`}
                        placeholder="Word"
                        autoComplete="off"
                        spellCheck={false}
                        aria-invalid={rowErrors?.from !== undefined}
                        {...form.register(`value.${key}.from`)}
                      />
                      <Input
                        aria-label={`Write word ${row} as`}
                        placeholder="Written as"
                        autoComplete="off"
                        spellCheck={false}
                        aria-invalid={rowErrors?.to !== undefined}
                        {...form.register(`value.${key}.to`)}
                      />
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        aria-label={`Remove word ${row}`}
                        onClick={() => {
                          pairs.remove(index);
                        }}
                      >
                        <Trash2Icon aria-hidden="true" />
                      </Button>
                    </div>
                    <FieldError errors={[rowErrors?.from, rowErrors?.to]} />
                  </li>
                );
              })}
            </ul>
            <FieldError>{listError}</FieldError>
            <Button
              variant="ghost"
              size="sm"
              className="self-start"
              disabled={full}
              onClick={() => {
                pairs.append({ from: "", to: "" });
              }}
            >
              <PlusIcon aria-hidden="true" />
              Add entry
            </Button>
            <SheetFooter>
              <Button variant="secondary" onClick={close}>
                Cancel
              </Button>
              <Button type="submit">Save</Button>
            </SheetFooter>
          </form>
        </SheetContent>
      </Sheet>
    </div>
  );
}
