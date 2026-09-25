/**
 * SOURCE OF TRUTH KEYWORDS: TakeActionButton, icon action with tooltip, row action button, busy action
 * WHAT:  One History action as an icon-only ghost button with its name as the tooltip and accessible label;
 *        `busy` disables it and says so while its command runs.
 * WHY:   Icon-only buttons need a name on hover and focus (04 §6 Tooltip); the three row actions (Copy, Retry,
 *        Delete) share this so they look and behave alike. The tooltip content is the label, so the label is
 *        never written twice.
 * WHERE: HistoryRowActions (this folder).
 */
import type { LucideIcon } from "lucide-react";
import { Button, Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui";

export interface TakeActionButtonProps {
  readonly label: string;
  readonly icon: LucideIcon;
  readonly onClick: () => void;
  readonly disabled?: boolean;
  /** The action's command is running for this take. */
  readonly busy?: boolean;
  /** What the label says while `busy` (e.g. "Retrying"). */
  readonly busyLabel?: string;
}

export function TakeActionButton({
  label,
  icon: Icon,
  onClick,
  disabled = false,
  busy = false,
  busyLabel = label,
}: TakeActionButtonProps) {
  const name = busy ? busyLabel : label;
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={name}
          aria-busy={busy}
          disabled={disabled || busy}
          onClick={onClick}
        >
          <Icon aria-hidden="true" />
        </Button>
      </TooltipTrigger>
      <TooltipContent>{name}</TooltipContent>
    </Tooltip>
  );
}
