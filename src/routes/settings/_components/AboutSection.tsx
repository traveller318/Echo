/**
 * SOURCE OF TRUTH KEYWORDS: AboutSection, Settings About, version, speech engine in use, accelerator in use, memory use, open logs folder, models and licenses, troubleshooting, detailed logging, measure again
 * WHAT:  The About card at the end of Settings: Echo's version, the speech engine and where it runs (and why), the
 *        memory Echo uses, "Open logs folder", every model and runtime with its license and credit, and a
 *        Troubleshooting disclosure (closed by default) holding the detailed-logging switch and, where a graphics card
 *        can run the engine, "Measure again".
 * WHY:   02 §12 and 05 A7/A15: the user can see what runs, what it costs and whose work it is, and the licenses that
 *        require attribution (Parakeet, CC-BY-4.0) are always shown. Everything comes from Rust (app_about,
 *        engine_status, models_list, the registry) and stays fresh from events; memory is read again whenever the
 *        window comes back to the front. The debug switch is a hidden registry setting (not in the list above) shown
 *        through SettingRowFor, so it saves and validates like every other row; it sits behind a disclosure because
 *        almost no one needs it. A part whose read fails shows an em dash instead of failing the whole page.
 * WHERE: routes/settings/index.tsx, after the registry sections.
 */
import { ChevronDownIcon, ChevronUpIcon } from "lucide-react";
import { useId, useState } from "react";
import { DEBUG_LOG_SETTING } from "@/bindings";
import { acceleratorLabel, FactList, SettingRowFor, type Fact } from "@/components/global";
import { Button } from "@/components/ui";
import {
  useAbout,
  useModels,
  useOpenLogsFolder,
  useRegistryView,
  useRemeasureAccelerator,
  useSettingsAvailability,
  useSpeechEngineStatus,
} from "@/hooks";
import { formatBytes, MISSING_VALUE } from "@/lib/format";
import { acceleratorReasonCopy, engineSummary, modelLicenses, versionLabel } from "./about-copy";
import { SettingsSection } from "./SettingsSection";

export function AboutSection() {
  const about = useAbout();
  const engine = useSpeechEngineStatus();
  const models = useModels();
  const { engines } = useRegistryView();
  const logs = useOpenLogsFolder();

  const choice = engine.data?.accelerator ?? null;
  const memory = about.data?.memory ?? null;
  const facts: Fact[] = [
    { label: "Version", value: about.data === undefined ? MISSING_VALUE : versionLabel(about.data.app), numeric: true },
    { label: "Speech engine", value: engine.data === undefined ? MISSING_VALUE : engineSummary(engine.data, engines) },
    ...(choice === null ? [] : [{ label: "Runs on", value: acceleratorLabel(choice.accelerator) }]),
    { label: "Memory in use", value: memory === null ? MISSING_VALUE : formatBytes(memory.working_set), numeric: true },
  ];
  const licenses = models.data === undefined ? [] : modelLicenses(models.data);

  return (
    <SettingsSection label="About">
      <div className="flex flex-col gap-4 py-3">
        <FactList facts={facts} />
        {choice === null ? null : (
          <p className="text-footnote text-fg-secondary">{acceleratorReasonCopy(choice.reason)}</p>
        )}
        <div>
          <Button variant="secondary" size="sm" disabled={logs.pending} onClick={logs.open}>
            Open logs folder
          </Button>
        </div>
      </div>
      {licenses.length === 0 ? null : (
        <div className="flex flex-col gap-2 py-3">
          <h3 className="text-callout text-fg">Models &amp; licenses</h3>
          <ul className="flex flex-col gap-2">
            {licenses.map((manifest) => (
              <li key={manifest.id} className="flex flex-col gap-1">
                <p className="text-callout text-fg">
                  {manifest.label} <span className="text-fg-secondary">· {manifest.license}</span>
                </p>
                {manifest.attribution === null ? null : (
                  <p className="text-caption text-fg-secondary">{manifest.attribution}</p>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}
      <Troubleshooting />
    </SettingsSection>
  );
}

/** The closed-by-default disclosure with the detailed-logging switch and "Measure again". */
function Troubleshooting() {
  const [open, setOpen] = useState(false);
  const panelId = useId();
  const availability = useSettingsAvailability();
  const remeasure = useRemeasureAccelerator();
  const canMeasure = availability.data?.caps.includes("gpu_accelerator") === true;
  return (
    <div className="flex flex-col py-3">
      <div>
        <Button
          variant="ghost"
          size="sm"
          aria-expanded={open}
          aria-controls={panelId}
          onClick={() => {
            setOpen((wasOpen) => !wasOpen);
          }}
        >
          Troubleshooting
          {open ? <ChevronUpIcon aria-hidden="true" /> : <ChevronDownIcon aria-hidden="true" />}
        </Button>
      </div>
      {open ? (
        <div id={panelId} className="flex flex-col divide-y divide-separator">
          <SettingRowFor settingKey={DEBUG_LOG_SETTING} />
          {canMeasure ? (
            <div className="flex items-center justify-between gap-6 py-3">
              <div className="flex min-w-0 flex-col gap-1">
                <p className="text-body text-fg">Measure processor speed again</p>
                <p className="text-footnote text-fg-secondary">
                  Echo checks again whether the graphics card or the processor runs speech faster on this PC.
                </p>
              </div>
              <Button variant="secondary" size="sm" disabled={remeasure.pending} onClick={remeasure.remeasure}>
                Measure again
              </Button>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
