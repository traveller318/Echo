/**
 * SOURCE OF TRUTH KEYWORDS: optionLabel, enum option label, runtime option label, language option name, OptionSource label
 * WHAT:  The text a choice shows: the registry label, or for a runtime source whose options Rust labels with codes
 *        (languages), the name the UI's locale gives the code.
 * WHY:   The registry names languages by code because naming them is a locale concern (registry/settings/options.rs);
 *        an option Rust labelled itself (the "Auto-detect" choice) keeps that label. The table is keyed by the
 *        generated OptionSource union, so a new source fails tsc until it says how its options read, and nothing
 *        branches on a setting key.
 * WHERE: EnumControl.
 */
import type { EnumOption, EnumOptions, OptionSource } from "@/bindings";
import { formatLanguage } from "@/lib/format";

const registryLabel = (option: EnumOption) => option.label;

const SOURCE_LABEL: Readonly<Record<OptionSource, (option: EnumOption) => string>> = {
  asr_engines: registryLabel,
  model_polishers: registryLabel,
  asr_languages: (option) => (option.label === option.value ? formatLanguage(option.value) : option.label),
  asr_accelerators: registryLabel,
};

export function optionLabel(options: EnumOptions, option: EnumOption): string {
  return options.from === "fixed" ? option.label : SOURCE_LABEL[options.source](option);
}
