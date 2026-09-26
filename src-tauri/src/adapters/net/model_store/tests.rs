/*!
 * SOURCE OF TRUTH KEYWORDS: HttpModelStore tests, loopback HTTP server, Range resume test, dropped connection test, redirect allowlist test, offline gate test, import verify remove tests
 * WHAT:  HttpModelStore against a small HTTP/1.1 server on 127.0.0.1 (full downloads, Range resume, a connection
 *        that drops mid-file, a server that ignores Range, wrong bytes, redirects inside and outside the allowlist,
 *        offline mode switched on mid-download, a runtime release archive unpacked into `runtimes/`) and against
 *        local folders (import of files or of the archive, verify, remove, status).
 * WHY:   02 §8.2 and §10 are promises about bytes on the wire and on disk, so they are tested with real sockets and
 *        real files; the server is test code on loopback (the allowlist here admits only `127.0.0.1` over http),
 *        so the gate never needs the internet and never touches the user's models.
 * WHERE: `cargo test` (adapters::net::model_store::tests).
 */

use std::{
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
};

use sha2::{Digest, Sha256};

use super::HttpModelStore;
use crate::{
    adapters::net::HttpClient,
    ports::{ModelStore, fakes::RecordingSink},
    types::{
        AllowedHost, AppError, AppPaths, ByteCount, HostAllowlist, HttpPolicy, ModelFile, ModelId,
        ModelKind, ModelManifest, ModelPhase, ModelProgress, ModelStatus, Permission,
        PermissionGate, PermissionState, PortError, PortResult, ResourceKind, Sha256Hex,
        StaticList, StaticStr, testing::TempDir,
    },
};

/// How the test server answers one path.
#[derive(Clone, Default)]
struct Route {
    body: Vec<u8>,
    /// Answer 200 with the whole body even when a range is asked for.
    ignore_range: bool,
    /// Answer 302 to this location instead.
    redirect_to: Option<String>,
    /// The next this-many answers stop after sending `cut_at` body bytes (the connection drops).
    cuts: usize,
    cut_at: usize,
}

#[derive(Default)]
struct ServerState {
    routes: HashMap<String, Route>,
    /// Every request: path and the Range start it asked for.
    requests: Vec<(String, Option<u64>)>,
}

/// A tiny HTTP/1.1 server on 127.0.0.1 (one thread per connection, `Connection: close`).
struct TestServer {
    port: u16,
    state: Arc<Mutex<ServerState>>,
}

impl TestServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(ServerState::default()));
        let shared = Arc::clone(&state);
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let state = Arc::clone(&shared);
                thread::spawn(move || answer(stream, &state));
            }
        });
        Self { port, state }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn route(&self, path: &str, route: Route) {
        self.state
            .lock()
            .unwrap()
            .routes
            .insert(path.to_owned(), route);
    }

    fn requests(&self) -> Vec<(String, Option<u64>)> {
        self.state.lock().unwrap().requests.clone()
    }
}

fn answer(stream: TcpStream, state: &Mutex<ServerState>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_owned();
    let mut range = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() || line.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("range")
        {
            range = value
                .trim()
                .strip_prefix("bytes=")
                .and_then(|spec| spec.trim_end_matches('-').parse::<u64>().ok());
        }
    }
    let route = {
        let mut state = state.lock().unwrap();
        state.requests.push((path.clone(), range));
        let route = state.routes.get(&path).cloned();
        if let Some(stored) = state.routes.get_mut(&path)
            && stored.cuts > 0
        {
            stored.cuts -= 1;
        }
        route
    };
    let mut stream = stream;
    let Some(route) = route else {
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    };
    if let Some(location) = route.redirect_to {
        let _ = write!(
            stream,
            "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        return;
    }
    let length = route.body.len();
    let start = match range {
        Some(start) if !route.ignore_range => usize::try_from(start).unwrap(),
        _ => 0,
    };
    if start > 0 && start >= length {
        let _ = write!(
            stream,
            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{length}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        return;
    }
    let head = if start > 0 {
        format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{length}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            length - 1,
            length - start
        )
    } else {
        format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n")
    };
    let body = &route.body[start..];
    let body = if route.cuts > 0 {
        &body[..route.cut_at.min(body.len())]
    } else {
        body
    };
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Deterministic, non-repeating test content.
fn content(seed: u8, length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| (index as u8).wrapping_mul(31).wrapping_add(seed) ^ (index >> 8) as u8)
        .collect()
}

