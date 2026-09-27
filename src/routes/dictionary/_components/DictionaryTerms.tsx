/**
 * SOURCE OF TRUTH KEYWORDS: DictionaryTerms, saved terms card, dictionary list, add term, edit term, delete term, search terms, term row
 * WHAT:  The "Saved terms" card of the Dictionary page for one Pairs setting: its label and count, "Add term", a
 *        searchable DataList of every term (word → written as) with Edit and Delete on each row, and the TermDialog
 *        that adds or edits one. Every change saves the whole list through `settings_set`.
 * WHY:   Rust owns the dictionary (root CLAUDE.md §7): the terms shown are the value in effect, never a copy, and a
 *        save only sends the next list; SettingsChanged refetches it. Each change starts from the value in effect and
 *        waits while a save is in flight or the value is being read again, so two quick edits can never undo each
 *        other. A refused save shows inline under the header (Rust's message), not as a toast. The list is the
 *        global DataList (search, keyboard, virtualization) with the row, actions and empty states as slots; Enter
 *        or a click on a row edits it. Search runs over the word and its spelling, ignoring case, capped at the
 *        registry's entry length.
 * WHERE: routes/dictionary/index.tsx, for the Pairs setting of the dictionary section.
 */
import { ArrowRightIcon, PencilIcon, PlusIcon, SearchXIcon, Trash2Icon } from "lucide-react";
import { useMemo, useState } from "react";
import type { NavIcon as NavIconName, SettingSpec, SettingValue, TextPair } from "@/bindings";
import { DataList, EmptyState, GlassSurface, NavIcon } from "@/components/global";
import { Badge, Button, FieldError } from "@/components/ui";
import { useSettingWrite } from "@/hooks";
import { inlineAppError } from "@/lib/app-error";
import { formatCount, NUMERIC_CLASS } from "@/lib/format";
import { unwrapSettingValue, wrapSettingValue, type SettingKindOf } from "@/lib/setting-schema";
import { matchesTerm, withoutTerm, withTerm, type TermEdit } from "./dictionary-terms";
import { TermDialog } from "./TermDialog";

export interface DictionaryTermsProps {
  /** The Pairs setting that holds the terms. */
  readonly spec: SettingSpec;
  readonly kind: SettingKindOf<"pairs">;
  /** The value in effect. */
  readonly value: SettingValue;
  /** The value is being read again (after a save); edits wait for it. */
  readonly refreshing: boolean;
  /** The page's icon, for the empty state. */
  readonly icon: NavIconName;
}

/** A saved term with its place in the list, so edits and deletes survive a filtered view. */
interface IndexedTerm {
  readonly term: TextPair;
  readonly index: number;
}

const EMPTY_TERM: TextPair = { from: "", to: "" };
const NO_TERMS: readonly TextPair[] = [];

export function DictionaryTerms({ spec, kind, value, refreshing, icon }: DictionaryTermsProps) {
  const write = useSettingWrite(spec.key);
  const [search, setSearch] = useState("");
  const [editing, setEditing] = useState<TermEdit | null>(null);
  const terms = unwrapSettingValue("pairs", value) ?? NO_TERMS;
  const busy = write.pending || refreshing;
  const full = terms.length >= kind.max_pairs;

  const shown = useMemo<IndexedTerm[]>(
    () => terms.map((term, index) => ({ term, index })).filter(({ term }) => matchesTerm(term, search)),
    [terms, search],
  );
  const dialog = useMemo(() => {
    if (editing === null || editing.kind === "add") {
      return { initial: EMPTY_TERM, others: terms };
    }
    return { initial: terms[editing.index] ?? EMPTY_TERM, others: withoutTerm(terms, editing.index) };
  }, [editing, terms]);

  const save = (next: TextPair[]) => {
    write.clearError();
    write.set(wrapSettingValue("pairs", next));
  };
  const openAdd = () => {
    write.clearError();
    setEditing({ kind: "add" });
  };
  const openEdit = (index: number) => {
    write.clearError();
    setEditing({ kind: "edit", index });
  };

  const empty =
    search.trim() === "" ? (
      <EmptyState
        icon={<NavIcon icon={icon} />}
        title="No terms yet"
        body="Add a word Echo gets wrong, such as a name or a product, and how you want it written."
        action={
          <Button variant="primary" disabled={busy || full} onClick={openAdd}>
            <PlusIcon aria-hidden="true" />
            Add term
          </Button>
        }
      />
    ) : (
      <EmptyState icon={<SearchXIcon />} title="No terms match" body="Try a different word or spelling." />
    );

  return (
    <GlassSurface asChild className="flex min-h-0 flex-1 flex-col gap-3 px-5 py-4">
      <section aria-label={spec.label}>
        <header className="flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2">
            <h2 className="text-title3 text-fg">{spec.label}</h2>
            <Badge className={NUMERIC_CLASS}>{formatCount(terms.length)}</Badge>
          </div>
          <Button variant="primary" size="sm" disabled={busy || full} aria-busy={write.pending} onClick={openAdd}>
            <PlusIcon aria-hidden="true" />
            Add term
          </Button>
        </header>
        <FieldError>{write.error === null ? undefined : inlineAppError(write.error)}</FieldError>
        <DataList<IndexedTerm>
          className="flex-1"
          label={spec.label}
          items={shown}
          getKey={({ index }) => String(index)}
          row={({ term }) => <TermRow term={term} />}
          actions={({ term, index }) => (
            <>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={`Edit ${term.from}`}
                disabled={busy}
                onClick={() => {
                  openEdit(index);
                }}
              >
                <PencilIcon aria-hidden="true" />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={`Delete ${term.from}`}
                disabled={busy}
                onClick={() => {
                  save(withoutTerm(terms, index));
                }}
              >
                <Trash2Icon aria-hidden="true" />
              </Button>
            </>
          )}
          onActivate={({ index }) => {
            if (!busy) {
              openEdit(index);
            }
          }}
          empty={empty}
          search={
            terms.length === 0
              ? undefined
              : { value: search, onChange: setSearch, label: "Search terms", placeholder: "Search terms", maxLength: kind.max_len }
          }
        />
        <TermDialog
          open={editing !== null}
          mode={editing?.kind ?? "add"}
          initial={dialog.initial}
          others={dialog.others}
          kind={kind}
          onCancel={() => {
            setEditing(null);
          }}
          onSave={(term) => {
            if (editing === null || busy) {
              return;
            }
            save(withTerm(terms, editing, term));
            setEditing(null);
          }}
        />
      </section>
    </GlassSurface>
  );
}

function TermRow({ term }: { readonly term: TextPair }) {
  return (
    <div data-slot="term-row" className="flex min-w-0 items-center gap-2">
      <span className="min-w-0 truncate text-body text-fg-secondary">{term.from}</span>
      <ArrowRightIcon aria-label="written as" className="size-icon-sm shrink-0 text-fg-tertiary" />
      {term.to === "" ? (
        <Badge>Removed</Badge>
      ) : (
        <span className="min-w-0 truncate text-callout text-fg">{term.to}</span>
      )}
    </div>
  );
}
