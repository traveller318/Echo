/**
 * SOURCE OF TRUTH KEYWORDS: termFormSchema, TermFormValues, withTerm, withoutTerm, matchesTerm, dictionary term rules, dictionary search, TermEdit
 * WHAT:  The pure rules of the Dictionary page: `termFormSchema(kind, others)` validates one term (word → written as)
 *        against the rest of the list, `withTerm` / `withoutTerm` build the whole list to save after an add, an edit
 *        or a delete, and `matchesTerm` filters the saved terms by a search.
 * WHY:   Every input is validated against a declared schema (root CLAUDE.md §5), and the dictionary's rules already
 *        live in the registry schema of the Pairs setting (lib/setting-schema.ts, the mirror of Rust's check): blank
 *        word, length, control characters, a word listed twice ignoring case, too many entries. The term schema runs
 *        that list schema on the list as it would be saved, with the term last so a clash is always reported on the
 *        term being typed, so the dialog and Rust can never disagree. Both sides are trimmed before saving: the
 *        polish rule matches a trimmed word, and stray spaces around "written as" would land in the text. A term keeps
 *        its place in the list when edited, since the order is the user's.
 * WHERE: routes/dictionary/_components/TermDialog.tsx (schema) and DictionaryTerms.tsx (list edits, search).
 */
import { z } from "zod";
import type { TextPair } from "@/bindings";
import { settingValueSchema, type SettingKindOf } from "@/lib/setting-schema";

export type TermFormValues = TextPair;

/** Which term the dialog edits: a new one, or the saved one at `index`. */
export type TermEdit = { readonly kind: "add" } | { readonly kind: "edit"; readonly index: number };

/** The schema of one term, given every other saved term. */
export function termFormSchema(
  kind: SettingKindOf<"pairs">,
  others: readonly TextPair[],
): z.ZodType<TermFormValues, TermFormValues> {
  const list = settingValueSchema("pairs", kind);
  const at = others.length;
  return z.object({ from: z.string(), to: z.string() }).superRefine((term, context) => {
    const parsed = list.safeParse([...others, trimmed(term)]);
    if (parsed.success) {
      return;
    }
    for (const issue of parsed.error.issues) {
      const [index, side] = issue.path;
      if (issue.path.length === 0) {
        // A rule about the whole list (too many entries) belongs to the word being added.
        context.addIssue({ code: "custom", message: issue.message, path: ["from"] });
      } else if (index === at && (side === "from" || side === "to")) {
        context.addIssue({ code: "custom", message: issue.message, path: [side] });
      }
    }
  });
}

export function trimmed(term: TextPair): TextPair {
  return { from: term.from.trim(), to: term.to.trim() };
}

/** Every term but the one at `index`. */
export function withoutTerm(terms: readonly TextPair[], index: number): TextPair[] {
  return terms.filter((_, at) => at !== index);
}

/** The list after `edit` saves `term`: appended when new, in place when edited. */
export function withTerm(terms: readonly TextPair[], edit: TermEdit, term: TextPair): TextPair[] {
  const clean = trimmed(term);
  return edit.kind === "add" ? [...terms, clean] : terms.map((saved, at) => (at === edit.index ? clean : saved));
}

/** Whether a term's word or spelling holds `query`, ignoring case; an empty query matches everything. */
export function matchesTerm(term: TextPair, query: string): boolean {
  const needle = query.trim().toLocaleLowerCase();
  return (
    needle === "" ||
    term.from.toLocaleLowerCase().includes(needle) ||
    term.to.toLocaleLowerCase().includes(needle)
  );
}
