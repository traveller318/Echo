/*!
 * SOURCE OF TRUTH KEYWORDS: accelerator picker tests, auto benchmark test, background GPU measurement test, remembered accelerator test, GPU fallback test, driver update remeasure test
 * WHAT:  AcceleratorPicker against FakeAsrEngine and FakeGraphicsAdapters: fixed choices, no GPU, the CPU half of
 *        auto (`Measuring`), the background GPU half winning and losing, the answer remembered per GPU and driver, a
 *        GPU that cannot start or run, a model error during measurement, a superseded load and the CPU reload after a
 *        lost GPU.
 * WHY:   05 A6 is only trustworthy if every branch is proven without a graphics card; the fake's per-accelerator
 *        latency makes the measurement's outcome deterministic (0 ms against 60 ms).
 * WHERE: `cargo test`.
 */

use std::{path::Path, sync::Arc, time::Duration};

use super::AcceleratorPicker;
use crate::{
    ports::{
        AsrEngine,
        fakes::{FakeAsrEngine, FakeGraphicsAdapters},
    },
    services::{Db, accelerator_benchmarks},
    types::{
        Accelerator, AcceleratorChoice, AcceleratorPolicy, AcceleratorReason, AcceleratorRequest,
        AcceleratorTiming, AppError, AsrCaps, BringUpOutcome, ComputeDevice, EngineId, Language,
        ModelId, PortError, PortResult, StaticList,
    },
};

const ENGINE: EngineId = EngineId::from_static("engine-a");
const POLICY: AcceleratorPolicy = AcceleratorPolicy {
    benchmark_audio_ms: 1_000,
    benchmark_runs: 1,
    gpu_margin_percent: 10,
};
/// Slow enough that scheduling noise never flips a comparison against 0 ms.
const SLOW: Duration = Duration::from_millis(60);

struct Rig {
    picker: AcceleratorPicker,
    gpus: Arc<FakeGraphicsAdapters>,
    db: Db,
    engine: FakeAsrEngine,
}

impl Rig {
    fn new(gpus: FakeGraphicsAdapters) -> Self {
        let gpus = Arc::new(gpus);
        let db = Db::open_in_memory().unwrap();
        Self {
            picker: AcceleratorPicker::new(Arc::clone(&gpus) as _, db.clone(), POLICY),
            gpus,
            db,
            engine: FakeAsrEngine::english(),
        }
    }

    fn with_gpu(driver: &str) -> Self {
        Self::new(FakeGraphicsAdapters::with(vec![
            FakeGraphicsAdapters::integrated(driver),
        ]))
    }

    fn bring_up(&self, request: AcceleratorRequest) -> PortResult<BringUpOutcome> {
        self.picker.bring_up(
            &ENGINE,
            &self.engine,
            Path::new("models/fake"),
            request,
            &|| true,
        )
    }

    /// A bring-up that must install the instance.
    fn loaded(&self, request: AcceleratorRequest) -> AcceleratorChoice {
        match self.bring_up(request).unwrap() {
            BringUpOutcome::Loaded(choice) => choice,
            BringUpOutcome::KeepCurrent(choice) => panic!("kept the current engine: {choice:?}"),
        }
    }

    /// The CPU half of a new auto measurement; returns the CPU timing the GPU half compares against.
    fn start_measuring(&self) -> AcceleratorTiming {
        self.script_runs(1);
        let choice = self.loaded(AcceleratorRequest::Auto);
        assert_eq!(
            (choice.accelerator, choice.reason),
            (Accelerator::Cpu, AcceleratorReason::Measuring)
        );
        *choice.benchmark.unwrap().timing(Accelerator::Cpu).unwrap()
    }

    /// Scripts the silent benchmark runs `count` inferences will make.
    fn script_runs(&self, count: usize) {
        for _ in 0..count {
            self.engine.push_text("");
        }
    }

    /// The accelerators `load` was asked for, in order.
    fn requested(&self) -> Vec<Accelerator> {
        self.engine
            .requested()
            .iter()
            .map(ComputeDevice::accelerator)
            .collect()
    }

