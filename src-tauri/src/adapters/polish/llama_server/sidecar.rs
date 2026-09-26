/*!
 * SOURCE OF TRUTH KEYWORDS: Sidecar, llama-server supervisor, SidecarPhase, start sidecar, stop sidecar, health check, restart backoff, crash restart, warm-up request, startup log, API key
 * WHAT:  Sidecar: one llama-server process kept alive for the LLM polisher. `start` begins supervising (once),
 *        `settle` waits until it is ready or cannot run, `ready_port` is where a request goes, `stop` kills it.
 *        The supervisor task launches the child on a free loopback port inside its own kill-on-close Job Object,
 *        polls /health until the model is loaded, sends a one-token warm-up, marks it Ready, and after a crash
 *        restarts it after a growing wait, giving up after too many starts in a row that never became healthy.
 * WHY:   05 A13: started once when LLM polish is on (never per take), bound to 127.0.0.1 on a random port, killed
 *        with Echo however Echo ends (the Job Object), checked before first use and restarted with backoff. Every
 *        start is numbered by an epoch: `stop` moves the epoch on and kills the running child, and a supervisor
 *        whose epoch is no longer current exits without touching anything, so a stop racing a restart can never
 *        leave a process behind or kill the next one (each launch has its own job, and the running one is
 *        registered under a lock that `stop` also takes). Files that are not installed are `Missing` (no retry
 *        loop; the next use or install tries again); a model that crashes the runtime every time is `GaveUp` until
 *        the next explicit prepare. The child gets a random API key (llama-server otherwise accepts any local
 *        caller, including web pages, since it allows every CORS origin), no console window, `--offline` (no
 *        network), the app-local C++ runtime on its PATH (05 A20), and no stdout; its stderr is drained always and
 *        kept only until the first health answer (a few lines of load diagnostics for the log; no request, and so
 *        no transcript text, has been sent by then).
 * WHERE: Owned by LlamaServerPolisher (mod.rs); `stop` on unload and drop.
 */

use std::{
    collections::VecDeque,
    ffi::OsString,
    io::{BufRead, BufReader},
    os::windows::process::CommandExt,
    process::{Child, ChildStderr, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use tokio::sync::watch;
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

use super::request::{CHAT_PATH, warm_up_body};
use crate::{
    adapters::{
        net::{LoopbackClient, free_port},
        win32::KillOnCloseJob,
    },
    types::{AppError, GpuOffload, LlamaServerSetup, PortError, ThinkingControl},
};

/// The health endpoint (200 once the model is loaded, 503 while loading).
const HEALTH_PATH: &str = "/health";

/// Startup lines kept for the log when a start fails.
const STARTUP_LOG_LINES: usize = 12;

/// How long a killed child may take to be reported gone.
const EXIT_WAIT: Duration = Duration::from_secs(5);

/// Exits remembered, newest last (a late report from an old launch must not hide the current one's).
const EXITS_KEPT: usize = 8;

/// What the sidecar is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SidecarPhase {
    /// Not running and not asked to (never started, or stopped).
    Idle,
    /// The process is starting and loading the model.
    Starting,
    /// Healthy and warm; requests go to this port.
    Ready { port: u16 },
    /// It stopped unexpectedly and starts again after a wait.
    Restarting,
    /// The runtime or model is not installed (`ModelMissing`); the next use checks again.
    Missing(AppError),
    /// It failed to start too many times in a row; only an explicit prepare tries again.
    GaveUp(AppError),
}

/// A child's exit, reported by its waiter thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Exit {
    launch: u64,
    code: Option<i32>,
}

#[derive(Debug, Clone)]
struct Status {
    epoch: u64,
    phase: SidecarPhase,
    exits: Vec<Exit>,
}

impl Status {
    fn exit_of(&self, launch: u64) -> Option<Exit> {
        self.exits
            .iter()
            .rev()
            .find(|exit| exit.launch == launch)
            .copied()
    }
}

/// The launch running now and the job that holds it.
struct Running {
    launch: u64,
    job: Arc<KillOnCloseJob>,
}

/// How one supervised run ended.
enum RunEnd {
    /// `stop` (or a newer start) took over.
    Superseded,
    /// A file is not installed.
    Missing(AppError),
    /// The process ended or never became healthy.
    Ended {
        became_ready: bool,
        ran_for: Duration,
    },
}

/// One llama-server process and its supervisor.
pub(super) struct Sidecar {
    setup: LlamaServerSetup,
    http: LoopbackClient,
    /// The bearer key this sidecar accepts, random per sidecar.
    key: String,
    status: watch::Sender<Status>,
    launches: AtomicU64,
    running: Mutex<Option<Running>>,
}