/// A two-file manifest served by `server` at `/model/<name>`.
fn manifest(server: &TestServer, files: &[(&str, &[u8])]) -> ModelManifest {
    ModelManifest {
        id: ModelId::from_static("test-model"),
        label: StaticStr::new("Test model"),
        kind: ModelKind::Model,
        license: StaticStr::new("MIT"),
        attribution: None,
        revision: StaticStr::new("abc"),
        files: StaticList::from(
            files
                .iter()
                .map(|(name, bytes)| ModelFile {
                    name: StaticStr::from((*name).to_owned()),
                    url: StaticStr::from(server.url(&format!("/model/{name}"))),
                    sha256: Sha256Hex::from(sha256_hex(bytes)),
                    bytes: ByteCount::new(bytes.len() as u64),
                })
                .collect::<Vec<_>>(),
        ),
        archive: None,
        requires: StaticList::new(&[]),
        bundled: false,
    }
}

/// A zip of `entries`, every one Deflated like a llama.cpp release.
fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// A runtime manifest: `kept` unpacked from `archive`, served at `/release/tool.zip`.
fn runtime_manifest(server: &TestServer, archive: &[u8], kept: &[(&str, &[u8])]) -> ModelManifest {
    let url = server.url("/release/tool.zip");
    ModelManifest {
        id: ModelId::from_static("test-runtime"),
        label: StaticStr::new("Test runtime"),
        kind: ModelKind::Runtime,
        license: StaticStr::new("MIT"),
        attribution: None,
        revision: StaticStr::new("v1"),
        files: StaticList::from(
            kept.iter()
                .map(|(name, bytes)| ModelFile {
                    name: StaticStr::from((*name).to_owned()),
                    url: StaticStr::from(url.clone()),
                    sha256: Sha256Hex::from(sha256_hex(bytes)),
                    bytes: ByteCount::new(bytes.len() as u64),
                })
                .collect::<Vec<_>>(),
        ),
        archive: Some(ModelFile {
            name: StaticStr::new("tool.zip"),
            url: StaticStr::from(url),
            sha256: Sha256Hex::from(sha256_hex(archive)),
            bytes: ByteCount::new(archive.len() as u64),
        }),
        requires: StaticList::new(&[]),
        bundled: false,
    }
}

struct Rig {
    server: TestServer,
    data: TempDir,
    paths: AppPaths,
    store: HttpModelStore,
    online: Arc<AtomicBool>,
    /// Gate calls left before it starts denying (usize::MAX: never).
    gate_budget: Arc<AtomicUsize>,
}

impl Rig {
    fn new() -> Self {
        let server = TestServer::start();
        let data = TempDir::new("model-store");
        let paths = AppPaths::new(data.join("data"), data.join("resources"));
        let online = Arc::new(AtomicBool::new(true));
        let gate_budget = Arc::new(AtomicUsize::new(usize::MAX));
        let (reading, budget) = (Arc::clone(&online), Arc::clone(&gate_budget));
        let gate = PermissionGate::new(
            Permission::Network,
            AppError::Offline,
            Arc::new(move || {
                let left = budget.load(Ordering::SeqCst);
                if left != usize::MAX {
                    budget.store(left.saturating_sub(1), Ordering::SeqCst);
                }
                Ok(if reading.load(Ordering::SeqCst) && left > 0 {
                    PermissionState::Granted
                } else {
                    PermissionState::Denied
                })
            }),
        );
        let http = HttpClient::new(
            HostAllowlist {
                hosts: StaticList::from(vec![AllowedHost {
                    domain: StaticStr::new("127.0.0.1"),
                    include_subdomains: false,
                }]),
                require_https: false,
            },
            HttpPolicy {
                system_proxy: false,
                read_timeout_ms: 5_000,
                ..HttpPolicy::DEFAULT
            },
            gate,
        )
        .unwrap();
        let store = HttpModelStore::new(http, paths.clone());
        Self {
            server,
            data,
            paths,
            store,
            online,
            gate_budget,
        }
    }