    fn loaded_on(&self) -> Option<Accelerator> {
        self.engine.loaded().map(|(_, accelerator)| accelerator)
    }

    fn remembered(&self, driver: &str) -> Option<crate::types::AcceleratorBenchmark> {
        let key = FakeGraphicsAdapters::integrated(driver).fingerprint();
        accelerator_benchmarks::get::one(&self.db, &ENGINE, &key).unwrap()
    }
}

#[test]
fn auto_without_a_gpu_runs_on_the_cpu_without_measuring() {
    let rig = Rig::new(FakeGraphicsAdapters::none());
    let choice = rig.loaded(AcceleratorRequest::Auto);
    assert_eq!(choice.accelerator, Accelerator::Cpu);
    assert_eq!(choice.reason, AcceleratorReason::NoGpu);
    assert_eq!((choice.gpu, choice.benchmark), (None, None));
    assert_eq!(rig.requested(), [Accelerator::Cpu]);
    assert_eq!(rig.engine.warm_ups(), 1);
    assert!(rig.engine.calls().is_empty(), "no benchmark runs");
}

#[test]
fn fixed_choices_load_once_and_say_why() {
    let rig = Rig::with_gpu("1.0");
    let cpu = rig.loaded(AcceleratorRequest::Fixed(Accelerator::Cpu));
    assert_eq!(
        (cpu.accelerator, cpu.reason),
        (Accelerator::Cpu, AcceleratorReason::Preference)
    );
    assert_eq!(rig.gpus.lists(), 0, "a CPU choice never looks for a GPU");
    let gpu = rig.loaded(AcceleratorRequest::Fixed(Accelerator::Gpu));
    assert_eq!(
        (gpu.accelerator, gpu.reason),
        (Accelerator::Gpu, AcceleratorReason::Preference)
    );
    assert_eq!(gpu.gpu, Some(FakeGraphicsAdapters::integrated("1.0")));
    assert_eq!(rig.requested(), [Accelerator::Cpu, Accelerator::Gpu]);

    let cpu_only = FakeAsrEngine::new(AsrCaps {
        languages: StaticList::from(vec![Language::from_static("en")]),
        auto_language: false,
        punctuation: true,
        casing: true,
        accelerators: StaticList::from(vec![Accelerator::Cpu]),
        max_segment_s: 20,
    });
    let only = rig
        .picker
        .bring_up(
            &ENGINE,
            &cpu_only,
            Path::new("models/fake"),
            AcceleratorRequest::Auto,
            &|| true,
        )
        .unwrap();
    assert_eq!(
        (only.choice().accelerator, only.choice().reason),
        (Accelerator::Cpu, AcceleratorReason::OnlyOption)
    );
}

#[test]
fn a_first_auto_load_serves_on_the_cpu_while_measuring() {
    let rig = Rig::with_gpu("1.0");
    rig.engine.set_latency(Accelerator::Cpu, SLOW);
    let cpu = rig.start_measuring();
    assert_eq!(
        rig.requested(),
        [Accelerator::Cpu],
        "the GPU is not touched before the first take"
    );
    assert_eq!(rig.loaded_on(), Some(Accelerator::Cpu));
    assert!(cpu.run_ms >= 50 && cpu.warm_up_ms >= 50, "{cpu:?}");
    assert_eq!(
        rig.remembered("1.0"),
        None,
        "nothing is remembered until the GPU was measured"
    );
}

#[test]
fn a_faster_gpu_is_installed_and_remembered() {
    let rig = Rig::with_gpu("1.0");
    rig.engine.set_latency(Accelerator::Cpu, SLOW);
    let cpu = rig.start_measuring();
    rig.script_runs(1);
    let choice = rig.loaded(AcceleratorRequest::MeasureGpu { cpu });
    assert_eq!(
        (choice.accelerator, choice.reason),
        (Accelerator::Gpu, AcceleratorReason::Measured)
    );
    assert_eq!(rig.loaded_on(), Some(Accelerator::Gpu));
    let benchmark = choice.benchmark.unwrap();
    assert_eq!(benchmark.chosen, Accelerator::Gpu);
    let gpu = benchmark.timing(Accelerator::Gpu).unwrap();
    assert!(gpu.run_ms < cpu.run_ms, "{gpu:?}");
    assert_eq!(benchmark.timing(Accelerator::Cpu), Some(&cpu));
    assert_eq!(rig.remembered("1.0"), Some(benchmark));
}

