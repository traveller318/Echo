/*!
 * SOURCE OF TRUTH KEYWORDS: AcceleratorPicker, auto accelerator, GPU benchmark, DirectML auto-select, remembered choice per GPU and driver, CPU fallback, background GPU measurement, bring_up, accelerator-benchmark metric
 * WHAT:  AcceleratorPicker::bring_up loads and warms one speech engine instance on the accelerator its
 *        AcceleratorRequest resolves to and says what to do with it (BringUpOutcome). Auto with a DirectX 12 GPU
 *        reuses the answer remembered for that GPU and driver; with none remembered it runs in two halves: now the
 *        CPU is loaded, warmed and timed and the instance is installed (`Measuring`); then, on a second instance in
 *        the background (`MeasureGpu`), the GPU is loaded, warmed and timed, kept only when it beats the CPU by the
 *        policy's margin, and the answer is remembered.
 * WHY:   02 §8.1 / 05 A6: DirectML can be slower than int8 on the CPU (integrated GPUs) and some drivers fail, so
 *        `auto` decides by measurement on this machine, once per GPU and driver (services/accelerator_benchmarks).
 *        Compiling the int8 encoder for DirectML took minutes on the dev box's Intel UHD, so the GPU is never on the
 *        path to the first take: the CPU serves while the GPU is measured, exactly like an engine switch (a second
 *        instance warms up, then swaps; 02 §8.1), and a losing GPU instance is simply unloaded. The decision uses
 *        steady-state runs, not the warm-up, because the first inference includes one-time compilation a take never
 *        pays again (AcceleratorPolicy). Any GPU trouble (the adapter fell back while opening, a warm-up or run
 *        failed) is logged and answered with the CPU, never with a failed load, and is remembered, so a broken
 *        driver is not retried at every start; a model that fails on the CPU too is the real error (ModelMissing,
 *        ModelCorrupt). A GPU-list or database failure only costs the memory of the measurement. A newer load makes
 *        this one stop between steps (`still_wanted`). It branches only on the engine's declared caps.
 * WHERE: Built by app/bootstrap (DxgiGraphicsAdapters, the Db) into AsrWorkerConfig; called on the ASR loader thread
 *        by pipeline/asr/loader.rs; the worker starts the `MeasureGpu` half after installing a `Measuring` engine;
 *        timings go to the local log as the `accelerator-benchmark` metric.
 */

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use super::plan::effective_language;
use crate::{
    ports::{AsrEngine, GraphicsAdapters},
    registry::metrics::ACCELERATOR_BENCHMARK,
    services::{Db, accelerator_benchmarks},
    types::{
        Accelerator, AcceleratorBenchmark, AcceleratorChoice, AcceleratorPolicy, AcceleratorReason,
        AcceleratorRequest, AcceleratorTiming, AppError, AsrCaps, BringUpOutcome, BringUpTiming,
        ComputeDevice, EngineId, GpuAdapter, PIPELINE_SAMPLE_RATE_HZ, PortError, PortResult,
        UnixMs,
    },
};

/// Chooses, measures and remembers where speech engines run.
#[derive(Clone)]
pub struct AcceleratorPicker {
    gpus: Arc<dyn GraphicsAdapters>,
    db: Db,
    policy: AcceleratorPolicy,
}

/// One engine instance being brought up: what every step needs.
struct BringUp<'a> {
    engine_id: &'a EngineId,
    engine: &'a dyn AsrEngine,
    caps: AsrCaps,
    model_dir: &'a Path,
    policy: AcceleratorPolicy,
    still_wanted: &'a dyn Fn() -> bool,
}

impl AcceleratorPicker {
    pub fn new(gpus: Arc<dyn GraphicsAdapters>, db: Db, policy: AcceleratorPolicy) -> Self {
        Self { gpus, db, policy }
    }

    /// Test rigs: a machine without a GPU and its own in-memory database, so every load goes to the CPU.
    #[cfg(test)]
    pub fn without_gpu() -> Self {
        Self::with_gpus(Arc::new(crate::ports::fakes::FakeGraphicsAdapters::none()))
    }

