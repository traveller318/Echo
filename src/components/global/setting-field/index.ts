/**
 * SOURCE OF TRUTH KEYWORDS: setting-field barrel, SettingField, SettingControlProps, useSettingForm, optionLabel
 * WHAT:  Public surface of the setting-field folder: the SettingField row, the contract and form hook every kind
 *        control uses, and how an option's label reads.
 * WHY:   Callers import `@/components/global`; the per-kind controls stay private to the folder.
 * WHERE: components/global/index.ts.
 */
export { optionLabel } from "./option-label";
export { useSettingForm, type SettingControlProps, type SettingForm } from "./setting-control";
export { SettingField, type SettingFieldProps } from "./SettingField";