#[test]
fn a_slower_gpu_is_unloaded_and_the_cpu_engine_kept() {
    let rig = Rig::with_gpu("1.0");
    rig.engine.set_latency(Accelerator::Gpu, SLOW);
    let cpu = rig.start_measuring();
    rig.script_runs(1);
    let outcome = rig
        .bring_up(AcceleratorRequest::MeasureGpu { cpu })
        .unwrap();
    let BringUpOutcome::KeepCurrent(choice) = outcome else {
        panic!("the slower GPU was installed: {outcome:?}");
    };
    assert_eq!(
        (choice.accelerator, choice.reason),
        (Accelerator::Cpu, AcceleratorReason::Measured)
    );
    assert_eq!(rig.loaded_on(), None, "the measuring instance was unloaded");
    assert_eq!(
        (choice.bring_up.load_ms, choice.bring_up.warm_up_ms),
        (cpu.load_ms, cpu.warm_up_ms),
        "the kept engine's own bring-up"
    );
    let benchmark = choice.benchmark.unwrap();
    assert_eq!(
        (benchmark.chosen, benchmark.timings.len()),
        (Accelerator::Cpu, 2)
    );
}

#[test]
fn the_answer_is_reused_until_the_driver_changes() {
    let rig = Rig::with_gpu("1.0");
    rig.engine.set_latency(Accelerator::Gpu, SLOW);
    let cpu = rig.start_measuring();
    rig.script_runs(1);
    let measured = rig
        .bring_up(AcceleratorRequest::MeasureGpu { cpu })
        .unwrap();

    let again = rig.loaded(AcceleratorRequest::Auto);
    assert_eq!(
        (again.accelerator, again.reason),
        (Accelerator::Cpu, AcceleratorReason::Remembered)
    );
    assert_eq!(again.benchmark, measured.choice().benchmark);
    assert_eq!(rig.engine.calls().len(), 2, "remembering runs no benchmark");

    rig.gpus
        .replace(vec![FakeGraphicsAdapters::integrated("2.0")]);
    rig.start_measuring();
    assert_eq!(
        rig.engine.calls().len(),
        3,
        "a driver update measures again"
    );
}

#[test]
fn a_remembered_gpu_answer_loads_straight_onto_the_gpu() {
    let rig = Rig::with_gpu("1.0");
    rig.engine.set_latency(Accelerator::Cpu, SLOW);
    let cpu = rig.start_measuring();
    rig.script_runs(1);
    rig.loaded(AcceleratorRequest::MeasureGpu { cpu });
    let before = rig.requested().len();
    let again = rig.loaded(AcceleratorRequest::Auto);
    assert_eq!(
        (again.accelerator, again.reason),
        (Accelerator::Gpu, AcceleratorReason::Remembered)
    );
    assert_eq!(rig.requested()[before..], [Accelerator::Gpu]);
}

#[test]
fn a_gpu_that_cannot_start_is_remembered_as_a_cpu_answer() {
    let rig = Rig::with_gpu("1.0");
    let cpu = rig.start_measuring();
    rig.engine.fall_back_to_cpu();
    let outcome = rig
        .bring_up(AcceleratorRequest::MeasureGpu { cpu })
        .unwrap();
    let BringUpOutcome::KeepCurrent(choice) = outcome else {
        panic!("a failed GPU was installed: {outcome:?}");
    };
    assert_eq!(
        (choice.accelerator, choice.reason),
        (Accelerator::Cpu, AcceleratorReason::GpuFailed)
    );
    let benchmark = rig.remembered("1.0").unwrap();
    assert_eq!(
        (benchmark.chosen, benchmark.timing(Accelerator::Gpu)),
        (Accelerator::Cpu, None)
    );
    assert_eq!(
        rig.loaded(AcceleratorRequest::Auto).reason,
        AcceleratorReason::Remembered,
        "a broken driver is not retried at every start"
    );
}

