/**
 * SOURCE OF TRUTH KEYWORDS: DataListSearch, list search field, search schema, Zod search, React Hook Form search, clear search, Escape clears
 * WHAT:  The search field DataList owns: a labelled Input with a search glyph and a clear button, validated by a
 *        Zod schema (at most `maxLength` characters) through React Hook Form; every valid value is reported with
 *        `onChange`. Escape clears it, Enter or ArrowDown hands focus to the list (`onLeave`).
 * WHY:   Every input is validated against a declared schema (root CLAUDE.md §5), and the limit matches the one Rust
 *        enforces for the same command (HistoryListInput.MAX_SEARCH_CHARS), so an over-long search shows the
 *        error inline instead of reaching IPC. Values are reported as typed (no debounce timer): the caller's query
 *        keeps the previous rows on screen while the next ones load (useEchoInfiniteQuery). A value set from
 *        outside (the caller clears it) resets the form.
 * WHERE: DataList.tsx (the `search` prop).
 */
import { zodResolver } from "@hookform/resolvers/zod";
import { SearchIcon, XIcon } from "lucide-react";
import { useEffect, useId, useMemo, useRef, type KeyboardEvent } from "react";
import { Controller, useForm, useWatch } from "react-hook-form";
import { z } from "zod";
import { Button, Field, FieldError, FieldLabel, Input } from "@/components/ui";

export interface DataListSearchProps {
  readonly value: string;
  readonly onChange: (value: string) => void;
  /** Accessible name of the field (visually hidden). */
  readonly label: string;
  readonly placeholder?: string;
  /** Longest search accepted; the same limit the command validates. */
  readonly maxLength: number;
}

interface SearchValues {
  readonly query: string;
}

export function DataListSearch({
  value,
  onChange,
  label,
  placeholder,
  maxLength,
  onLeave,
}: DataListSearchProps & { readonly onLeave: () => void }) {
  const id = useId();
  const schema = useMemo(
    () =>
      z.object({
        query: z.string().max(maxLength, `Keep the search under ${String(maxLength)} characters.`),
      }),
    [maxLength],
  );
  const form = useForm<SearchValues>({
    resolver: zodResolver(schema),
    defaultValues: { query: value },
    mode: "onChange",
  });
  const report = useRef(onChange);
  useEffect(() => {
    report.current = onChange;
  }, [onChange]);

  const query = useWatch({ control: form.control, name: "query" });
  useEffect(() => {
    const parsed = schema.safeParse({ query });
    if (parsed.success) {
      report.current(parsed.data.query);
    }
  }, [query, schema]);

  useEffect(() => {
    if (form.getValues("query") !== value) {
      form.reset({ query: value });
    }
  }, [form, value]);

  const clear = () => {
    form.setValue("query", "", { shouldValidate: true });
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape" && form.getValues("query") !== "") {
      event.preventDefault();
      clear();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      onLeave();
    }
  };

  return (
    <form
      role="search"
      data-slot="data-list-search"
      noValidate
      onSubmit={(event) => {
        event.preventDefault();
        onLeave();
      }}
    >
      <Controller
        name="query"
        control={form.control}
        render={({ field, fieldState }) => (
          <Field data-invalid={fieldState.invalid}>
            <FieldLabel htmlFor={id} className="sr-only">
              {label}
            </FieldLabel>
            <div className="relative flex items-center">
              <SearchIcon aria-hidden="true" className="pointer-events-none absolute left-3 size-icon-sm text-fg-tertiary" />
              <Input
                {...field}
                id={id}
                type="search"
                autoComplete="off"
                spellCheck={false}
                placeholder={placeholder}
                aria-invalid={fieldState.invalid}
                onKeyDown={onKeyDown}
                className="pr-10 pl-8 [&::-webkit-search-cancel-button]:hidden"
              />
              {field.value === "" ? null : (
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label="Clear search"
                  className="absolute right-1"
                  onClick={clear}
                >
                  <XIcon aria-hidden="true" />
                </Button>
              )}
            </div>
            <FieldError errors={[fieldState.error]} />
          </Field>
        )}
      />
    </form>
  );
}
