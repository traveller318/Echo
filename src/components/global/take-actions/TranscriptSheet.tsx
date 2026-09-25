/**
 * SOURCE OF TRUTH KEYWORDS: TranscriptSheet, take detail drawer, full transcript text, take details, raw text, take failure reason
 * WHAT:  The detail drawer for one take: when it happened and its status, its full text (the polished
 *        text, else the raw text a failed delivery left, labelled as such), why it failed when it did, its
 *        measurements, and Copy / Retry / Delete.
 * WHY:   04 §5: the list shows two lines; the drawer shows everything without leaving the list. It reads the take
 *        through useTranscript, so it refreshes from HistoryChanged / TranscriptSaved like the list (a retry's new
 *        text appears in place) and closes itself when the take is deleted elsewhere (NotFound). The text is
 *        selectable for partial copies. Measurements that were never taken are left out, never shown as zero.
 * WHERE: TakeOverlays (this folder), opened on row activation in History and the Dashboard's recent takes.
 */
import { CopyIcon, RotateCcwIcon, Trash2Icon } from "lucide-react";
import { useEffect, type ReactNode } from "react";
import type { Transcript, TranscriptId } from "@/bindings";
import {
  Button,
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui";
import { useTranscript, type TranscriptActions } from "@/hooks";
import { describeAppError, describeTakeFailure, toAppError } from "@/lib/app-error";
import { cn } from "@/lib/cn";
import { formatDuration, formatMilliseconds, formatTakeTime, formatWords, NUMERIC_CLASS } from "@/lib/format";
import { EmptyState } from "../empty-state";
import { ProgressBar } from "../progress-bar";
import { TranscriptStatusBadge } from "../transcript-row";
import { takeAvailability } from "./take-availability";

export interface TranscriptSheetProps {
  /** The take to show; null keeps the sheet closed. */
  readonly id: TranscriptId | null;
  readonly onClose: () => void;
  readonly actions: TranscriptActions;
  readonly onDelete: (take: Transcript) => void;
}

export function TranscriptSheet({ id, onClose, actions, onDelete }: TranscriptSheetProps) {
  const query = useTranscript(id);
  const missing = query.isError && toAppError(query.error).code === "NotFound";
  useEffect(() => {
    if (missing) {
      onClose();
    }
  }, [missing, onClose]);

  return (
    <Sheet
      open={id !== null}
      onOpenChange={(open) => {
        if (!open) {
          onClose();
        }
      }}
    >
      {/* While loading there is no description yet; once loaded, Radix links the status line as the description. */}
      <SheetContent {...(query.data === undefined ? { "aria-describedby": undefined } : {})}>
        {query.data === undefined ? (
          <SheetLoading error={query.isError ? describeAppError(toAppError(query.error)).title : null} />
        ) : (
          <TakeDetails take={query.data} actions={actions} onDelete={onDelete} />
        )}
      </SheetContent>
    </Sheet>
  );
}

function SheetLoading({ error }: { readonly error: string | null }) {
  return (
    <>
      <SheetHeader>
        <SheetTitle>Take</SheetTitle>
      </SheetHeader>
      {error === null ? <ProgressBar value={null} aria-label="Loading the take" /> : <EmptyState title={error} titleAs="p" />}
    </>
  );
}

function TakeDetails({
  take,
  actions,
  onDelete,
}: {
  readonly take: Transcript;
  readonly actions: TranscriptActions;
  readonly onDelete: (take: Transcript) => void;
}) {
  const can = takeAvailability(take);
  const retrying = actions.retry.isPending && actions.retry.variables.id === take.id;
  const text = take.final_text ?? take.raw_text;
  const details: readonly (readonly [string, string | null])[] = [
    ["Length", take.duration_ms === null ? null : formatDuration(take.duration_ms)],
    ["Speech", take.speech_ms === null ? null : formatDuration(take.speech_ms)],
    ["Words", take.word_count === null ? null : formatWords(take.word_count)],
    ["Delivered to", take.app_name],
    ["Speech engine", take.engine_id],
    ["Language", take.language],
    ["Stop to paste", take.latency_ms === null ? null : formatMilliseconds(take.latency_ms)],
  ];
  return (
    <>
      <SheetHeader>
        <SheetTitle className={NUMERIC_CLASS}>{formatTakeTime(take.created_at)}</SheetTitle>
        <SheetDescription asChild>
          <div className="flex items-center gap-2">
            <TranscriptStatusBadge status={take.status} />
            {take.error_code === null ? null : <span>{describeTakeFailure(take.error_code)}</span>}
          </div>
        </SheetDescription>
      </SheetHeader>
      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto">
        {text === null ? (
          <p className="text-body text-fg-tertiary">No text yet.</p>
        ) : (
          <Section label={take.final_text === null ? "Heard, before cleanup" : "Text"}>
            <p className="text-body whitespace-pre-wrap text-fg select-text">{text}</p>
          </Section>
        )}
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-footnote">
          {details
            .filter((detail): detail is readonly [string, string] => detail[1] !== null && detail[1] !== "")
            .map(([label, value]) => (
              <div key={label} className="flex flex-col">
                <dt className="text-fg-secondary">{label}</dt>
                <dd className={cn("text-fg", NUMERIC_CLASS)}>{value}</dd>
              </div>
            ))}
        </dl>
      </div>
      <SheetFooter>
        <Button
          variant="ghost"
          disabled={!can.remove}
          onClick={() => {
            onDelete(take);
          }}
        >
          <Trash2Icon aria-hidden="true" />
          Delete
        </Button>
        <Button
          disabled={!can.retry || retrying}
          aria-busy={retrying}
          onClick={() => {
            actions.retry.run(take.id);
          }}
        >
          <RotateCcwIcon aria-hidden="true" />
          {retrying ? "Retrying" : "Retry"}
        </Button>
        <Button
          variant="primary"
          disabled={!can.copy || text === null}
          onClick={() => {
            actions.copy.run(take.id);
          }}
        >
          <CopyIcon aria-hidden="true" />
          Copy
        </Button>
      </SheetFooter>
    </>
  );
}

function Section({ label, children }: { readonly label: string; readonly children: ReactNode }) {
  return (
    <section className="flex flex-col gap-1">
      <h3 className="text-footnote text-fg-secondary">{label}</h3>
      {children}
    </section>
  );
}