#[test]
fn a_gpu_that_fails_to_run_falls_back_to_the_cpu() {
    let rig = Rig::with_gpu("1.0");
    rig.engine.fail_inference_on(Some(Accelerator::Gpu));
    let fixed = rig.loaded(AcceleratorRequest::Fixed(Accelerator::Gpu));
    assert_eq!(
        (fixed.accelerator, fixed.reason),
        (Accelerator::Cpu, AcceleratorReason::GpuFailed)
    );
    assert_eq!(rig.loaded_on(), Some(Accelerator::Cpu));

    let cpu = rig.start_measuring();
    let measured = rig
        .bring_up(AcceleratorRequest::MeasureGpu { cpu })
        .unwrap();
    assert_eq!(measured.choice().reason, AcceleratorReason::GpuFailed);
    assert!(matches!(measured, BringUpOutcome::KeepCurrent(_)));
}

#[test]
fn wanting_the_gpu_without_one_or_without_a_list_uses_the_cpu() {
    let rig = Rig::new(FakeGraphicsAdapters::none());
    let fixed = rig.loaded(AcceleratorRequest::Fixed(Accelerator::Gpu));
    assert_eq!(
        (fixed.accelerator, fixed.reason),
        (Accelerator::Cpu, AcceleratorReason::NoGpu)
    );
    let broken = Rig::with_gpu("1.0");
    broken
        .gpus
        .fail_next(PortError::new(AppError::Internal).with_detail("DXGI failed"));
    assert_eq!(
        broken.loaded(AcceleratorRequest::Auto).reason,
        AcceleratorReason::NoGpu
    );
    let cpu = broken.start_measuring();
    broken.gpus.replace(Vec::new());
    let gone = broken
        .bring_up(AcceleratorRequest::MeasureGpu { cpu })
        .unwrap();
    assert_eq!(
        gone,
        BringUpOutcome::KeepCurrent(gone.choice().clone()),
        "a GPU that went away is simply not measured"
    );
    assert_eq!(gone.choice().reason, AcceleratorReason::NoGpu);
}

#[test]
fn a_model_error_is_returned_not_blamed_on_the_gpu() {
    let rig = Rig::with_gpu("1.0");
    let cpu = rig.start_measuring();
    let corrupt = AppError::ModelCorrupt {
        model_id: ModelId::from_static("model-a"),
    };
    rig.engine.fail_next_load(PortError::new(corrupt.clone()));
    let error = rig
        .bring_up(AcceleratorRequest::MeasureGpu { cpu })
        .err()
        .unwrap();
    assert_eq!(error.into_app_error(), corrupt);
    assert_eq!(
        rig.remembered("1.0"),
        None,
        "nothing is remembered from a failed measurement"
    );
}

#[test]
fn a_superseded_load_stops_with_busy() {
    let rig = Rig::with_gpu("1.0");
    let error = rig
        .picker
        .bring_up(
            &ENGINE,
            &rig.engine,
            Path::new("models/fake"),
            AcceleratorRequest::Auto,
            &|| false,
        )
        .err()
        .unwrap();
    assert_eq!(error.into_app_error(), AppError::Busy);
    assert!(rig.engine.calls().is_empty());
}

#[test]
fn after_a_lost_gpu_the_cpu_is_loaded_and_says_so() {
    let rig = Rig::with_gpu("1.0");
    let choice = rig.loaded(AcceleratorRequest::CpuAfterGpuLoss);
    assert_eq!(
        (choice.accelerator, choice.reason),
        (Accelerator::Cpu, AcceleratorReason::GpuLost)
    );
    assert_eq!(rig.gpus.lists(), 0);
    assert_eq!(rig.engine.caps().accelerators.len(), 2);
}