    fn serve(&self, files: &[(&str, &[u8])]) -> ModelManifest {
        for (name, bytes) in files {
            self.server.route(
                &format!("/model/{name}"),
                Route {
                    body: bytes.to_vec(),
                    ..Route::default()
                },
            );
        }
        manifest(&self.server, files)
    }

    fn download(&self, manifest: &ModelManifest) -> (PortResult<()>, Vec<ModelProgress>) {
        let sink = RecordingSink::default();
        let result = block_on(self.store.download(manifest, &sink));
        (result, sink.events())
    }

    fn installed_file(&self, manifest: &ModelManifest, name: &str) -> Vec<u8> {
        fs::read(self.paths.model_dir(&manifest.id).join(name)).unwrap()
    }

    fn partial_file(&self, manifest: &ModelManifest, name: &str) -> std::path::PathBuf {
        self.paths.model_partial_dir(&manifest.id).join(name)
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn app_error<T>(result: PortResult<T>) -> AppError {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(error) => PortError::into_app_error(error),
    }
}

const BIG: usize = 3 * (1 << 20) + 12_345;

#[test]
fn a_download_verifies_every_file_and_installs_the_folder() {
    let rig = Rig::new();
    let big = content(1, BIG);
    let small = content(2, 1_000);
    let manifest = rig.serve(&[("model.onnx", &big), ("vocab.txt", &small)]);
    assert_eq!(
        rig.store.status(&manifest).unwrap(),
        ModelStatus::NotInstalled
    );
    let (result, progress) = rig.download(&manifest);
    result.unwrap();
    assert_eq!(rig.store.status(&manifest).unwrap(), ModelStatus::Installed);
    assert_eq!(rig.installed_file(&manifest, "model.onnx"), big);
    assert_eq!(rig.installed_file(&manifest, "vocab.txt"), small);
    assert!(!rig.paths.model_partial_dir(&manifest.id).exists());
    assert_eq!(
        rig.store.locate(&manifest).unwrap(),
        Some(rig.paths.model_dir(&manifest.id))
    );
    let total = manifest.total_bytes().get();
    let phases: Vec<ModelPhase> = progress.iter().map(|event| event.phase).collect();
    assert_eq!(
        &phases[phases.len() - 2..],
        [ModelPhase::Verifying, ModelPhase::Installing]
    );
    assert!(progress.iter().all(|event| event.total.get() == total));
    assert!(
        progress
            .windows(2)
            .all(|pair| pair[0].bytes <= pair[1].bytes),
        "bytes never go backwards"
    );
    assert_eq!(progress.last().map(|event| event.bytes.get()), Some(total));
}

#[test]
fn a_dropped_connection_keeps_what_arrived_and_the_next_download_resumes() {
    let rig = Rig::new();
    let big = content(3, BIG);
    let manifest = rig.serve(&[("model.onnx", &big)]);
    rig.server.route(
        "/model/model.onnx",
        Route {
            body: big.clone(),
            cuts: 1,
            cut_at: 1 << 20,
            ..Route::default()
        },
    );
    let (result, _) = rig.download(&manifest);
    assert_eq!(app_error(result), AppError::Network);
    let status = rig.store.status(&manifest).unwrap();
    let ModelStatus::Partial { bytes } = status else {
        panic!("expected a partial download, got {status:?}");
    };
    assert_eq!(bytes.get(), 1 << 20);

    let (result, _) = rig.download(&manifest);
    result.unwrap();
    assert_eq!(rig.installed_file(&manifest, "model.onnx"), big);
    assert_eq!(
        rig.server.requests(),
        [
            ("/model/model.onnx".to_owned(), None),
            ("/model/model.onnx".to_owned(), Some(1 << 20)),
        ],
        "the second request resumes with Range"
    );
}

#[test]
fn a_server_that_ignores_range_restarts_the_file() {
    let rig = Rig::new();
    let big = content(4, BIG);
    let manifest = rig.serve(&[("model.onnx", &big)]);
    rig.server.route(
        "/model/model.onnx",
        Route {
            body: big.clone(),
            ignore_range: true,
            ..Route::default()
        },
    );
    fs::create_dir_all(rig.paths.model_partial_dir(&manifest.id)).unwrap();
    fs::write(rig.partial_file(&manifest, "model.onnx"), &big[..5_000]).unwrap();
    rig.download(&manifest).0.unwrap();
    assert_eq!(rig.installed_file(&manifest, "model.onnx"), big);
}

#[test]
fn a_complete_partial_with_wrong_bytes_is_downloaded_again() {
    let rig = Rig::new();
    let small = content(5, 4_000);
    let manifest = rig.serve(&[("vocab.txt", &small)]);
    fs::create_dir_all(rig.paths.model_partial_dir(&manifest.id)).unwrap();
    // A complete file with the wrong bytes: its hash fails, so it is discarded and downloaded again.
    fs::write(rig.partial_file(&manifest, "vocab.txt"), vec![0_u8; 4_000]).unwrap();
    rig.download(&manifest).0.unwrap();
    assert_eq!(rig.installed_file(&manifest, "vocab.txt"), small);
}

#[test]
fn wrong_bytes_are_model_corrupt_and_discarded() {
    let rig = Rig::new();
    let expected = content(6, 10_000);
    let manifest = rig.serve(&[("model.onnx", &expected)]);
    rig.server.route(
        "/model/model.onnx",
        Route {
            body: content(7, 10_000),
            ..Route::default()
        },
    );
    let (result, _) = rig.download(&manifest);
    assert_eq!(
        app_error(result),
        AppError::ModelCorrupt {
            model_id: manifest.id.clone()
        }
    );
    assert!(!rig.partial_file(&manifest, "model.onnx").exists());
    rig.server.route(
        "/model/model.onnx",
        Route {
            body: content(7, 20_000),
            ..Route::default()
        },
    );
    assert!(matches!(
        app_error(rig.download(&manifest).0),
        AppError::ModelCorrupt { .. }
    ));
    assert_eq!(
        rig.store.status(&manifest).unwrap(),
        ModelStatus::Partial {
            bytes: ByteCount::new(0)
        }
    );
}

#[test]
fn redirects_are_followed_only_to_allowed_hosts() {
    let rig = Rig::new();
    let small = content(8, 2_000);
    let manifest = rig.serve(&[("vocab.txt", &small)]);
    rig.server.route(
        "/cdn/vocab.txt",
        Route {
            body: small.clone(),
            ..Route::default()
        },
    );
    rig.server.route(
        "/model/vocab.txt",
        Route {
            redirect_to: Some(rig.server.url("/cdn/vocab.txt")),
            ..Route::default()
        },
    );
    rig.download(&manifest).0.unwrap();
    assert_eq!(rig.installed_file(&manifest, "vocab.txt"), small);

    rig.store.remove(&manifest).unwrap();
    rig.server.route(
        "/model/vocab.txt",
        Route {
            redirect_to: Some(format!(
                "http://localhost:{}/cdn/vocab.txt",
                rig.server.port
            )),
            ..Route::default()
        },
    );
    assert_eq!(app_error(rig.download(&manifest).0), AppError::Network);
    let cdn_requests = rig
        .server
        .requests()
        .iter()
        .filter(|(path, _)| path == "/cdn/vocab.txt")
        .count();
    assert_eq!(cdn_requests, 1, "the disallowed hop was never requested");
}

#[test]
fn a_url_outside_the_allowlist_is_never_requested() {
    let rig = Rig::new();
    let small = content(9, 100);
    let mut manifest = rig.serve(&[("vocab.txt", &small)]);
    manifest.files = StaticList::from(vec![ModelFile {
        url: StaticStr::from(format!(
            "http://localhost:{}/model/vocab.txt",
            rig.server.port
        )),
        ..manifest.files[0].clone()
    }]);
    assert_eq!(app_error(rig.download(&manifest).0), AppError::Network);
    assert!(rig.server.requests().is_empty());
}

#[test]
fn offline_mode_blocks_a_download_and_stops_one_that_is_running() {
    let rig = Rig::new();
    let big = content(10, BIG);
    let manifest = rig.serve(&[("model.onnx", &big)]);
    rig.online.store(false, Ordering::SeqCst);
    assert_eq!(app_error(rig.download(&manifest).0), AppError::Offline);
    assert!(rig.server.requests().is_empty());

    rig.online.store(true, Ordering::SeqCst);
    // The request and a few chunks pass, then offline mode is on.
    rig.gate_budget.store(4, Ordering::SeqCst);
    assert_eq!(app_error(rig.download(&manifest).0), AppError::Offline);
    assert!(matches!(
        rig.store.status(&manifest).unwrap(),
        ModelStatus::Partial { .. }
    ));
    rig.gate_budget.store(usize::MAX, Ordering::SeqCst);
    rig.download(&manifest).0.unwrap();
    assert_eq!(rig.installed_file(&manifest, "model.onnx"), big);
}

/// A folder holding `files` as a user would pick it.
fn folder_with(root: &Path, files: &[(&str, &[u8])]) -> std::path::PathBuf {
    let folder = root.join("picked");
    fs::create_dir_all(&folder).unwrap();
    for (name, bytes) in files {
        fs::write(folder.join(name), bytes).unwrap();
    }
    folder
}

#[test]
fn import_copies_and_verifies_a_folder_offline() {
    let rig = Rig::new();
    let big = content(11, BIG);
    let small = content(12, 700);
    let files: [(&str, &[u8]); 2] = [("model.onnx", &big), ("vocab.txt", &small)];
    let manifest = manifest(&rig.server, &files);
    rig.online.store(false, Ordering::SeqCst);
    let folder = folder_with(rig.data.path(), &files);
    let sink = RecordingSink::default();
    block_on(rig.store.import(&manifest, &folder, &sink)).unwrap();
    assert_eq!(rig.store.status(&manifest).unwrap(), ModelStatus::Installed);
    assert_eq!(rig.installed_file(&manifest, "model.onnx"), big);
    assert!(
        folder.join("model.onnx").exists(),
        "the source is copied, not moved"
    );
    assert_eq!(
        sink.events().last().map(|event| event.phase),
        Some(ModelPhase::Installing)
    );
    assert!(rig.server.requests().is_empty());
}

#[test]
fn import_refuses_a_folder_that_is_not_the_model() {
    let rig = Rig::new();
    let small = content(13, 700);
    let other = content(14, 700);
    let manifest = manifest(&rig.server, &[("vocab.txt", &small), ("extra.bin", &small)]);
    let sink = RecordingSink::default();
    let missing = folder_with(rig.data.path(), &[("vocab.txt", &small)]);
    assert_eq!(
        app_error(block_on(rig.store.import(&manifest, &missing, &sink))),
        AppError::NotFound {
            resource: ResourceKind::Model
        }
    );
    fs::write(missing.join("extra.bin"), &other).unwrap();
    assert!(matches!(
        app_error(block_on(rig.store.import(&manifest, &missing, &sink))),
        AppError::ModelCorrupt { .. }
    ));
    fs::write(missing.join("extra.bin"), b"short").unwrap();
    assert!(matches!(
        app_error(block_on(rig.store.import(&manifest, &missing, &sink))),
        AppError::ModelCorrupt { .. }
    ));
    assert_ne!(rig.store.status(&manifest).unwrap(), ModelStatus::Installed);

    let staging = rig.paths.model_partial_dir(&manifest.id);
    fs::create_dir_all(&staging).unwrap();
    assert!(matches!(
        app_error(block_on(rig.store.import(&manifest, &staging, &sink))),
        AppError::Validation { .. }
    ));
}

#[test]
fn verify_finds_a_changed_byte_and_status_finds_a_changed_size() {
    let rig = Rig::new();
    let big = content(15, BIG);
    let manifest = rig.serve(&[("model.onnx", &big)]);
    let sink = RecordingSink::default();
    assert_eq!(
        app_error(block_on(rig.store.verify(&manifest, &sink))),
        AppError::ModelMissing {
            model_id: manifest.id.clone()
        }
    );
    rig.download(&manifest).0.unwrap();
    block_on(rig.store.verify(&manifest, &sink)).unwrap();
    assert!(
        sink.events()
            .iter()
            .any(|event| event.phase == ModelPhase::Verifying
                && event.bytes == manifest.total_bytes())
    );

    let installed = rig.paths.model_dir(&manifest.id).join("model.onnx");
    let mut tampered = big.clone();
    tampered[BIG / 2] ^= 0xFF;
    fs::write(&installed, &tampered).unwrap();
    assert_eq!(rig.store.status(&manifest).unwrap(), ModelStatus::Installed);
    assert!(matches!(
        app_error(block_on(rig.store.verify(&manifest, &sink))),
        AppError::ModelCorrupt { .. }
    ));

    fs::write(&installed, b"truncated").unwrap();
    assert_eq!(rig.store.status(&manifest).unwrap(), ModelStatus::Corrupt);
    assert_eq!(rig.store.locate(&manifest).unwrap(), None);
    // Downloading again replaces the damaged install.
    rig.download(&manifest).0.unwrap();
    assert_eq!(rig.installed_file(&manifest, "model.onnx"), big);
    assert!(!rig.paths.model_removal_dir(&manifest.id).exists());
}

#[test]
fn remove_deletes_the_install_and_the_partial_and_keeps_bundled_models() {
    let rig = Rig::new();
    let small = content(16, 3_000);
    let manifest = rig.serve(&[("vocab.txt", &small)]);
    rig.download(&manifest).0.unwrap();
    fs::create_dir_all(rig.paths.model_partial_dir(&manifest.id)).unwrap();
    fs::write(rig.partial_file(&manifest, "vocab.txt"), b"part").unwrap();
    rig.store.remove(&manifest).unwrap();
    rig.store.remove(&manifest).unwrap();
    assert_eq!(
        rig.store.status(&manifest).unwrap(),
        ModelStatus::NotInstalled
    );
    assert!(!rig.paths.model_dir(&manifest.id).exists());
    assert!(!rig.paths.model_removal_dir(&manifest.id).exists());

    let bundled = ModelManifest {
        bundled: true,
        ..manifest.clone()
    };
    assert_eq!(rig.store.status(&bundled).unwrap(), ModelStatus::Installed);
    assert!(matches!(
        app_error(rig.store.remove(&bundled)),
        AppError::Validation { .. }
    ));
    assert!(matches!(
        app_error(rig.download(&bundled).0),
        AppError::Validation { .. }
    ));
}

/// The archive rig: a tool, a library and a readme in one zip, of which the tool and the library are kept.
struct Release {
    tool: Vec<u8>,
    library: Vec<u8>,
    archive: Vec<u8>,
}

impl Release {
    fn new() -> Self {
        let tool = content(21, BIG);
        let library = content(22, 5_000);
        let archive = zip_of(&[
            ("tool.exe", &tool),
            ("readme.txt", b"not kept"),
            ("library.dll", &library),
        ]);
        Self {
            tool,
            library,
            archive,
        }
    }

