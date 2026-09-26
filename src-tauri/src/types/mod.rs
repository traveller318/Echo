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

mod accelerator;
mod appearance;
mod asr;
mod audio;
mod calendar;
mod clipboard;
mod command;
mod delivery;
mod durability;
mod engine;
mod error;
mod events;
mod future;
mod hotkey;
mod ids;
mod launcher;
mod llm;
mod metrics;
mod model;
mod nav;
mod network;
mod notification;
mod onboarding;
mod overlay;
mod paths;
mod permission;
mod polish;
mod port_error;
mod power;
mod registry;
mod scheduling;
mod session;
mod session_machine;
mod settings;
mod sound;
mod static_data;
mod target;
#[cfg(test)]
pub mod testing;
mod transcript;
mod units;
mod update;

pub use accelerator::{
    AcceleratorBenchmark, AcceleratorChoice, AcceleratorPolicy, AcceleratorReason,
    AcceleratorRequest, AcceleratorTiming, BringUpOutcome, BringUpTiming, ComputeDevice,
    GpuAdapter,
};
pub use appearance::{AppearanceView, Backdrop, ThemePreference, Transparency};
pub use asr::{AsrEvent, AsrLoadRequest, AsrOutput, AsrReadiness, SpeechEngineStatus};
pub use audio::{
    AudioDevice, AudioTestLevelInput, AudioTransport, CaptureEvent, CaptureFormat, CaptureSummary,
    EndpointChange, MicCheck, MicVerdict, PIPELINE_SAMPLE_RATE_HZ, SegmentPolicy, SpeechSegment,
    VadEvent, samples_to_ms,
};
pub use calendar::{InvalidDate, LocalDate};
pub use clipboard::ClipboardHistory;
pub use command::{CommandSpec, Reentrancy};
pub use delivery::{ClipboardRestore, CopyReason, DeliveryPlan, DeliveryPolicy, DeliveryReport};
pub use durability::{RecoveryReport, RetentionPolicy, RetentionReport};
pub use engine::{
    Accelerator, AppearanceCaps, AsrCaps, AudioCaps, EngineCaps, EngineKind, EngineSpec,
    HotkeyCaps, InserterCaps, Language, LanguageSupport, LatencyClass, PolisherCaps, UpdaterCaps,
    VadCaps,
};
pub use error::{AppError, AppErrorCode, HotkeyIssue, ResourceKind};
pub use events::{
    AppEvent, AppearanceChanged, AudioDevicesChanged, AudioLevel, HistoryChangeReason,
    HistoryChanged, HotkeyRehearsed, MetricsChanged, ModelProgress, ModelsChanged,
    NavigationRequested, OnboardingRequested, SessionStateChanged, SettingsChanged,
    TranscriptSaved,
};
pub use future::BoxFuture;
pub use hotkey::{
    HotkeyAction, HotkeyBindFailure, HotkeyEvent, HotkeyScope, HotkeySpec, KeyState, RecordMode,
    Shortcut,
};
pub use ids::{
    AudioDeviceId, EngineId, HotkeyId, InvalidId, MetricId, ModelId, REGISTRY_ID_MAX_LEN,
    SettingKey, TranscriptId, is_registry_id,
};
pub use launcher::SettingsPage;
pub use llm::{
    GpuOffload, LlamaServerSetup, LlmPolishProfile, LlmSafety, SidecarFiles, SidecarPolicy,
    ThinkingControl,
};
pub use metrics::{
    ActivityDay, LogMetricSpec, MetricAggregate, MetricEmphasis, MetricQuery, MetricSpec,
    MetricUnit, MetricValue, MetricsActivityInput, MetricsRange, MetricsSummary,
    MetricsSummaryInput, TranscriptTotals,
};
pub use model::{
    EngineInput, EngineRuntime, EngineSelection, ModelEntry, ModelFile, ModelInput, ModelKind,
    ModelManifest, ModelPhase, ModelStatus, ModelTransferOutcome, ModelsView, Sha256Hex,
};
pub use nav::{NavIcon, NavId, NavItem, OpenPageInput};
pub use network::{AllowedHost, HostAllowlist, HttpPolicy};
pub use notification::{OneTimeNotice, Toast, ToastKind};
pub use onboarding::{
    OnboardingCondition, OnboardingNeeds, OnboardingStepId, OnboardingStepSpec, OnboardingView,
};
pub use overlay::{OverlayRect, PillHitAreas};
pub use paths::{AppPaths, ONNX_RUNTIME_LOAD_ORDER, OnnxRuntimeLibrary};
pub use permission::{Permission, PermissionCheckFn, PermissionGate, PermissionState};
pub use polish::{
    PolishContext, PolishFallback, PolishFallbackReason, PolishOutcome, PolishPlan, PolishPolicy,
};
pub use port_error::{PortError, PortResult};
pub use power::PowerEvent;
pub use registry::RegistryView;
pub use scheduling::WorkerPriority;
pub use session::{DeliveryOutcome, SessionRehearsal, SessionStatus, SessionUiInput, SessionView};
pub use session_machine::{
    ArmingTake, CancelPendingTake, DeliveringTake, FinalizingTake, IgnoreReason, IgnoredInput,
    RecordClock, RecordingTake, SessionCue, SessionEffect, SessionInput, SessionPhase,
    SessionPolicy, SessionState, SessionTimer, SettledTake, StopCause, TakeData, TimerToken,
};
pub use settings::{
    AdapterCaps, CapsRequirement, EnumOption, EnumOptions, OptionSource, SettingEntry, SettingKind,
    SettingOptions, SettingSection, SettingSectionSpec, SettingSpec, SettingUnit, SettingValue,
    SettingsAvailability, SettingsResetInput, SettingsSetInput, SettingsSnapshot, SharedSettings,
    TextPair,
};
pub use sound::{CueSound, SoundClip, Tone};
pub use static_data::{StaticList, StaticStr};
pub use target::{AppTarget, ScreenRect, WindowHandle};
pub use transcript::{
    HistoryListInput, NewTranscript, Page, PageCursor, Transcript, TranscriptChange,
    TranscriptInput, TranscriptRef, TranscriptSelector, TranscriptStatus, TranscriptSummary,
};
pub use units::{ByteCount, MonotonicMs, UnixMs};
pub use update::UpdateStatus;
