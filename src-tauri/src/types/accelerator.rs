/*!
 * SOURCE OF TRUTH KEYWORDS: GpuAdapter, ComputeDevice, AcceleratorRequest, AcceleratorChoice, AcceleratorReason, AcceleratorBenchmark, AcceleratorTiming, AcceleratorPolicy, BringUpOutcome, GPU fingerprint
 * WHAT:  Where inference runs and why: a DirectX 12 graphics adapter (GpuAdapter) and the device an engine is loaded
 *        on (ComputeDevice); what the pipeline asks for (AcceleratorRequest: auto or one accelerator); what a
 *        measurement found (AcceleratorBenchmark of AcceleratorTimings); the rules of that measurement
 *        (AcceleratorPolicy); the outcome the UI reads (AcceleratorChoice with its AcceleratorReason); and what one
 *        bring-up produced for the ASR worker (BringUpOutcome: install this instance, or keep the current one).
 * WHY:   02 §8.1 / 05 A6: `auto` measures the GPU against the CPU and keeps the faster, remembering the result per
 *        GPU and driver; a GPU that cannot start falls back to the CPU silently. The first measurement runs in two
 *        halves (CPU now, GPU in the background: `Measuring`, then `MeasureGpu`) because DirectML can take minutes
 *        to compile on an integrated GPU, and dictation must work from the first second. Those rules live in the pipeline,
 *        but the shapes they exchange with the port (GraphicsAdapters), the adapter (ComputeDevice), the database
 *        (AcceleratorBenchmark) and the UI (AcceleratorChoice) are types, so they live here (root CLAUDE.md §4).
 *        `Accelerator` itself stays with the caps in engine.rs because caps declare it. A GpuAdapter keeps its DXGI
 *        enumeration `ordinal` because that is the device index DirectML takes; the ordinal can change when adapters
 *        come and go, so identity for remembering a benchmark is the `fingerprint` (vendor, device, driver version)
 *        instead: a driver update measures again. Millisecond fields are u32 so they export as plain numbers
 *        (types/units.rs); a stage longer than 49 days saturates.
 * WHERE: GpuAdapter from ports/gpu.rs (GraphicsAdapters, adapters/gpu); ComputeDevice into `AsrEngine::load`
 *        (ports/asr.rs); AcceleratorRequest in AsrLoadRequest (pipeline/asr/plan.rs); the rest built by
 *        pipeline/asr/accelerator.rs, stored by services/accelerator_benchmarks and returned by `engine_status`
 *        (ipc/commands/engine.rs) inside SpeechEngineStatus.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{Accelerator, ByteCount, EngineId, UnixMs};

/// A hardware graphics adapter that can run DirectX 12 compute (what DirectML needs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct GpuAdapter {
    /// Position in the DXGI adapter enumeration: the device index DirectML opens. Not stable across hot-plug.
    pub ordinal: u32,
    /// The name the driver reports, e.g. `Intel(R) UHD Graphics`.
    pub name: String,
    /// PCI vendor id (0x10DE NVIDIA, 0x1002 AMD, 0x8086 Intel).
    pub vendor_id: u32,
    /// PCI device id.
    pub device_id: u32,
    /// The user-mode driver version, e.g. `32.0.101.7076`.
    pub driver_version: String,
    /// Memory on the card itself; 0 or small on integrated GPUs, which share system memory.
    pub dedicated_memory: ByteCount,
}

impl GpuAdapter {
    /// Identity for remembering a benchmark: the GPU model and its driver version, so a new GPU or a driver update
    /// is measured again while a changed enumeration order is not.
    pub fn fingerprint(&self) -> String {
        format!(
            "{:04x}:{:04x}:{}",
            self.vendor_id, self.device_id, self.driver_version
        )
    }
}

/// The device an engine is loaded on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComputeDevice {
    Cpu,
    /// A DirectX 12 GPU through DirectML.
    Gpu(GpuAdapter),
}

impl ComputeDevice {
    /// The accelerator class of this device, as caps declare it.
    pub const fn accelerator(&self) -> Accelerator {
        match self {
            Self::Cpu => Accelerator::Cpu,
            Self::Gpu(_) => Accelerator::Gpu,
        }
    }
}

/// Which accelerator a load asks for, after the stored preference was narrowed to the engine's caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceleratorRequest {
    /// The engine declares more than one accelerator and the user left it to Echo: measure, keep the faster.
    Auto,
    /// This accelerator (the user's choice, or the only one the engine declares).
    Fixed(Accelerator),
    /// The CPU, because the GPU the engine ran on stopped working during a take; lasts until the next load.
    CpuAfterGpuLoss,
    /// Auto's second half, on a second instance of the engine while the first serves takes on the CPU: time the
    /// GPU against the CPU timing `cpu` and keep it only if it wins.
    MeasureGpu { cpu: AcceleratorTiming },
}

/// How long one accelerator took on one engine, in ms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AcceleratorTiming {
    pub accelerator: Accelerator,
    /// Opening the model on it.
    pub load_ms: u32,
    /// The first inference (05 A8), which includes one-time graph and shader compilation.
    pub warm_up_ms: u32,
    /// The best steady-state inference on the benchmark audio: what a take feels.
    pub run_ms: u32,
}

/// One measurement of an engine on this machine: the timing of every accelerator that ran (a GPU that could not
/// start has none) and the accelerator it chose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AcceleratorBenchmark {
    pub chosen: Accelerator,
    pub timings: Vec<AcceleratorTiming>,
    pub measured_at: UnixMs,
}

impl AcceleratorBenchmark {
    /// The timing measured for `accelerator`, if it ran.
    pub fn timing(&self, accelerator: Accelerator) -> Option<&AcceleratorTiming> {
        self.timings
            .iter()
            .find(|timing| timing.accelerator == accelerator)
    }
}

/// Why the engine runs where it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AcceleratorReason {
    /// The engine declares one accelerator; there was nothing to choose.
    OnlyOption,
    /// The user picked this accelerator in Settings.
    Preference,
    /// The GPU was wanted but this PC has no DirectX 12 GPU.
    NoGpu,
    /// The GPU was wanted but could not start or run the model; the CPU took over (05 A6).
    GpuFailed,
    /// Automatic: runs on the CPU while the GPU is measured in the background (first start on this GPU and driver).
    Measuring,
    /// Automatic: both were measured just now and this one was faster.
    Measured,
    /// Automatic: the measurement made earlier for this GPU and driver was reused.
    Remembered,
    /// The GPU stopped working during a take (driver reset, device removed); the CPU took over for this run.
    GpuLost,
}

/// Loading and warming the engine on the accelerator it runs on now, in ms.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct BringUpTiming {
    pub load_ms: u32,
    pub warm_up_ms: u32,
}

/// Where the loaded speech engine runs, why, and what that was based on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AcceleratorChoice {
    pub engine_id: EngineId,
    /// The accelerator in use.
    pub accelerator: Accelerator,
    pub reason: AcceleratorReason,
    /// The GPU that was considered (in use when `accelerator` is `gpu`); None when none was looked for or found.
    pub gpu: Option<GpuAdapter>,
    /// This load's own timing.
    pub bring_up: BringUpTiming,
    /// The measurement `auto` decided from (made now or remembered); None for a fixed choice.
    pub benchmark: Option<AcceleratorBenchmark>,
}

/// What bringing an engine instance up produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BringUpOutcome {
    /// The instance is loaded and warm: install it.
    Loaded(AcceleratorChoice),
    /// The instance measured the GPU and lost (or the GPU failed) and was unloaded: the engine already serving takes
    /// stays, now described by this choice.
    KeepCurrent(AcceleratorChoice),
}

impl BringUpOutcome {
    pub const fn choice(&self) -> &AcceleratorChoice {
        match self {
            Self::Loaded(choice) | Self::KeepCurrent(choice) => choice,
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: AcceleratorPolicy, benchmark audio length, benchmark runs, GPU margin, auto accelerator rule
 * WHAT:  The rules of the `auto` measurement: how much silent audio one benchmark inference transcribes, how many
 *        steady-state runs are timed (the best counts) and how much faster the GPU must be to be kept.
 * WHY:   The first inference includes one-time compilation (DirectML compiles shaders, 05 A8), so the decision uses
 *        steady-state runs after the warm-up instead of the warm-up itself; the best of a few runs filters out a
 *        background hiccup. 5 s is a typical segment (VAD cuts at pauses, at most 20 s), where a GPU's fixed
 *        dispatch cost is not over-weighted as it would be on the 1 s warm-up. Silence costs the encoder exactly what
 *        speech does. A tie goes to the CPU: it has no driver to reset, keeps the GPU free for the user's apps and
 *        uses less power, so the GPU must win by `gpu_margin_percent`.
 * WHERE: pipeline/asr/accelerator.rs (AcceleratorPicker); the app uses DEFAULT, tests shorten it.
 */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceleratorPolicy {
    /// Silent audio one benchmark inference transcribes, in ms (capped at the engine's `max_segment_s`).
    pub benchmark_audio_ms: u32,
    /// Timed steady-state runs per accelerator; the fastest counts.
    pub benchmark_runs: u32,
    /// The GPU is kept only when its run is at least this much faster than the CPU's, in percent.
    pub gpu_margin_percent: u32,
}

