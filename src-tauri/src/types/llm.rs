/*!
 * SOURCE OF TRUTH KEYWORDS: LlmPolishProfile, ThinkingControl, LlmSafety, SidecarPolicy, SidecarFiles, LlamaServerSetup, GpuOffload, local LLM polish config, llama-server launch
 * WHAT:  What the registry hands the LLM polisher adapter: how the model is asked to polish and when its answer is
 *        not trusted (LlmPolishProfile with its ThinkingControl and LlmSafety), where the sidecar and its model are
 *        (SidecarFiles), and how the sidecar is started, health-checked and restarted (SidecarPolicy); all together
 *        in LlamaServerSetup.
 * WHY:   02 §8.3 stage 6 and 05 A12/A13/A16 are policy about a model, not code: a different LLM is a new profile
 *        and manifest in the registry, not a new adapter (00 constraint 4). Adapters may not read the registry
 *        (02 §3.2), so the shapes live here and the registry's build fn fills them from AppPaths and the manifests.
 *        None of these cross IPC: the UI sees the engine's caps and manifest only.
 * WHERE: Declared in registry/llm.rs; built into LlamaServerSetup by the registry's LLM build fn
 *        (registry/engines.rs); consumed by adapters/polish/llama_server.
 */

use std::path::PathBuf;

use super::ModelId;

/**
 * SOURCE OF TRUTH KEYWORDS: LlmPolishProfile, grammar prompt, temperature 0, max tokens ratio, stop sequences
 * WHAT:  The request a chat model gets for one take (system prompt, thinking switch, sampling, output budget, stop
 *        sequences) and the checks its answer must pass.
 * WHY:   05 A12: small models answer questions in the text or add preambles, so the prompt is strict, sampling is
 *        greedy and the answer is filtered. The output budget is 1.5 × the input's tokens; the input is not
 *        tokenized before sending (a round trip on the delivery path), so its tokens are estimated from characters
 *        with a divisor that errs high, and an answer that hits the budget is rejected as cut off.
 * WHERE: LlamaServerSetup; the adapter's request builder and safety filter.
 */
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LlmPolishProfile {
    /// Sent verbatim as the system message (02 §8.3).
    pub system_prompt: &'static str,
    pub thinking: ThinkingControl,
    /// Sampling temperature; 0 is greedy (05 A12).
    pub temperature: f32,
    /// Output tokens allowed, as a percentage of the input's estimated tokens (150 = 1.5 ×).
    pub max_tokens_percent: u16,
    /// Characters per token assumed when estimating the input (low, so the estimate errs high).
    pub chars_per_token: u8,
    /// Tokens always allowed on top of the estimate (very short inputs, the end-of-turn token).
    pub min_tokens: u16,
    /// Generation stops at any of these (a new chat turn starting).
    pub stop: &'static [&'static str],
    pub safety: LlmSafety,
}

/// How a model's reasoning mode is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingControl {
    /// The model does not reason before answering.
    NotApplicable,
    /// A hybrid model (Qwen3, 05 A16): `marker` (`/no_think`) ends the system prompt, the chat template is asked for
    /// `enable_thinking = false`, and any `open`…`close` block the model still writes is removed before the checks.
    Disable {
        marker: &'static str,
        open: &'static str,
        close: &'static str,
    },
}

/// When an answer is not used (05 A12); the rule output is kept instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlmSafety {
    /// The answer's length may differ from the input's by at most this percentage.
    pub max_length_change_percent: u8,
    /// Openings (compared without case) that mean the model talked to the user instead of rewriting the text.
    pub preambles: &'static [&'static str],
}

/// How many of the model's layers go to the GPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuOffload {
    /// None: the model runs on the CPU only.
    Off,
    /// As many as fit (the runtime decides; with no usable GPU it runs on the CPU).
    Auto,
}

/**
 * SOURCE OF TRUTH KEYWORDS: SidecarPolicy, llama-server start timeout, health poll, restart backoff, crash restart, context size
 * WHAT:  How the sidecar runs: context size and GPU offload, how long a start may take and how often /health is
 *        asked meanwhile, the waits before each restart after a crash, how long a run must last to reset those
 *        waits, and how many starts in a row may fail before it waits for the next explicit prepare.
 * WHY:   05 A13: started once when LLM polish is enabled, never per take; a crash restarts it after a growing wait
 *        so a model that cannot load does not spin the CPU, and a sidecar that ran fine for a while restarts at
 *        once again after its next crash (the "kill llama-server during a take" case).
 * WHERE: LlamaServerSetup; the adapter's supervisor.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidecarPolicy {
    /// `--ctx-size`: prompt + answer tokens of one request.
    pub context_tokens: u32,
    pub gpu_offload: GpuOffload,
    /// A start that is not healthy within this is killed and counts as a failed start.
    pub start_timeout_ms: u32,
    /// Interval between /health checks while starting.
    pub health_poll_ms: u32,
    /// Timeout of one /health request.
    pub health_request_ms: u32,
    /// Waits before each restart after a crash or failed start; the last repeats.
    pub restart_delays_ms: &'static [u32],
    /// A run healthy for this long resets the waits.
    pub stable_after_ms: u32,
    /// Failed starts in a row after which the supervisor stops until the next `prepare`.
    pub max_failed_starts: u8,
}

/// Where the sidecar and its model are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidecarFiles {
    /// `llama-server.exe` in the runtime's install folder.
    pub executable: PathBuf,
    /// The model file it loads.
    pub model: PathBuf,
    /// Folders put first on the child's `PATH` so the libraries it links resolve (the app-local C++ runtime).
    pub library_dirs: Vec<PathBuf>,
}

/// Everything the LLM polisher adapter is built with.
#[derive(Debug, Clone, PartialEq)]
pub struct LlamaServerSetup {
    /// The model manifest, named in `ModelMissing` when its file is absent.
    pub model_id: ModelId,
    /// The runtime manifest, named in `ModelMissing` when the executable is absent.
    pub runtime_id: ModelId,
    pub files: SidecarFiles,
    pub policy: SidecarPolicy,
    pub profile: LlmPolishProfile,
}
