/*!
 * SOURCE OF TRUTH KEYWORDS: LlamaServerPolisher tests, fake llama-server, timeout fallback test, request cancel on timeout, missing model test, sidecar give up test
 * WHAT:  LlamaServerPolisher against a fake llama-server on 127.0.0.1 (an accepted answer, a rejected answer, an
 *        HTTP error, a server that never answers: the caller's timeout wins and the connection is closed so the
 *        server can cancel) and the sidecar supervisor against missing files and a runtime that exits at once.
 * WHY:   02 §8.3 promises that the LLM never blocks delivery; the chain enforces the 2 s budget (its own test uses a
 *        hanging fake stage), and this proves the real adapter gives up its request when that budget drops it. The
 *        supervisor's give-up path is exercised with a real short-lived process, so no model is needed.
 * WHERE: `cargo test` (adapters::polish::llama_server::tests).
 */

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

use super::*;
use crate::types::{
    GpuOffload, LlmPolishProfile, LlmSafety, ModelId, SidecarFiles, SidecarPolicy, StaticList,
    ThinkingControl, testing::TempDir,
};

const PROFILE: LlmPolishProfile = LlmPolishProfile {
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
    stop: &["<|im_start|>"],
    safety: LlmSafety {
        max_length_change_percent: 35,
        preambles: &["Here is", "Sure"],
    },
};

const POLICY: SidecarPolicy = SidecarPolicy {
    context_tokens: 2_048,
    gpu_offload: GpuOffload::Off,
    start_timeout_ms: 10_000,
    health_poll_ms: 50,
    health_request_ms: 500,
    restart_delays_ms: &[20],
    stable_after_ms: 60_000,
    max_failed_starts: 2,
};

const KEY: &str = "test-key";

fn setup(files: SidecarFiles) -> LlamaServerSetup {
    LlamaServerSetup {
        model_id: ModelId::from_static("test-llm"),
        runtime_id: ModelId::from_static("test-runtime"),
        files,
        policy: POLICY,
        profile: PROFILE,
    }
}

fn nowhere() -> SidecarFiles {
    SidecarFiles {
        executable: PathBuf::from("missing").join("llama-server.exe"),
        model: PathBuf::from("missing").join("model.gguf"),
        library_dirs: Vec::new(),
    }
}

fn context() -> PolishContext {
    PolishContext {
        language: None,
        punctuated: true,
        cased: true,
        remove_fillers: true,
        dictionary: StaticList::from(Vec::new()),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// How the fake server answers.
#[derive(Clone, Copy)]
enum Answer {
    /// 200 with a chat completion carrying this text and finish reason.
    Chat(&'static str, &'static str),
    /// This status with an empty body.
    Status(u16),
    /// Reads the request and never answers.
    Hang,
}

/// A one-connection-at-a-time fake llama-server; records each request and whether a hung client went away.
struct FakeServer {
    port: u16,
    requests: Arc<Mutex<Vec<String>>>,
    closed: mpsc::Receiver<()>,
}

impl FakeServer {
    fn start(answer: Answer) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (closed_tx, closed) = mpsc::channel();
        let seen = Arc::clone(&requests);
        thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let request = read_request(&mut stream);
                seen.lock().unwrap().push(request);
                match answer {
                    Answer::Chat(content, finish) => {
                        let body = serde_json::json!({
                            "choices": [{
                                "finish_reason": finish,
                                "message": { "role": "assistant", "content": content },
                            }],
                        })
                        .to_string();
                        respond(&mut stream, 200, &body);
                    }
                    Answer::Status(status) => respond(&mut stream, status, ""),
                    Answer::Hang => {
                        // Blocks until the client closes its end.
                        let mut rest = [0_u8; 64];
                        while matches!(stream.read(&mut rest), Ok(read) if read > 0) {}
                        let _ = closed_tx.send(());
                    }
                }
            }
        });
        Self {
            port,
            requests,
            closed,
        }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = stream.read(&mut buffer).unwrap_or(0);
        request.extend_from_slice(&buffer[..read]);
        let text = String::from_utf8_lossy(&request).to_string();
        if let Some(end) = text.find("\r\n\r\n") {
            let length = text
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|value| value.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            if request.len() >= end + 4 + length {
                return text;
            }
        }
        if read == 0 {
            return text;
        }
    }
}

fn respond(stream: &mut TcpStream, status: u16, body: &str) {
    let answer = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(answer.as_bytes());
}

const INPUT: &str = "so i was thinking we should of went to the store yesterday";

#[test]
fn polishes_through_the_server_with_the_key_and_the_safety_filter() {
    let server = FakeServer::start(Answer::Chat(
        "<think>\n\n</think>\n\nSo I was thinking we should have gone to the store yesterday.",
        "stop",
    ));
    let polisher = LlamaServerPolisher::attached(setup(nowhere()), server.port, KEY).unwrap();
    let polished = runtime().block_on(polisher.polish(INPUT, &context()));
    assert_eq!(
        polished.unwrap(),
        "So I was thinking we should have gone to the store yesterday."
    );
    let request = server.requests().remove(0);
    assert!(request.starts_with("POST /v1/chat/completions "));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer test-key")
    );
    assert!(request.contains("\"enable_thinking\":false"));
    assert!(request.contains(INPUT));
}

