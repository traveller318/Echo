/*!
 * SOURCE OF TRUTH KEYWORDS: types layer, domain types, AppError, PortError, ids, caps structs, registry entry shapes, CommandSpec, specta types, IPC types
 * WHAT:  Layer 0: every shared Rust type (domain shapes, AppError, PortError, ids, caps structs, the data shapes
 *        the port traits exchange, registry entry shapes, event payloads, app paths), re-exported flat so callers write
 *        `use crate::types::{AppError, TranscriptId}`.
 * WHY:   Types are written only here (root CLAUDE.md §4). Anything crossing IPC derives specta and reaches the UI
 *        through the generated src/bindings.ts, so no TS type is ever hand-written. Imports no other layer.
 *        Conventions for every IPC type: snake_case field names and enum values, PascalCase AppError codes and
 *        event names (05 decision log).
 * WHERE: Imported by every other layer (02 §3.2 matrix).
 */

mod appearance;
mod asr;
mod audio;
mod clipboard;
mod command;
mod engine;
mod error;
mod events;
mod future;
mod hotkey;
mod ids;
mod launcher;
mod metrics;
mod model;
mod nav;
mod notification;
mod paths;
mod permission;
mod polish;
mod port_error;
mod power;
mod registry;
mod scheduling;
mod session;
mod settings;
mod static_data;
mod target;
#[cfg(test)]
pub mod testing;
mod transcript;
mod units;
mod update;

pub use appearance::{AppearanceView, Backdrop, ThemePreference, Transparency};
pub use asr::{AsrEvent, AsrLoadRequest, AsrLoaded, AsrOutput, AsrReadiness};
pub use audio::{
    AudioDevice, AudioTestLevelInput, CaptureEvent, CaptureFormat, CaptureSummary, MicCheck,
    MicVerdict, PIPELINE_SAMPLE_RATE_HZ, SegmentPolicy, SpeechSegment, VadEvent, samples_to_ms,
};
pub use clipboard::ClipboardHistory;
pub use command::{CommandSpec, Reentrancy};
pub use engine::{
    Accelerator, AppearanceCaps, AsrCaps, AudioCaps, EngineCaps, EngineKind, EngineSpec,
    HotkeyCaps, InserterCaps, Language, LanguageSupport, LatencyClass, PolisherCaps, UpdaterCaps,
    VadCaps,
};
pub use error::{AppError, AppErrorCode, HotkeyIssue, ResourceKind};
pub use events::{
    AppEvent, AppearanceChanged, AudioLevel, HistoryChangeReason, HistoryChanged, MetricsChanged,
    ModelProgress, SessionStateChanged, SettingsChanged, TranscriptSaved,
};
pub use future::BoxFuture;
pub use hotkey::{HotkeyEvent, HotkeyScope, HotkeySpec, KeyState, Shortcut};
pub use ids::{
    AudioDeviceId, EngineId, HotkeyId, InvalidId, MetricId, ModelId, SettingKey, TranscriptId,
};
pub use launcher::SettingsPage;
pub use metrics::{
    ActivityDay, LogMetricSpec, MetricAggregate, MetricEmphasis, MetricQuery, MetricSpec,
    MetricUnit, MetricValue, MetricsRange, MetricsSummary, TranscriptTotals,
};
pub use model::{ModelFile, ModelManifest, ModelPhase, ModelStatus, Sha256Hex};
pub use nav::{NavIcon, NavId, NavItem};
pub use notification::{Toast, ToastKind};
pub use paths::{AppPaths, ONNX_RUNTIME_LOAD_ORDER, OnnxRuntimeLibrary};
pub use permission::{Permission, PermissionState};
pub use polish::PolishContext;
pub use port_error::{PortError, PortResult};
pub use power::PowerEvent;
pub use registry::RegistryView;
pub use scheduling::WorkerPriority;
pub use session::{DeliveryOutcome, SessionStatus, SessionUiInput, SessionView};
pub use settings::{
    CapsRequirement, EnumOption, EnumOptions, OptionSource, SettingEntry, SettingKind,
    SettingSection, SettingSpec, SettingUnit, SettingValue, SettingsResetInput, SettingsSetInput,
    SettingsSnapshot, SharedSettings, TextPair,
};
pub use static_data::{StaticList, StaticStr};
pub use target::{AppTarget, ScreenRect, WindowHandle};
pub use transcript::{
    NewTranscript, Page, PageCursor, Transcript, TranscriptChange, TranscriptRef,
    TranscriptSelector, TranscriptStatus, TranscriptSummary,
};
pub use units::{ByteCount, UnixMs};
pub use update::UpdateStatus;
