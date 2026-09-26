/**
 * SOURCE OF TRUTH KEYWORDS: TRANSCRIPT_STATUS_LOOK, transcript status badge, status glyph, status label, TranscriptStatus copy, badge variant per status, transcriptPlaceholder
 * WHAT:  How each TranscriptStatus looks: its badge label, badge variant and lucide glyph, plus the placeholder a row
 *        shows when the take has no text yet (and `transcriptPlaceholder`, which picks it for a take).
 * WHY:   Status is never colour alone (04 §7): every status has a glyph and a word. The table is keyed by the
 *        generated TranscriptStatus union, so a new status in Rust fails tsc until it has a look here, and no
 *        component switches on a status. Colours follow 04 §3.1: red only for a take that is recording, orange
 *        for attention (failed, recovered), green for done. Copy is calm (04 §1).
 * WHERE: TranscriptStatusBadge and TranscriptRow (this folder).
 */
import {
  AudioLinesIcon,
  CheckIcon,
  CircleAlertIcon,
  HistoryIcon,
  MicIcon,
  MicOffIcon,
  type LucideIcon,
} from "lucide-react";
import type { TranscriptStatus } from "@/bindings";
import type { BadgeProps } from "@/components/ui";

const NO_AUDIO_PLACEHOLDER = "The audio couldn't be kept, so this take can't be retried.";

export type BadgeVariant = NonNullable<BadgeProps["variant"]>;

export interface TranscriptStatusLook {
  readonly label: string;
  readonly variant: BadgeVariant;
  readonly icon: LucideIcon;
  /** What the row says instead of a preview when the take has no text. */
  readonly placeholder: string;
  /** Said instead of `placeholder` when the take's audio is gone too, for statuses whose placeholder promises it. */
  readonly noAudioPlaceholder?: string;
}

export const TRANSCRIPT_STATUS_LOOK: Readonly<Record<TranscriptStatus, TranscriptStatusLook>> = {
  recording: {
    label: "Recording",
    variant: "record",
    icon: MicIcon,
    placeholder: "Recording now.",
  },
  transcribing: {
    label: "Transcribing",
    variant: "accent",
    icon: AudioLinesIcon,
    placeholder: "Turning the audio into text.",
  },
  done: {
    label: "Done",
    variant: "success",
    icon: CheckIcon,
    placeholder: "No text.",
  },
  empty: {
    label: "No speech",
    variant: "neutral",
    icon: MicOffIcon,
    placeholder: "No speech was detected.",
  },
  failed: {
    label: "Failed",
    variant: "warning",
    icon: CircleAlertIcon,
    placeholder: "The audio is saved. Retry to transcribe it.",
    noAudioPlaceholder: NO_AUDIO_PLACEHOLDER,
  },
  recoverable: {
    label: "Recovered",
    variant: "warning",
    icon: HistoryIcon,
    placeholder: "Recovered after Echo closed. Retry to transcribe it.",
    noAudioPlaceholder: NO_AUDIO_PLACEHOLDER,
  },
};

/**
 * SOURCE OF TRUTH KEYWORDS: transcriptPlaceholder, row placeholder, no audio placeholder, take without text
 * WHAT:  What a row says instead of a preview: its status's placeholder, or the no-audio variant when the take's
 *        audio is gone (startup recovery could not read it, 02 §7.3).
 * WHY:   A failed or recovered take normally keeps its audio for a retry, and its copy says so; one whose audio is gone
 *        must not promise a Retry the row cannot offer (take-availability hides it).
 * WHERE: TranscriptRow (this folder).
 */
export function transcriptPlaceholder(take: { readonly status: TranscriptStatus; readonly has_audio: boolean }): string {
  const look = TRANSCRIPT_STATUS_LOOK[take.status];
  return take.has_audio ? look.placeholder : (look.noAudioPlaceholder ?? look.placeholder);
}