    /// Test rigs: these GPUs, an in-memory database and a short benchmark.
    #[cfg(test)]
    pub fn with_gpus(gpus: Arc<dyn GraphicsAdapters>) -> Self {
        let db =
            Db::open_in_memory().unwrap_or_else(|error| panic!("in-memory database: {error:?}"));
        Self::new(
            gpus,
            db,
            AcceleratorPolicy {
                benchmark_audio_ms: 1_000,
                benchmark_runs: 1,
                ..AcceleratorPolicy::DEFAULT
            },
        )
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: bring_up, resolve accelerator request, load and warm engine, BringUpOutcome
     * WHAT:  Loads `engine` from `model_dir` on the device `request` resolves to, warms it, and returns whether to
     *        install it (with where it runs and why) or to keep the engine already serving; `still_wanted` turning
     *        false ends the work early with `Busy`.
     * WHY:   The one place an AcceleratorRequest becomes a device, so the fixed, auto, measuring and fallback paths
     *        agree on what counts as a GPU failure.
     * WHERE: pipeline/asr/loader.rs (`load_engine`) on the loader thread.
     */
    pub fn bring_up(
        &self,
        engine_id: &EngineId,
        engine: &dyn AsrEngine,
        model_dir: &Path,
        request: AcceleratorRequest,
        still_wanted: &dyn Fn() -> bool,
    ) -> PortResult<BringUpOutcome> {
        let job = BringUp {
            engine_id,
            engine,
            caps: engine.caps(),
            model_dir,
            policy: self.policy,
            still_wanted,
        };
        let cpu_reason = if job.caps.accelerators.len() > 1 {
            AcceleratorReason::Preference
        } else {
            AcceleratorReason::OnlyOption
        };
        let has_gpu_path = job.caps.supports_accelerator(Accelerator::Gpu);
        let has_cpu_path = job.caps.supports_accelerator(Accelerator::Cpu);
        let loaded = match request {
            AcceleratorRequest::MeasureGpu { cpu } => return self.measure_gpu(&job, cpu),
            AcceleratorRequest::CpuAfterGpuLoss => {
                job.on_cpu(AcceleratorReason::GpuLost, None, None)
            }
            AcceleratorRequest::Fixed(Accelerator::Cpu) => job.on_cpu(cpu_reason, None, None),
            AcceleratorRequest::Fixed(Accelerator::Gpu) if has_gpu_path => {
                match self.preferred_gpu() {
                    Some(gpu) => job.on_gpu(gpu, AcceleratorReason::Preference, None),
                    None => job.without_gpu(),
                }
            }
            AcceleratorRequest::Fixed(Accelerator::Gpu) => job.on_cpu(cpu_reason, None, None),
            AcceleratorRequest::Auto if !has_gpu_path => job.on_cpu(cpu_reason, None, None),
            AcceleratorRequest::Auto => match self.preferred_gpu() {
                None => job.without_gpu(),
                Some(gpu) if !has_cpu_path => job.on_gpu(gpu, AcceleratorReason::OnlyOption, None),
                Some(gpu) => self.auto(&job, gpu),
            },
        };
        loaded.map(BringUpOutcome::Loaded)
    }

    /// The most capable DirectX 12 GPU, or None (no GPU, or Windows could not list them).
    fn preferred_gpu(&self) -> Option<GpuAdapter> {
        match self.gpus.list() {
            Ok(gpus) => gpus.into_iter().next(),
            Err(error) => {
                tracing::warn!(
                    detail = error.detail(),
                    "graphics adapters could not be listed; using the CPU"
                );
                None
            }
        }
    }

    /// Auto with a GPU: the remembered answer for this GPU and driver, or the CPU half of a new measurement.
    fn auto(&self, job: &BringUp<'_>, gpu: GpuAdapter) -> PortResult<AcceleratorChoice> {
        let remembered =
            accelerator_benchmarks::get::one(&self.db, job.engine_id, &gpu.fingerprint())
                .unwrap_or_else(|error| {
                    tracing::warn!(
                        detail = error.detail(),
                        "remembered accelerator measurements could not be read; measuring again"
                    );
                    None
                });
        if let Some(benchmark) = remembered {
            tracing::info!(engine = %job.engine_id, chosen = ?benchmark.chosen, gpu = %gpu.name, "using the remembered accelerator");
            return match benchmark.chosen {
                Accelerator::Gpu => job.on_gpu(gpu, AcceleratorReason::Remembered, Some(benchmark)),
                Accelerator::Cpu => {
                    job.on_cpu(AcceleratorReason::Remembered, Some(gpu), Some(benchmark))
                }
            };
        }
        tracing::info!(engine = %job.engine_id, gpu = %gpu.name, driver = %gpu.driver_version, "measuring the CPU now and the GPU in the background");
        let cpu = job
            .time(&ComputeDevice::Cpu)?
            .ok_or_else(|| internal("the engine did not load on the CPU"))?;
        record(job.engine_id, &cpu);
        Ok(job.choice(
            Accelerator::Cpu,
            AcceleratorReason::Measuring,
            Some(gpu),
            bring_up_of(&cpu),
            Some(AcceleratorBenchmark {
                chosen: Accelerator::Cpu,
                timings: vec![cpu],
                measured_at: UnixMs::now(),
            }),
        ))
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: measure_gpu, background GPU benchmark, GPU against CPU, gpu_wins, remember answer
     * WHAT:  The second half of auto, on a second engine instance: times the GPU, picks by the policy against the CPU
     *        timing, remembers the answer; returns the instance to install when the GPU won, otherwise unloads it and
     *        returns the choice that now describes the CPU engine serving takes.
     * WHY:   A GPU failure is an answer ("the CPU"), not an error, and is remembered; only a model error is returned.
     * WHERE: AcceleratorPicker::bring_up for AcceleratorRequest::MeasureGpu.
     */
    fn measure_gpu(&self, job: &BringUp<'_>, cpu: AcceleratorTiming) -> PortResult<BringUpOutcome> {
        let Some(gpu) = self.preferred_gpu() else {
            // The GPU went away since the CPU half: nothing to measure, nothing learned.
            return Ok(BringUpOutcome::KeepCurrent(job.choice(
                Accelerator::Cpu,
                AcceleratorReason::NoGpu,
                None,
                bring_up_of(&cpu),
                None,
            )));
        };
        let on_gpu = match job.time(&ComputeDevice::Gpu(gpu.clone())) {
            Ok(timing) => timing,
            Err(error) if is_model_error(&error) => return Err(error),
            Err(error) => {
                tracing::warn!(
                    gpu = %gpu.name,
                    code = error.error().code().as_str(),
                    detail = error.detail(),
                    "the GPU could not run the speech engine; staying on the CPU"
                );
                None
            }
        };
        if let Some(timing) = &on_gpu {
            record(job.engine_id, timing);
        }
        let chosen = match on_gpu {
            Some(timing) if self.policy.gpu_wins(cpu.run_ms, timing.run_ms) => Accelerator::Gpu,
            _ => Accelerator::Cpu,
        };
        tracing::info!(engine = %job.engine_id, ?chosen, "accelerator chosen");
        let benchmark = AcceleratorBenchmark {
            chosen,
            timings: std::iter::once(cpu).chain(on_gpu).collect(),
            measured_at: UnixMs::now(),
        };
        job.proceed()?;
        if let Err(error) = accelerator_benchmarks::put::put(
            &self.db,
            job.engine_id,
            &gpu.fingerprint(),
            &benchmark,
        ) {
            tracing::warn!(
                detail = error.detail(),
                "the accelerator measurement could not be remembered; the next start measures again"
            );
        }
        match (chosen, on_gpu) {
            (Accelerator::Gpu, Some(timing)) => Ok(BringUpOutcome::Loaded(job.choice(
                Accelerator::Gpu,
                AcceleratorReason::Measured,
                Some(gpu),
                bring_up_of(&timing),
                Some(benchmark),
            ))),
            _ => {
                job.release();
                let reason = if on_gpu.is_some() {
                    AcceleratorReason::Measured
                } else {
                    AcceleratorReason::GpuFailed
                };
                Ok(BringUpOutcome::KeepCurrent(job.choice(
                    Accelerator::Cpu,
                    reason,
                    Some(gpu),
                    bring_up_of(&cpu),
                    Some(benchmark),
                )))
            }
        }
    }
}

impl BringUp<'_> {
    /// Fails with `Busy` once a newer load has replaced this one.
    fn proceed(&self) -> PortResult<()> {
        if (self.still_wanted)() {
            Ok(())
        } else {
            Err(PortError::new(AppError::Busy)
                .with_detail("the accelerator measurement was superseded by a newer load"))
        }
    }

    /// Frees this instance's session; a failure only leaves memory to the engine's drop. Timed, because tearing
    /// down a DirectML session can take tens of seconds (05 W43).
    fn release(&self) {
        let started = Instant::now();
        match self.engine.unload() {
            Ok(()) => tracing::info!(
                engine = %self.engine_id,
                unload_ms = millis(started.elapsed()),
                "the measuring instance was unloaded"
            ),
            Err(error) => tracing::warn!(
                detail = error.detail(),
                "a measured speech engine did not unload cleanly"
            ),
        }
    }

    /// Loads and warms on the CPU.
    fn on_cpu(
        &self,
        reason: AcceleratorReason,
        gpu: Option<GpuAdapter>,
        benchmark: Option<AcceleratorBenchmark>,
    ) -> PortResult<AcceleratorChoice> {
        let (accelerator, bring_up) = self.load_and_warm(&ComputeDevice::Cpu)?;
        Ok(self.choice(accelerator, reason, gpu, bring_up, benchmark))
    }

    /// The GPU was wanted but none exists: the CPU, or an error for an engine that has no CPU path.
    fn without_gpu(&self) -> PortResult<AcceleratorChoice> {
        if self.caps.supports_accelerator(Accelerator::Cpu) {
            self.on_cpu(AcceleratorReason::NoGpu, None, None)
        } else {
            Err(internal(
                "the engine needs a GPU and this PC has no DirectX 12 GPU",
            ))
        }
    }

    /// Loads and warms on `gpu`; any GPU trouble ends on the CPU with `GpuFailed`.
    fn on_gpu(
        &self,
        gpu: GpuAdapter,
        reason: AcceleratorReason,
        benchmark: Option<AcceleratorBenchmark>,
    ) -> PortResult<AcceleratorChoice> {
        match self.load_and_warm(&ComputeDevice::Gpu(gpu.clone())) {
            Ok((Accelerator::Gpu, bring_up)) => {
                Ok(self.choice(Accelerator::Gpu, reason, Some(gpu), bring_up, benchmark))
            }
            // The adapter already fell back while opening and logged why; the engine is warm on the CPU.
            Ok((Accelerator::Cpu, bring_up)) => Ok(self.choice(
                Accelerator::Cpu,
                AcceleratorReason::GpuFailed,
                Some(gpu),
                bring_up,
                benchmark,
            )),
            Err(error)
                if is_model_error(&error) || !self.caps.supports_accelerator(Accelerator::Cpu) =>
            {
                Err(error)
            }
            Err(error) => {
                tracing::warn!(
                    gpu = %gpu.name,
                    code = error.error().code().as_str(),
                    detail = error.detail(),
                    "the speech engine failed on the GPU; using the CPU"
                );
                self.proceed()?;
                self.on_cpu(AcceleratorReason::GpuFailed, Some(gpu), benchmark)
            }
        }
    }

    /// Loads on `device` and runs the warm-up; returns the accelerator in use and both timings.
    fn load_and_warm(&self, device: &ComputeDevice) -> PortResult<(Accelerator, BringUpTiming)> {
        let started = Instant::now();
        let accelerator = self.engine.load(self.model_dir, device)?;
        let load_ms = millis(started.elapsed());
        self.proceed()?;
        let started = Instant::now();
        self.engine.warm_up()?;
        Ok((
            accelerator,
            BringUpTiming {
                load_ms,
                warm_up_ms: millis(started.elapsed()),
            },
        ))
    }

    /// Loads, warms and times steady-state runs on `device`; None when a GPU request ended on the CPU.
    fn time(&self, device: &ComputeDevice) -> PortResult<Option<AcceleratorTiming>> {
        let (accelerator, bring_up) = self.load_and_warm(device)?;
        if accelerator != device.accelerator() {
            return Ok(None);
        }
        let limit_ms = self.caps.max_segment_s.saturating_mul(1000);
        let audio_ms = self.policy.benchmark_audio_ms.min(limit_ms);
        let samples =
            usize::try_from(u64::from(audio_ms) * u64::from(PIPELINE_SAMPLE_RATE_HZ) / 1000)
                .unwrap_or(0);
        let silence = vec![0.0_f32; samples];
        let language = effective_language(None, &self.caps);
        let mut best = u32::MAX;
        for _ in 0..self.policy.benchmark_runs.max(1) {
            self.proceed()?;
            let started = Instant::now();
            self.engine.transcribe(&silence, language.as_ref())?;
            best = best.min(millis(started.elapsed()));
        }
        Ok(Some(AcceleratorTiming {
            accelerator,
            load_ms: bring_up.load_ms,
            warm_up_ms: bring_up.warm_up_ms,
            run_ms: best,
        }))
    }

    fn choice(
        &self,
        accelerator: Accelerator,
        reason: AcceleratorReason,
        gpu: Option<GpuAdapter>,
        bring_up: BringUpTiming,
        benchmark: Option<AcceleratorBenchmark>,
    ) -> AcceleratorChoice {
        AcceleratorChoice {
            engine_id: self.engine_id.clone(),
            accelerator,
            reason,
            gpu,
            bring_up,
            benchmark,
        }
    }
}

/// One `accelerator-benchmark` log line (02 §12).
fn record(engine: &EngineId, timing: &AcceleratorTiming) {
    tracing::info!(
        metric = ACCELERATOR_BENCHMARK.id.as_str(),
        engine = %engine,
        accelerator = ?timing.accelerator,
        load_ms = timing.load_ms,
        warm_up_ms = timing.warm_up_ms,
        run_ms = timing.run_ms,
        "accelerator measured"
    );
}

const fn bring_up_of(timing: &AcceleratorTiming) -> BringUpTiming {
    BringUpTiming {
        load_ms: timing.load_ms,
        warm_up_ms: timing.warm_up_ms,
    }
}

/// Errors that belong to the model or the load itself, not to the device: never answered with a CPU retry.
fn is_model_error(error: &PortError) -> bool {
    matches!(
        error.error(),
        AppError::ModelMissing { .. } | AppError::ModelCorrupt { .. } | AppError::Busy
    )
}

fn internal(detail: &str) -> PortError {
    PortError::new(AppError::Internal).with_detail(detail.to_owned())
}

fn millis(elapsed: Duration) -> u32 {
    u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX)
}