impl Sidecar {
    pub fn new(setup: LlamaServerSetup, http: LoopbackClient) -> Self {
        let (status, _) = watch::channel(Status {
            epoch: 0,
            phase: SidecarPhase::Idle,
            exits: Vec::new(),
        });
        Self {
            setup,
            http,
            // 80 random bits; the key only has to be unguessable by other local programs for this run.
            key: format!("{:020x}", ulid::Ulid::generate().random()),
            status,
            launches: AtomicU64::new(0),
            running: Mutex::new(None),
        }
    }

    /// A sidecar that behaves as if it were ready on `port` with `key`, with no process (tests point it at a fake
    /// server).
    #[cfg(test)]
    pub fn attached(setup: LlamaServerSetup, http: LoopbackClient, port: u16, key: &str) -> Self {
        let sidecar = Self {
            key: key.to_owned(),
            ..Self::new(setup, http)
        };
        sidecar
            .status
            .send_modify(|status| status.phase = SidecarPhase::Ready { port });
        sidecar
    }

    pub fn setup(&self) -> &LlamaServerSetup {
        &self.setup
    }

    pub fn http(&self) -> &LoopbackClient {
        &self.http
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn phase(&self) -> SidecarPhase {
        self.status.borrow().phase.clone()
    }

    /// The port of the healthy sidecar, if it is ready now.
    pub fn ready_port(&self) -> Option<u16> {
        match self.status.borrow().phase {
            SidecarPhase::Ready { port } => Some(port),
            _ => None,
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: start sidecar, arm supervisor, restart after give up
     * WHAT:  Starts a supervisor unless one runs; a sidecar that gave up starts again only when `after_give_up`.
     * WHY:   Idempotent so every take may call it; only an explicit prepare (settings change, model installed)
     *        retries a runtime that crashed on every start, so a broken install does not reload 1.3 GB per take.
     * WHERE: LlamaServerPolisher::prepare (true) and ::polish when not ready (false).
     */
    pub fn start(self: &Arc<Self>, after_give_up: bool) {
        let mut started = None;
        self.status.send_if_modified(|status| {
            let restart = match status.phase {
                SidecarPhase::Idle | SidecarPhase::Missing(_) => true,
                SidecarPhase::GaveUp(_) => after_give_up,
                SidecarPhase::Starting | SidecarPhase::Ready { .. } | SidecarPhase::Restarting => {
                    false
                }
            };
            if restart {
                status.epoch += 1;
                status.phase = SidecarPhase::Starting;
                started = Some(status.epoch);
            }
            restart
        });
        let Some(epoch) = started else {
            return;
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(Arc::clone(self).supervise(epoch));
            }
            Err(error) => {
                tracing::error!(%error, "the grammar model cannot start outside the async runtime");
                self.set_phase(epoch, SidecarPhase::GaveUp(AppError::Internal));
            }
        }
    }

    /// Waits until the sidecar is ready, cannot run, or was stopped; returns that phase.
    pub async fn settle(&self) -> SidecarPhase {
        let mut status = self.status.subscribe();
        let settled = status
            .wait_for(|status| {
                matches!(
                    status.phase,
                    SidecarPhase::Idle
                        | SidecarPhase::Ready { .. }
                        | SidecarPhase::Missing(_)
                        | SidecarPhase::GaveUp(_)
                )
            })
            .await
            .map(|status| status.phase.clone());
        // The sender lives in self, so the channel cannot close while this runs.
        settled.unwrap_or(SidecarPhase::Idle)
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: stop sidecar, kill llama-server, unload grammar model
     * WHAT:  Ends the current epoch (its supervisor exits) and kills the running process now.
     * WHY:   The epoch moves first, then the running launch is taken under its lock: a supervisor that registers its
     *        launch afterwards sees the new epoch under the same lock and kills its own child (see the file header).
     * WHERE: LlamaServerPolisher::unload and its Drop.
     */
    pub fn stop(&self) {
        self.status.send_modify(|status| {
            status.epoch += 1;
            status.phase = SidecarPhase::Idle;
        });
        if let Some(running) = self.running.lock().take()
            && let Err(error) = running.job.terminate()
        {
            tracing::warn!(
                detail = error.detail(),
                "the grammar model could not be stopped"
            );
        }
    }

    /// `ModelMissing` for the runtime or model file that is not installed, or None when both are.
    pub fn missing(&self) -> Option<AppError> {
        let files = &self.setup.files;
        if !files.executable.is_file() {
            return Some(AppError::ModelMissing {
                model_id: self.setup.runtime_id.clone(),
            });
        }
        if !files.model.is_file() {
            return Some(AppError::ModelMissing {
                model_id: self.setup.model_id.clone(),
            });
        }
        None
    }

    fn is_current(&self, epoch: u64) -> bool {
        self.status.borrow().epoch == epoch
    }

    fn set_phase(&self, epoch: u64, phase: SidecarPhase) {
        self.status.send_if_modified(|status| {
            let current = status.epoch == epoch;
            if current {
                status.phase = phase;
            }
            current
        });
    }

    /// Waits up to `limit` for this epoch to end or `launch` to exit; true when the epoch ended.
    async fn wait_event(&self, epoch: u64, launch: Option<u64>, limit: Duration) -> bool {
        let mut status = self.status.subscribe();
        let _ = tokio::time::timeout(
            limit,
            status.wait_for(|status| {
                status.epoch != epoch
                    || launch.is_some_and(|launch| status.exit_of(launch).is_some())
            }),
        )
        .await;
        !self.is_current(epoch)
    }

    fn exit_of(&self, launch: u64) -> Option<Exit> {
        self.status.borrow().exit_of(launch)
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: supervise, restart delays, failed starts, give up, stable run resets backoff
     * WHAT:  Runs the sidecar until its epoch ends: each run ends in a restart after the next wait of the policy,
     *        unless files are missing (stop) or too many starts in a row failed (give up).
     * WHY:   A run that stayed healthy for `stable_after_ms` resets the waits, so a sidecar killed after a long
     *        healthy run (Task Manager, a driver reset) is back within a second.
     * WHERE: Spawned by `start`.
     */
    async fn supervise(self: Arc<Self>, epoch: u64) {
        let policy = self.setup.policy;
        let mut failed_starts = 0_u8;
        let mut waits = 0_usize;
        loop {
            match self.run_once(epoch).await {
                RunEnd::Superseded => return,
                RunEnd::Missing(error) => {
                    tracing::info!(
                        code = error.code().as_str(),
                        "the grammar model is not installed"
                    );
                    self.set_phase(epoch, SidecarPhase::Missing(error));
                    return;
                }
                RunEnd::Ended {
                    became_ready,
                    ran_for,
                } => {
                    if became_ready
                        && ran_for >= Duration::from_millis(u64::from(policy.stable_after_ms))
                    {
                        failed_starts = 0;
                        waits = 0;
                    }
                    if !became_ready {
                        failed_starts = failed_starts.saturating_add(1);
                    }
                    if failed_starts >= policy.max_failed_starts {
                        tracing::error!(
                            failed_starts,
                            "the grammar model failed to start repeatedly; grammar polish is off until it is enabled again"
                        );
                        self.set_phase(epoch, SidecarPhase::GaveUp(AppError::Polish));
                        return;
                    }
                    let delays = policy.restart_delays_ms;
                    let delay_ms = delays
                        .get(waits)
                        .or_else(|| delays.last())
                        .copied()
                        .unwrap_or(0);
                    waits = waits.saturating_add(1);
                    tracing::warn!(
                        delay_ms,
                        "the grammar model stopped; restarting after a wait"
                    );
                    self.set_phase(epoch, SidecarPhase::Restarting);
                    if self
                        .wait_event(epoch, None, Duration::from_millis(u64::from(delay_ms)))
                        .await
                    {
                        return;
                    }
                }
            }
        }
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: run_once, launch llama-server, health poll, warm-up, wait for exit
     * WHAT:  One launch: check the files, start the child in a fresh job, wait for /health (or its exit, the start
     *        timeout, or the epoch ending), warm it up, mark it Ready, then wait until it exits or is stopped.
     * WHY:   See the file header; a start that fails is killed and waited for, so the next launch never meets it.
     * WHERE: `supervise`.
     */
    async fn run_once(&self, epoch: u64) -> RunEnd {
        if let Some(missing) = self.missing() {
            return RunEnd::Missing(missing);
        }
        let started = Instant::now();
        let failed_start = RunEnd::Ended {
            became_ready: false,
            ran_for: Duration::ZERO,
        };
        let launch = self.launches.fetch_add(1, Ordering::Relaxed) + 1;
        let (port, job, mut child) = match self.launch() {
            Ok(launched) => launched,
            Err(error) => {
                tracing::warn!(
                    code = error.error().code().as_str(),
                    detail = error.detail(),
                    "the grammar model could not be started"
                );
                return failed_start;
            }
        };
        {
            let mut running = self.running.lock();
            if !self.is_current(epoch) {
                drop(running);
                let _ = job.terminate();
                let _ = child.wait();
                return RunEnd::Superseded;
            }
            *running = Some(Running {
                launch,
                job: Arc::clone(&job),
            });
        }
        let startup = StartupLog::drain(child.stderr.take());
        self.watch_exit(child, launch);
        self.set_phase(epoch, SidecarPhase::Starting);

        let healthy = self.wait_healthy(epoch, launch, port, started).await;
        startup.stop_keeping();
        if !healthy {
            if !self.is_current(epoch) {
                return RunEnd::Superseded;
            }
            let _ = job.terminate();
            let _ = self.wait_event(epoch, Some(launch), EXIT_WAIT).await;
            tracing::warn!(
                exit_code = self.exit_of(launch).and_then(|exit| exit.code),
                startup_log = %startup.lines(),
                "the grammar model did not become ready"
            );
            self.forget(launch);
            return failed_start;
        }
        self.warm_up(port).await;
        self.set_phase(epoch, SidecarPhase::Ready { port });
        let ready_at = Instant::now();
        tracing::info!(
            port,
            start_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "the grammar model is ready"
        );

        let mut status = self.status.subscribe();
        let _ = status
            .wait_for(|status| status.epoch != epoch || status.exit_of(launch).is_some())
            .await;
        if !self.is_current(epoch) {
            return RunEnd::Superseded;
        }
        tracing::warn!(
            exit_code = self.exit_of(launch).and_then(|exit| exit.code),
            ran_s = ready_at.elapsed().as_secs(),
            "the grammar model stopped unexpectedly"
        );
        self.forget(launch);
        RunEnd::Ended {
            became_ready: true,
            ran_for: ready_at.elapsed(),
        }
    }

    /// Starts the child on a free port inside a new kill-on-close job.
    fn launch(&self) -> Result<(u16, Arc<KillOnCloseJob>, Child), PortError> {
        let port = free_port()?;
        let job = Arc::new(KillOnCloseJob::new()?);
        let mut child = self.command(port)?.spawn().map_err(|error| {
            PortError::new(AppError::Polish)
                .with_detail(format!("llama-server could not be started: {error}"))
        })?;
        if let Err(error) = job.assign(&child) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok((port, job, child))
    }

    /**
     * SOURCE OF TRUTH KEYWORDS: llama-server command line, sidecar arguments, child PATH, no console window
     * WHAT:  The llama-server command for `port`: the model, loopback host and port, context size, one slot, no web
     *        UI, no network, no slot monitor, reasoning off for a model that thinks, GPU layers per the policy; the
     *        API key and PATH in its environment.
     * WHY:   See the file header; the key goes through the environment, not the command line other processes can
     *        read, and the working folder is the runtime's, so ggml finds its CPU and Vulkan backends beside it.
     * WHERE: `launch`.
     */
    fn command(&self, port: u16) -> Result<Command, PortError> {
        let files = &self.setup.files;
        let policy = &self.setup.policy;
        let folder = files.executable.parent().ok_or_else(|| {
            PortError::new(AppError::Internal).with_detail("llama-server has no folder")
        })?;
        let mut command = Command::new(&files.executable);
        command
            .arg("--model")
            .arg(&files.model)
            .args(["--host", "127.0.0.1", "--port"])
            .arg(port.to_string())
            .arg("--ctx-size")
            .arg(policy.context_tokens.to_string())
            .args(["--parallel", "1", "--no-webui", "--offline", "--no-slots"]);
        if matches!(self.setup.profile.thinking, ThinkingControl::Disable { .. }) {
            command.args(["--reasoning", "off"]);
        }
        if policy.gpu_offload == GpuOffload::Off {
            command.args(["--n-gpu-layers", "0"]);
        }
        command
            .current_dir(folder)
            .env("LLAMA_API_KEY", &self.key)
            .env("PATH", child_path(&files.library_dirs, folder)?)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW.0);
        Ok(command)
    }

    /// Reports the child's exit through the status channel from a thread that owns it.
    fn watch_exit(&self, mut child: Child, launch: u64) {
        let status = self.status.clone();
        let spawned = thread::Builder::new()
            .name("echo-llm-exit".to_owned())
            .spawn(move || {
                let code = child.wait().ok().and_then(|exit| exit.code());
                status.send_modify(|status| {
                    if status.exits.len() == EXITS_KEPT {
                        status.exits.remove(0);
                    }
                    status.exits.push(Exit { launch, code });
                });
            });
        if let Err(error) = spawned {
            // Without the watcher a crash goes unnoticed until a request fails; the next start replaces it.
            tracing::warn!(%error, "the grammar model's exit watcher could not start");
        }
    }

    /// Polls /health until it answers 200; false on exit, start timeout or the epoch ending.
    async fn wait_healthy(&self, epoch: u64, launch: u64, port: u16, started: Instant) -> bool {
        let policy = &self.setup.policy;
        let deadline = started + Duration::from_millis(u64::from(policy.start_timeout_ms));
        let request = Duration::from_millis(u64::from(policy.health_request_ms));
        let poll = Duration::from_millis(u64::from(policy.health_poll_ms));
        loop {
            if !self.is_current(epoch)
                || self.exit_of(launch).is_some()
                || Instant::now() >= deadline
            {
                return false;
            }
            if matches!(self.http.status(port, HEALTH_PATH, request).await, Ok(200)) {
                return true;
            }
            if self.wait_event(epoch, Some(launch), poll).await {
                return false;
            }
        }
    }

    /// Loads the system prompt into the prompt cache; a failure only costs the first take's speed.
    async fn warm_up(&self, port: u16) {
        let body = warm_up_body(&self.setup.profile);
        match self.http.post_json(port, CHAT_PATH, &self.key, body).await {
            Ok(response) if response.status == 200 => {}
            Ok(response) => tracing::warn!(
                status = response.status,
                "the grammar model's warm-up was refused"
            ),
            Err(error) => tracing::warn!(
                detail = error.detail(),
                "the grammar model's warm-up failed"
            ),
        }
    }

    /// Clears the running launch if it is still `launch`.
    fn forget(&self, launch: u64) {
        let mut running = self.running.lock();
        if running
            .as_ref()
            .is_some_and(|running| running.launch == launch)
        {
            *running = None;
        }
    }
}

/// The child's PATH: `library_dirs`, then its own folder, then Echo's PATH.
fn child_path(
    library_dirs: &[std::path::PathBuf],
    folder: &std::path::Path,
) -> Result<OsString, PortError> {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let dirs = library_dirs
        .iter()
        .cloned()
        .chain(std::iter::once(folder.to_path_buf()))
        .chain(std::env::split_paths(&inherited));
    std::env::join_paths(dirs).map_err(|error| {
        PortError::new(AppError::Internal)
            .with_detail(format!("the sidecar PATH is invalid: {error}"))
    })
}

/**
 * SOURCE OF TRUTH KEYWORDS: StartupLog, stderr drain, startup diagnostics, bounded log lines
 * WHAT:  Reads the child's stderr on its own thread until it closes, keeping the last lines only until
 *        `stop_keeping` (the first health answer).
 * WHY:   A pipe nobody reads fills and blocks the child; the kept lines explain a failed start in the log, and
 *        nothing is kept once requests (which carry transcript text) can reach the server (02 §12).
 * WHERE: Sidecar::run_once.
 */
struct StartupLog {
    lines: Arc<Mutex<VecDeque<String>>>,
    keeping: Arc<AtomicBool>,
}

impl StartupLog {
    fn drain(stderr: Option<ChildStderr>) -> Self {
        let log = Self {
            lines: Arc::default(),
            keeping: Arc::new(AtomicBool::new(true)),
        };
        if let Some(stderr) = stderr {
            let lines = Arc::clone(&log.lines);
            let keeping = Arc::clone(&log.keeping);
            let spawned = thread::Builder::new()
                .name("echo-llm-stderr".to_owned())
                .spawn(move || {
                    let mut reader = BufReader::new(stderr);
                    let mut line = Vec::new();
                    while matches!(reader.read_until(b'\n', &mut line), Ok(read) if read > 0) {
                        if keeping.load(Ordering::Relaxed) {
                            let mut kept = lines.lock();
                            if kept.len() == STARTUP_LOG_LINES {
                                kept.pop_front();
                            }
                            kept.push_back(String::from_utf8_lossy(&line).trim_end().to_owned());
                        }
                        line.clear();
                    }
                });
            if let Err(error) = spawned {
                tracing::warn!(%error, "the grammar model's log reader could not start");
            }
        }
        log
    }

    fn stop_keeping(&self) {
        self.keeping.store(false, Ordering::Relaxed);
    }

    fn lines(&self) -> String {
        self.lines
            .lock()
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ")
    }
}
