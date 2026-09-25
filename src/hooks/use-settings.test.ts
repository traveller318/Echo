/**
 * SOURCE OF TRUTH KEYWORDS: settings queries test, AUDIO_DEVICES_QUERY test, microphone hot-plug refresh test, invalidatedBy events test
 * WHAT:  Verifies the Settings reads refresh from the Rust events that make them stale: settings on SettingsChanged,
 *        the microphone list on AudioDevicesChanged, and that every named event exists in the generated bindings.
 * WHY:   The Settings page never polls (root CLAUDE.md §7); a query that names no event, or a misspelt one, would
 *        silently show a stale microphone list after a hot-plug.
 * WHERE: Runs in the `web` Vitest project against the real generated bindings (no Tauri call is made).
 */
import { describe, expect, it } from "vitest";
import { ECHO_EVENT_NAMES } from "@/lib/echo-events";
import { AUDIO_DEVICES_QUERY, SETTINGS_AVAILABILITY_QUERY, SETTINGS_QUERY } from "./use-settings";

describe("settings queries", () => {
  it("refresh the microphone list when Rust reports a hot-plug change", () => {
    expect(AUDIO_DEVICES_QUERY.invalidatedBy).toEqual(["audioDevicesChanged"]);
    expect(SETTINGS_QUERY.invalidatedBy).toEqual(["settingsChanged"]);
    expect(SETTINGS_AVAILABILITY_QUERY.invalidatedBy).toEqual(["settingsChanged"]);
  });

  it("name only events the bindings define", () => {
    for (const query of [SETTINGS_QUERY, SETTINGS_AVAILABILITY_QUERY, AUDIO_DEVICES_QUERY]) {
      for (const name of query.invalidatedBy ?? []) {
        expect(ECHO_EVENT_NAMES).toContain(name);
      }
    }
  });
});