    fn manifest(&self, server: &TestServer) -> ModelManifest {
        runtime_manifest(
            server,
            &self.archive,
            &[("tool.exe", &self.tool), ("library.dll", &self.library)],
        )
    }
}

impl Rig {
    fn serve_release(&self, release: &Release) -> ModelManifest {
        self.server.route(
            "/release/tool.zip",
            Route {
                body: release.archive.clone(),
                ..Route::default()
            },
        );
        release.manifest(&self.server)
    }

    fn runtime_dir(&self, manifest: &ModelManifest) -> std::path::PathBuf {
        self.paths.install_dir(ModelKind::Runtime, &manifest.id)
    }
}

#[test]
fn an_archive_download_keeps_only_the_listed_files_in_the_runtimes_folder() {
    let rig = Rig::new();
    let release = Release::new();
    let manifest = rig.serve_release(&release);
    let (result, progress) = rig.download(&manifest);
    result.unwrap();
    assert_eq!(rig.store.status(&manifest).unwrap(), ModelStatus::Installed);
    let folder = rig.runtime_dir(&manifest);
    assert_eq!(fs::read(folder.join("tool.exe")).unwrap(), release.tool);
    assert_eq!(
        fs::read(folder.join("library.dll")).unwrap(),
        release.library
    );
    assert!(
        !folder.join("readme.txt").exists(),
        "unlisted files stay in the archive"
    );
    assert!(!folder.join("tool.zip").exists(), "the archive is not kept");
    assert!(
        !rig.paths.model_dir(&manifest.id).exists(),
        "runtimes never land in models/"
    );
    assert!(
        !rig.paths
            .install_partial_dir(ModelKind::Runtime, &manifest.id)
            .exists()
    );
    assert_eq!(rig.store.locate(&manifest).unwrap(), Some(folder));
    let archive_bytes = release.archive.len() as u64;
    assert!(
        progress
            .iter()
            .all(|event| event.total.get() == archive_bytes)
    );
    assert_eq!(
        progress
            .last()
            .map(|event| (event.bytes.get(), event.phase)),
        Some((archive_bytes, ModelPhase::Installing))
    );
    let sink = RecordingSink::default();
    block_on(rig.store.verify(&manifest, &sink)).unwrap();
    rig.store.remove(&manifest).unwrap();
    assert_eq!(
        rig.store.status(&manifest).unwrap(),
        ModelStatus::NotInstalled
    );
}

#[test]
fn an_archive_member_that_is_absent_or_different_fails_the_install() {
    let rig = Rig::new();
    let release = Release::new();
    let mut manifest = rig.serve_release(&release);
    let mut files = manifest.files.to_vec();
    files[1].sha256 = Sha256Hex::from(sha256_hex(b"something else"));
    manifest.files = StaticList::from(files);
    let (result, _) = rig.download(&manifest);
    assert_eq!(
        app_error(result),
        AppError::ModelCorrupt {
            model_id: manifest.id.clone()
        }
    );
    assert_ne!(rig.store.status(&manifest).unwrap(), ModelStatus::Installed);
    let absent = runtime_manifest(
        &rig.server,
        &release.archive,
        &[
            ("tool.exe", &release.tool),
            ("absent.dll", &release.library),
        ],
    );
    assert!(matches!(
        app_error(rig.download(&absent).0),
        AppError::ModelCorrupt { .. }
    ));
}

#[test]
fn an_interrupted_archive_download_is_partial_and_resumes() {
    let rig = Rig::new();
    let release = Release::new();
    let manifest = release.manifest(&rig.server);
    // Half the archive arrives before the connection drops.
    let half = release.archive.len() / 2;
    rig.server.route(
        "/release/tool.zip",
        Route {
            body: release.archive.clone(),
            cuts: 1,
            cut_at: half,
            ..Route::default()
        },
    );
    assert_eq!(app_error(rig.download(&manifest).0), AppError::Network);
    assert_eq!(
        rig.store.status(&manifest).unwrap(),
        ModelStatus::Partial {
            bytes: ByteCount::new(half as u64)
        }
    );
    rig.download(&manifest).0.unwrap();
    assert_eq!(rig.store.status(&manifest).unwrap(), ModelStatus::Installed);
    assert_eq!(
        rig.server.requests().last(),
        Some(&("/release/tool.zip".to_owned(), Some(half as u64)))
    );
}

#[test]
fn a_runtime_imports_from_a_folder_with_its_archive_or_its_files() {
    let rig = Rig::new();
    let release = Release::new();
    let manifest = release.manifest(&rig.server);
    rig.online.store(false, Ordering::SeqCst);
    let sink = RecordingSink::default();
    let with_archive = folder_with(rig.data.path(), &[("tool.zip", &release.archive)]);
    block_on(rig.store.import(&manifest, &with_archive, &sink)).unwrap();
    assert_eq!(rig.store.status(&manifest).unwrap(), ModelStatus::Installed);
    assert!(!rig.runtime_dir(&manifest).join("tool.zip").exists());
    rig.store.remove(&manifest).unwrap();
    let unpacked = rig.data.path().join("unpacked");
    fs::create_dir_all(&unpacked).unwrap();
    fs::write(unpacked.join("tool.exe"), &release.tool).unwrap();
    fs::write(unpacked.join("library.dll"), &release.library).unwrap();
    block_on(rig.store.import(&manifest, &unpacked, &sink)).unwrap();
    assert_eq!(rig.store.status(&manifest).unwrap(), ModelStatus::Installed);
    let empty = rig.data.path().join("empty");
    fs::create_dir_all(&empty).unwrap();
    assert_eq!(
        app_error(block_on(rig.store.import(&manifest, &empty, &sink))),
        AppError::NotFound {
            resource: ResourceKind::Model
        }
    );
    assert!(rig.server.requests().is_empty());
}