#[test]
fn an_answer_the_filter_rejects_or_an_http_error_is_a_polish_error() {
    let preamble = FakeServer::start(Answer::Chat(
        "Here is the corrected text: So I was thinking we should have gone.",
        "stop",
    ));
    let cut_off = FakeServer::start(Answer::Chat("So I was thinking we", "length"));
    let refused = FakeServer::start(Answer::Status(401));
    let runtime = runtime();
    for server in [&preamble, &cut_off, &refused] {
        let polisher = LlamaServerPolisher::attached(setup(nowhere()), server.port, KEY).unwrap();
        let error = runtime
            .block_on(polisher.polish(INPUT, &context()))
            .err()
            .unwrap();
        assert_eq!(error.error(), &AppError::Polish);
        assert!(error.detail().is_some());
    }
}

#[test]
fn a_text_too_long_for_the_context_is_refused_without_a_request() {
    let server = FakeServer::start(Answer::Status(500));
    let polisher = LlamaServerPolisher::attached(setup(nowhere()), server.port, KEY).unwrap();
    let long = "word ".repeat(2_000);
    let error = runtime()
        .block_on(polisher.polish(&long, &context()))
        .err()
        .unwrap();
    assert_eq!(error.error(), &AppError::Polish);
    assert!(server.requests().is_empty());
}

/// The chain gives a slow stage 2 s (PolishPolicy); when that budget drops the call, the request must end too, so
/// llama-server stops generating and the next take is not queued behind it.
#[test]
fn a_server_that_never_answers_loses_to_the_timeout_and_the_request_is_closed() {
    let server = FakeServer::start(Answer::Hang);
    let polisher = LlamaServerPolisher::attached(setup(nowhere()), server.port, KEY).unwrap();
    let runtime = runtime();
    let started = Instant::now();
    let context = context();
    let outcome = runtime.block_on(async {
        tokio::time::timeout(Duration::from_millis(300), polisher.polish(INPUT, &context)).await
    });
    assert!(outcome.is_err(), "the timeout must win");
    assert!(started.elapsed() < Duration::from_secs(2));
    // The connection is closed by the HTTP client's connection task, so the runtime keeps running meanwhile (in
    // the app it always does).
    let closed = runtime.block_on(async {
        for _ in 0..250 {
            if server.closed.try_recv().is_ok() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        false
    });
    assert!(closed, "the dropped request closes its connection");
}

#[test]
fn missing_files_are_model_missing_for_the_file_that_is_absent() {
    let folder = TempDir::new("llm-missing");
    let runtime = runtime();
    let absent = LlamaServerPolisher::new(setup(nowhere())).unwrap();
    let error = runtime.block_on(absent.prepare()).err().unwrap();
    assert_eq!(
        error.error(),
        &AppError::ModelMissing {
            model_id: ModelId::from_static("test-runtime")
        }
    );
    let executable = folder.path().join("llama-server.exe");
    std::fs::write(&executable, b"").unwrap();
    let no_model = LlamaServerPolisher::new(setup(SidecarFiles {
        executable,
        model: folder.path().join("model.gguf"),
        library_dirs: Vec::new(),
    }))
    .unwrap();
    let error = runtime.block_on(no_model.prepare()).err().unwrap();
    assert_eq!(
        error.error(),
        &AppError::ModelMissing {
            model_id: ModelId::from_static("test-llm")
        }
    );
    // A take while the model is missing falls back at once and reports why.
    let error = runtime
        .block_on(no_model.polish(INPUT, &context()))
        .err()
        .unwrap();
    assert_eq!(
        error.error(),
        &AppError::ModelMissing {
            model_id: ModelId::from_static("test-llm")
        }
    );
}

/// A Windows tool that rejects llama-server's arguments and exits at once, standing in for a runtime that crashes
/// on every start.
fn exits_at_once() -> PathBuf {
    let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap();
    system.join("System32").join("PING.EXE")
}

#[test]
fn a_runtime_that_crashes_on_every_start_is_retried_then_given_up() {
    let folder = TempDir::new("llm-crash");
    let model = folder.path().join("model.gguf");
    std::fs::write(&model, b"gguf").unwrap();
    let polisher = LlamaServerPolisher::new(setup(SidecarFiles {
        executable: exits_at_once(),
        model,
        library_dirs: vec![folder.path().to_path_buf()],
    }))
    .unwrap();
    let runtime = runtime();
    let started = Instant::now();
    let error = runtime.block_on(polisher.prepare()).err().unwrap();
    assert_eq!(error.error(), &AppError::Polish);
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(matches!(polisher.sidecar.phase(), SidecarPhase::GaveUp(_)));
    // A take does not restart a runtime that gave up; it falls back at once.
    let error = runtime
        .block_on(polisher.polish(INPUT, &context()))
        .err()
        .unwrap();
    assert_eq!(error.error(), &AppError::Polish);
    assert!(matches!(polisher.sidecar.phase(), SidecarPhase::GaveUp(_)));
    // An explicit prepare (grammar polish switched on again) tries again.
    assert!(runtime.block_on(polisher.prepare()).is_err());
    polisher.unload();
    assert_eq!(polisher.sidecar.phase(), SidecarPhase::Idle);
}
