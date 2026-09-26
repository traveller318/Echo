/*!
 * SOURCE OF TRUTH KEYWORDS: LLM registry, QWEN3_POLISH profile, grammar system prompt, LLAMA_SERVER_POLICY, llama_server_setup, preambles, 35 percent, context size, GPU offload choice
 * WHAT:  The local LLM polish policy as data: the Qwen3 polish profile (the exact 02 §8.3 prompt, thinking off,
 *        greedy, 1.5 × budget, stop sequences, the 05 A12 safety limits), the llama-server sidecar policy (context,
 *        GPU use, start and restart timing), and `llama_server_setup`, which turns a model manifest into the
 *        LlamaServerSetup its adapter is built with (the runtime it requires, the file paths, the policies).
 * WHY:   A new local LLM is a manifest plus a profile here, never adapter code (00 constraint 4). GPU offload is
 *        off: on the dev box's Intel UHD (2026-09-26, b11146, Qwen3 1.7B Q4_K_M, on battery) the Vulkan build
 *        generated about 20 tokens/s against about 28 on the CPU (05 W44), and the take's 2 s budget leaves no room
 *        for the slower path; a discrete GPU would win, which is a one-line change here once Echo can tell them
 *        apart. The context is 2 048 tokens: within 2 s a CPU writes well under 100 tokens, so a longer text could
 *        not finish anyway, and every 1 024 tokens of context cost about 115 MB of memory for this model. The
 *        restart waits (1 s, then 2, 5, 10, 30 s) bring a killed sidecar back at once while a crash loop backs
 *        off, and three starts in a row that never become healthy stop it until grammar polish is switched on
 *        again.
 * WHERE: registry/engines.rs (the model polisher entry's build fn); tests below.
 */

use super::models::{self, LLAMA_SERVER_EXE};
use crate::types::{
    AppError, AppPaths, GpuOffload, LlamaServerSetup, LlmPolishProfile, LlmSafety, ModelId,
    ModelKind, PortError, PortResult, ResourceKind, SidecarFiles, SidecarPolicy, ThinkingControl,
};

/// How Qwen3 is asked to polish (02 §8.3 stage 6, 05 A12, A16).
pub const QWEN3_POLISH: LlmPolishProfile = LlmPolishProfile {
    system_prompt: "Fix grammar and punctuation. Keep meaning and wording. Output only the text.",
    thinking: ThinkingControl::Disable {
        marker: "/no_think",
        open: "<think>",
        close: "</think>",
    },
    temperature: 0.0,
    max_tokens_percent: 150,
    chars_per_token: 3,
    min_tokens: 16,
    stop: &["<|im_start|>", "<|im_end|>", "<|endoftext|>"],
    safety: LlmSafety {
        max_length_change_percent: 35,
        preambles: &[
            "Here is",
            "Here's",
            "Here are",
            "Sure",
            "Certainly",
            "Of course",
            "Okay, here",
            "The corrected",
            "Corrected text",
        ],
    },
};

/// How the llama-server sidecar runs (05 A13).
pub const LLAMA_SERVER_POLICY: SidecarPolicy = SidecarPolicy {
    context_tokens: 2_048,
    gpu_offload: GpuOffload::Off,
    start_timeout_ms: 120_000,
    health_poll_ms: 250,
    health_request_ms: 1_000,
    restart_delays_ms: &[1_000, 2_000, 5_000, 10_000, 30_000],
    stable_after_ms: 60_000,
    max_failed_starts: 3,
};

/**
 * SOURCE OF TRUTH KEYWORDS: llama_server_setup, LLM file paths, runtime requirement, single-file GGUF
 * WHAT:  The LlamaServerSetup for model `model_id` run with `profile`: its one GGUF file, the llama.cpp runtime
 *        it requires (`llama-server.exe` in that runtime's folder) and the app-local C++ runtime as a library folder.
 * WHY:   Paths are derived from AppPaths and the manifests, never spelled (05 W23), so the model manager's install
 *        folders and the adapter's launch agree by construction. A manifest of another shape (no runtime, several
 *        files) is a registry mistake: `Internal`, caught by the test below.
 * WHERE: registry/engines.rs build fn of the model polisher.
 */
