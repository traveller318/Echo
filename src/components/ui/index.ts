/**
 * SOURCE OF TRUTH KEYWORDS: components ui barrel, shadcn primitives, Button, Badge, Dialog, Input, Kbd, Select, Sheet, Slider, Switch, Toast, Tooltip
 * WHAT:  Barrel for the shadcn primitives restyled to Echo tokens (04 §6).
 * WHY:   One import path (`@/components/ui`) for every primitive; files inside can be reorganised without touching
 *        call sites. Primitives never import routes or feature code, only lib/, styles/ and GlassSurface.
 * WHERE: components/global, the app shell, routes and the pill.
 */
export { Badge, type BadgeProps } from "./Badge";
export { badgeVariants } from "./badge-variants";
export { Button, type ButtonProps } from "./Button";
export { buttonVariants } from "./button-variants";
export {
  Dialog,
  DialogClose,
  DialogContent,
  DialogCornerClose,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
  type DialogContentProps,
} from "./Dialog";
export { Input } from "./Input";
export { Kbd, KbdGroup } from "./Kbd";
export {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "./Select";
export {
  Sheet,
  SheetClose,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
  type SheetContentProps,
} from "./Sheet";
export { Slider } from "./Slider";
export { Switch } from "./Switch";
export {
  Toast,
  ToastAction,
  ToastClose,
  ToastDescription,
  ToastProvider,
  ToastTitle,
  ToastViewport,
  type ToastActionProps,
} from "./Toast";
export { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "./Tooltip";