impl AcceleratorPolicy {
    pub const DEFAULT: Self = Self {
        benchmark_audio_ms: 5_000,
        benchmark_runs: 2,
        gpu_margin_percent: 10,
    };

    /// The GPU's run beats the CPU's by the margin (a tie, even at 0 ms, keeps the CPU).
    pub const fn gpu_wins(&self, cpu_run_ms: u32, gpu_run_ms: u32) -> bool {
        let kept_percent = 100_u64.saturating_sub(self.gpu_margin_percent as u64);
        gpu_run_ms < cpu_run_ms && (gpu_run_ms as u64) * 100 <= (cpu_run_ms as u64) * kept_percent
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn gpu() -> GpuAdapter {
        GpuAdapter {
            ordinal: 0,
            name: String::from("Intel(R) UHD Graphics"),
            vendor_id: 0x8086,
            device_id: 0xa7a8,
            driver_version: String::from("32.0.101.7076"),
            dedicated_memory: ByteCount::new(128 * 1024 * 1024),
        }
    }

    #[test]
    fn the_fingerprint_is_the_model_and_driver_not_the_position() {
        let first = gpu();
        let moved = GpuAdapter {
            ordinal: 3,
            ..gpu()
        };
        assert_eq!(first.fingerprint(), "8086:a7a8:32.0.101.7076");
        assert_eq!(first.fingerprint(), moved.fingerprint());
        let updated = GpuAdapter {
            driver_version: String::from("32.0.101.8000"),
            ..gpu()
        };
        assert_ne!(first.fingerprint(), updated.fingerprint());
    }

    #[test]
    fn devices_name_their_accelerator() {
        assert_eq!(ComputeDevice::Cpu.accelerator(), Accelerator::Cpu);
        assert_eq!(ComputeDevice::Gpu(gpu()).accelerator(), Accelerator::Gpu);
    }

    #[test]
    fn the_gpu_must_win_by_the_margin() {
        let policy = AcceleratorPolicy::DEFAULT;
        assert!(policy.gpu_wins(100, 90));
        assert!(policy.gpu_wins(100, 50));
        assert!(
            !policy.gpu_wins(100, 91),
            "within the margin the CPU is kept"
        );
        assert!(!policy.gpu_wins(100, 100));
        assert!(!policy.gpu_wins(100, 400));
        assert!(!policy.gpu_wins(0, 0));
        assert!(policy.gpu_wins(u32::MAX, 0), "no overflow");
    }

    #[test]
    fn a_benchmark_finds_each_timing_and_exports_plain_fields() {
        let cpu = AcceleratorTiming {
            accelerator: Accelerator::Cpu,
            load_ms: 900,
            warm_up_ms: 300,
            run_ms: 180,
        };
        let benchmark = AcceleratorBenchmark {
            chosen: Accelerator::Cpu,
            timings: vec![cpu],
            measured_at: UnixMs::from_millis(1_000),
        };
        assert_eq!(benchmark.timing(Accelerator::Cpu), Some(&cpu));
        assert_eq!(benchmark.timing(Accelerator::Gpu), None);
        assert_eq!(
            serde_json::to_value(&benchmark).unwrap(),
            json!({
                "chosen": "cpu",
                "timings": [{ "accelerator": "cpu", "load_ms": 900, "warm_up_ms": 300, "run_ms": 180 }],
                "measured_at": 1000,
            })
        );
        assert_eq!(
            serde_json::to_value(AcceleratorReason::GpuFailed).unwrap(),
            json!({ "kind": "gpu_failed" })
        );
    }
}