pub fn llama_server_setup(
    paths: &AppPaths,
    model_id: &ModelId,
    profile: LlmPolishProfile,
) -> PortResult<LlamaServerSetup> {
    let model = models::find(model_id).ok_or_else(|| {
        PortError::new(AppError::NotFound {
            resource: ResourceKind::Model,
        })
        .with_detail(format!("no model is registered as `{model_id}`"))
    })?;
    let [file] = &*model.files else {
        return Err(PortError::new(AppError::Internal)
            .with_detail(format!("`{model_id}` must be a single-file GGUF model")));
    };
    let runtime = model
        .requires
        .iter()
        .filter_map(models::find)
        .find(|required| required.kind == ModelKind::Runtime)
        .ok_or_else(|| {
            PortError::new(AppError::Internal)
                .with_detail(format!("`{model_id}` names no runtime to run on"))
        })?;
    Ok(LlamaServerSetup {
        model_id: model.id.clone(),
        runtime_id: runtime.id.clone(),
        files: SidecarFiles {
            executable: paths
                .install_dir(runtime.kind, &runtime.id)
                .join(LLAMA_SERVER_EXE),
            model: paths
                .install_dir(model.kind, &model.id)
                .join(file.name.as_str()),
            library_dirs: vec![paths.cpp_runtime_dir()],
        },
        policy: LLAMA_SERVER_POLICY,
        profile,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::{LLAMA_CPP_VULKAN, PARAKEET_TDT_V3, QWEN3_1_7B_Q4};

    #[test]
    fn qwen_runs_on_the_llama_runtime_from_the_install_folders() {
        let paths = AppPaths::new("data", "resources");
        let setup = llama_server_setup(&paths, &QWEN3_1_7B_Q4, QWEN3_POLISH).unwrap();
        assert_eq!(setup.runtime_id, LLAMA_CPP_VULKAN);
        assert_eq!(
            setup.files.executable,
            paths
                .runtimes_dir()
                .join("llama-cpp-vulkan")
                .join("llama-server.exe")
        );
        assert_eq!(
            setup.files.model,
            paths
                .model_dir(&QWEN3_1_7B_Q4)
                .join("Qwen3-1.7B-Q4_K_M.gguf")
        );
        assert_eq!(setup.files.library_dirs, [paths.cpp_runtime_dir()]);
    }

    #[test]
    fn a_model_without_a_runtime_or_registration_is_refused() {
        let paths = AppPaths::new("data", "resources");
        let wrong_shape = llama_server_setup(&paths, &PARAKEET_TDT_V3, QWEN3_POLISH);
        assert_eq!(
            wrong_shape.err().map(PortError::into_app_error),
            Some(AppError::Internal)
        );
        let unknown = llama_server_setup(&paths, &ModelId::from_static("nope"), QWEN3_POLISH);
        assert_eq!(
            unknown.err().map(PortError::into_app_error),
            Some(AppError::NotFound {
                resource: ResourceKind::Model
            })
        );
    }

    #[test]
    fn the_profile_keeps_the_documented_limits() {
        // 02 §8.3 / 05 A12: the exact prompt, greedy, 1.5 × input, 35 %.
        assert_eq!(
            QWEN3_POLISH.system_prompt,
            "Fix grammar and punctuation. Keep meaning and wording. Output only the text."
        );
        assert!(QWEN3_POLISH.temperature.abs() < f32::EPSILON);
        assert_eq!(QWEN3_POLISH.max_tokens_percent, 150);
        assert_eq!(QWEN3_POLISH.safety.max_length_change_percent, 35);
        assert!(QWEN3_POLISH.safety.preambles.contains(&"Here is"));
        assert!(QWEN3_POLISH.safety.preambles.contains(&"Sure"));
        const {
            assert!(!LLAMA_SERVER_POLICY.restart_delays_ms.is_empty());
            assert!(LLAMA_SERVER_POLICY.max_failed_starts > 0);
        }
    }
}
