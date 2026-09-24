/*!
 * SOURCE OF TRUTH KEYWORDS: architecture test, import matrix, layer rules, downward only, W23 path literals, tauri::command placement
 * WHAT:  Fails `cargo test` when src-tauri/src breaks a structural rule: an import the 02 §3.2 matrix forbids, a
 *        hardcoded Windows path literal (05 W23), `#[tauri::command]` outside ipc/factory.rs, or a Rust file
 *        outside the known layer folders.
 * WHY:   The layer rules only hold if a machine checks them on every gate run. The checker lexes each file first
 *        (comments, strings, char literals and lifetimes), so a doc comment that mentions `crate::services` or an
 *        example path never trips it, while grouped imports (`use crate::{a, b}`), `$crate` paths inside macros
 *        and `super::` chains that climb to the crate root (tracking inline `mod x { }` blocks) are still caught.
 *        A new top-level folder must be added to LAYERS and the matrix, so it can't slip in unchecked.
 * WHERE: `cargo test` (local gate, 02 §11). MATRIX mirrors docs/02-TECHNICAL-PLAN.md §3.2; change both together.
 */

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

/// Layer folders under src-tauri/src, bottom to top.
const LAYERS: [&str; 8] = [
    "types", "ports", "adapters", "services", "registry", "pipeline", "ipc", "app",
];

/// 02 §3.2: the layers each row may import. A layer may always import itself.
const MATRIX: [(&str, &[&str]); 8] = [
    ("types", &[]),
    ("ports", &["types"]),
    ("adapters", &["types", "ports"]),
    ("services", &["types"]),
    ("registry", &["types", "ports", "adapters"]),
    ("pipeline", &["types", "ports", "services", "registry"]),
    (
        "ipc",
        &["types", "ports", "services", "registry", "pipeline"],
    ),
    (
        "app",
        &[
            "types", "ports", "adapters", "services", "registry", "pipeline", "ipc",
        ],
    ),
];

/// The only file allowed to write `#[tauri::command]` (02 §4.1).
const COMMAND_FACTORY: &str = "ipc/factory.rs";

/// Files at the crate root that belong to no layer.
const CRATE_ROOT_FILES: [&str; 2] = ["lib.rs", "main.rs"];

/// Windows folder placeholders that must be resolved through the Tauri path API instead.
const WINDOWS_FOLDER_VARIABLES: [&str; 9] = [
    "%appdata%",
    "%localappdata%",
    "%userprofile%",
    "%programfiles%",
    "%programdata%",
    "%temp%",
    "%systemroot%",
    "%windir%",
    "%homepath%",
];

#[derive(Debug, Clone, PartialEq, Eq)]
enum Violation {
    ForbiddenImport {
        file: String,
        line: usize,
        from: String,
        to: String,
    },
    WindowsPathLiteral {
        file: String,
        line: usize,
        literal: String,
    },
    TauriCommandOutsideFactory {
        file: String,
        line: usize,
    },
    UnknownLayer {
        file: String,
    },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ForbiddenImport {
                file,
                line,
                from,
                to,
            } => write!(
                f,
                "src/{file}:{line}: `{from}` must not import `{to}` (docs/02 §3.2 import matrix)"
            ),
            Self::WindowsPathLiteral {
                file,
                line,
                literal,
            } => write!(
                f,
                "src/{file}:{line}: hardcoded Windows path \"{literal}\"; resolve it through the Tauri path API (05 W23)"
            ),
            Self::TauriCommandOutsideFactory { file, line } => write!(
                f,
                "src/{file}:{line}: `#[tauri::command]` is written only inside {COMMAND_FACTORY}; declare commands with echo_command!"
            ),
            Self::UnknownLayer { file } => write!(
                f,
                "src/{file}: not inside a known layer folder; add the layer to LAYERS and MATRIX (and docs/02 §3.2)"
            ),
        }
    }
}

fn allowed(from: &str, to: &str) -> bool {
    from == to
        || MATRIX
            .iter()
            .any(|(row, targets)| *row == from && targets.contains(&to))
}

/**
 * SOURCE OF TRUTH KEYWORDS: Scanned, scan, lexer, comment stripping, string literals, raw strings, lifetimes
 * WHAT:  Splits Rust source into `code` (comments and literal contents blanked, newlines kept so line numbers
 *        survive) and the string literals with their starting line.
 * WHY:   Path and import checks must ignore prose in comments and text inside strings, and the path-literal check
 *        must look only inside strings. Char literals are told apart from lifetimes so `'"'` can't open a string.
 * WHERE: check_source.
 */
struct Scanned {
    code: String,
    literals: Vec<(usize, String)>,
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn blank(out: &mut String, c: char) {
    out.push(if c == '\n' { '\n' } else { ' ' });
}

fn scan(source: &str) -> Scanned {
    let chars: Vec<char> = source.chars().collect();
    let at = |index: usize| chars.get(index).copied();
    let mut code = String::with_capacity(source.len());
    let mut literals = Vec::new();
    let mut line = 1;
    let mut i = 0;

    while let Some(c) = at(i) {
        // Line comment (incl. doc comments).
        if c == '/' && at(i + 1) == Some('/') {
            while let Some(next) = at(i) {
                if next == '\n' {
                    break;
                }
                blank(&mut code, next);
                i += 1;
            }
            continue;
        }
        // Block comment, nested.
        if c == '/' && at(i + 1) == Some('*') {
            let mut depth = 0_usize;
            while let Some(next) = at(i) {
                if next == '/' && at(i + 1) == Some('*') {
                    depth += 1;
                    code.push_str("  ");
                    i += 2;
                    continue;
                }
                if next == '*' && at(i + 1) == Some('/') {
                    depth -= 1;
                    code.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                if next == '\n' {
                    line += 1;
                }
                blank(&mut code, next);
                i += 1;
            }
            continue;
        }

        // String literal, with optional b/c prefix and optional raw marker.
        let starts_token = i == 0 || at(i - 1).is_none_or(|prev| !is_ident_char(prev));
        let mut prefix_len = 0;
        if starts_token {
            if matches!(c, 'b' | 'c') {
                prefix_len = 1;
            }
            if at(i + prefix_len) == Some('r') {
                prefix_len += 1;
            }
        }
        let raw = prefix_len > 0 && at(i + prefix_len - 1) == Some('r');
        let mut hashes = 0;
        if raw {
            while at(i + prefix_len + hashes) == Some('#') {
                hashes += 1;
            }
        }
        let quote = i + prefix_len + hashes;
        if at(quote) == Some('"') && (prefix_len > 0 || c == '"') {
            let start_line = line;
            for index in i..=quote {
                if let Some(prefix) = at(index) {
                    blank(&mut code, prefix);
                }
            }
            let mut body = String::new();
            let mut j = quote + 1;
            while let Some(next) = at(j) {
                if !raw && next == '\\' {
                    body.push(next);
                    if let Some(escaped) = at(j + 1) {
                        body.push(escaped);
                        if escaped == '\n' {
                            line += 1;
                        }
                        blank(&mut code, next);
                        blank(&mut code, escaped);
                    }
                    j += 2;
                    continue;
                }
                if next == '"' && (0..hashes).all(|offset| at(j + 1 + offset) == Some('#')) {
                    for _ in 0..=hashes {
                        code.push(' ');
                    }
                    j += 1 + hashes;
                    break;
                }
                if next == '\n' {
                    line += 1;
                }
                body.push(next);
                blank(&mut code, next);
                j += 1;
            }
            literals.push((start_line, body));
            i = j;
            continue;
        }

        // Char literal ('a', '\n', '\'') versus lifetime ('a).
        if c == '\'' {
            let char_end = if at(i + 1) == Some('\\') {
                (i + 2..chars.len())
                    .find(|&index| at(index) == Some('\'') && at(index - 1) != Some('\\'))
            } else if at(i + 2) == Some('\'') {
                Some(i + 2)
            } else {
                None
            };
            if let Some(end) = char_end {
                for index in i..=end {
                    if let Some(inner) = at(index) {
                        blank(&mut code, inner);
                    }
                }
                i = end + 1;
                continue;
            }
        }

        if c == '\n' {
            line += 1;
        }
        code.push(c);
        i += 1;
    }

    Scanned { code, literals }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Ident(String),
    PathSep,
    Punct(char),
}

/// Tokens of blanked code with their line numbers. Numbers are dropped; they never matter here.
fn tokenize(code: &str) -> Vec<(Token, usize)> {
    let chars: Vec<char> = code.chars().collect();
    let mut tokens = Vec::new();
    let mut line = 1;
    let mut i = 0;
    while let Some(&c) = chars.get(i) {
        if c == '\n' {
            line += 1;
            i += 1;
        } else if c.is_whitespace() {
            i += 1;
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while chars.get(i).is_some_and(|&next| is_ident_char(next)) {
                i += 1;
            }
            tokens.push((Token::Ident(chars[start..i].iter().collect()), line));
        } else if c.is_ascii_digit() {
            while chars.get(i).is_some_and(|&next| is_ident_char(next)) {
                i += 1;
            }
        } else if c == ':' && chars.get(i + 1) == Some(&':') {
            tokens.push((Token::PathSep, line));
            i += 2;
        } else {
            tokens.push((Token::Punct(c), line));
            i += 1;
        }
    }
    tokens
}

fn ident(tokens: &[(Token, usize)], index: usize) -> Option<&str> {
    match tokens.get(index) {
        Some((Token::Ident(name), _)) => Some(name),
        _ => None,
    }
}

fn is(tokens: &[(Token, usize)], index: usize, expected: &Token) -> bool {
    tokens
        .get(index)
        .is_some_and(|(token, _)| token == expected)
}

/// First path segments named at `index`, right after `crate::` (or a `super::` chain that reached the root):
/// a single name, every item of a `{ … }` group, or `*` for a glob.
fn root_targets(tokens: &[(Token, usize)], index: usize) -> Vec<String> {
    if let Some(name) = ident(tokens, index) {
        return vec![name.to_owned()];
    }
    if is(tokens, index, &Token::Punct('*')) {
        return vec![String::from("*")];
    }
    if !is(tokens, index, &Token::Punct('{')) {
        return Vec::new();
    }
    let mut targets = Vec::new();
    let mut depth = 0_usize;
    let mut expecting_item = true;
    for (token, _) in tokens.iter().skip(index) {
        match token {
            Token::Punct('{') => {
                depth += 1;
                expecting_item = depth == 1;
            }
            Token::Punct('}') => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Token::Punct(',') if depth == 1 => expecting_item = true,
            Token::Ident(name) if expecting_item => {
                targets.push(name.clone());
                expecting_item = false;
            }
            Token::Punct('*') if expecting_item && depth == 1 => {
                targets.push(String::from("*"));
                expecting_item = false;
            }
            _ => expecting_item = false,
        }
    }
    targets
}

/// Module path of a file relative to src/: `pipeline/session/actor.rs` → [pipeline, session, actor].
fn module_path(file: &str) -> Vec<String> {
    let mut segments: Vec<String> = file
        .trim_end_matches(".rs")
        .split('/')
        .map(str::to_owned)
        .collect();
    if segments.last().is_some_and(|last| last == "mod") || CRATE_ROOT_FILES.contains(&file) {
        segments.pop();
    }
    segments
}

/**
 * SOURCE OF TRUTH KEYWORDS: crate_references, crate root paths, super chains, inline modules, $crate
 * WHAT:  Every (line, top-level name) this file reaches from the crate root.
 * WHY:   `crate::x`, `$crate::x` and a `super::` chain that climbs out of the file's module all name crate-root
 *        items; only those can cross a layer boundary. Inline `mod name { … }` blocks deepen the module path,
 *        so `super` inside a test module is resolved correctly.
 * WHERE: check_source.
 */
fn crate_references(file: &str, tokens: &[(Token, usize)]) -> Vec<(usize, String)> {
    let file_module = module_path(file);
    let mut inline_modules: Vec<(String, usize)> = Vec::new();
    let mut pending_module: Option<String> = None;
    let mut depth = 0_usize;
    let mut references = Vec::new();

    for (index, (token, line)) in tokens.iter().enumerate() {
        match token {
            Token::Punct('{') => {
                depth += 1;
                if let Some(name) = pending_module.take() {
                    inline_modules.push((name, depth));
                }
            }
            Token::Punct('}') => {
                if inline_modules
                    .last()
                    .is_some_and(|(_, open)| *open == depth)
                {
                    inline_modules.pop();
                }
                depth = depth.saturating_sub(1);
            }
            Token::Punct(';') => pending_module = None,
            Token::Ident(word) if word == "mod" => {
                pending_module = ident(tokens, index + 1).map(str::to_owned);
            }
            Token::Ident(word) if word == "crate" && is(tokens, index + 1, &Token::PathSep) => {
                let starts_path = index == 0 || !is(tokens, index - 1, &Token::PathSep);
                if starts_path {
                    for target in root_targets(tokens, index + 2) {
                        references.push((*line, target));
                    }
                }
            }
            Token::Ident(word) if word == "super" => {
                if index > 0 && is(tokens, index - 1, &Token::PathSep) {
                    continue;
                }
                let mut hops = 0;
                let mut cursor = index;
                while ident(tokens, cursor) == Some("super")
                    && is(tokens, cursor + 1, &Token::PathSep)
                {
                    hops += 1;
                    cursor += 2;
                }
                let module_depth = file_module.len() + inline_modules.len();
                if hops > 0 && hops >= module_depth {
                    for target in root_targets(tokens, cursor) {
                        references.push((*line, target));
                    }
                }
            }
            _ => {}
        }
    }
    references
}

/// True when a string literal's source text holds a drive-letter path or a Windows folder placeholder.
fn is_windows_path_literal(body: &str) -> bool {
    let chars: Vec<char> = body.chars().collect();
    let drive_letter = chars.windows(3).enumerate().any(|(index, window)| {
        let standalone = index == 0
            || chars
                .get(index - 1)
                .is_none_or(|prev| !prev.is_alphanumeric());
        matches!(window, [letter, ':', '\\' | '/'] if letter.is_ascii_alphabetic()) && standalone
    });
    let lower = body.to_lowercase();
    drive_letter
        || WINDOWS_FOLDER_VARIABLES
            .iter()
            .any(|variable| lower.contains(variable))
}

/// Every `tauri::command` path and bare `#[command]` attribute, by line.
fn tauri_command_lines(tokens: &[(Token, usize)]) -> Vec<usize> {
    let command = Token::Ident(String::from("command"));
    tokens
        .iter()
        .enumerate()
        .filter_map(|(index, (token, line))| {
            let qualified = *token == Token::Ident(String::from("tauri"))
                && is(tokens, index + 1, &Token::PathSep)
                && is(tokens, index + 2, &command);
            let bare_attribute = *token == Token::Punct('#')
                && is(tokens, index + 1, &Token::Punct('['))
                && is(tokens, index + 2, &command)
                && (is(tokens, index + 3, &Token::Punct(']'))
                    || is(tokens, index + 3, &Token::Punct('(')));
            (qualified || bare_attribute).then_some(*line)
        })
        .collect()
}

/**
 * SOURCE OF TRUTH KEYWORDS: check_source, architecture violations, layer of file
 * WHAT:  All violations in one file, given its path relative to src/ (forward slashes) and its source.
 * WHY:   Pure function of (path, text), so the unit tests below can feed it forbidden fixtures directly.
 * WHERE: source_tree_follows_the_architecture and the fixture tests.
 */
fn check_source(file: &str, source: &str) -> Vec<Violation> {
    let scanned = scan(source);
    let tokens = tokenize(&scanned.code);
    let mut violations = Vec::new();

    let layer = file
        .split('/')
        .next()
        .filter(|first| LAYERS.contains(first));
    match layer {
        Some(from) => {
            for (line, to) in crate_references(file, &tokens) {
                let is_layer = LAYERS.contains(&to.as_str()) || to == "*";
                if is_layer && !allowed(from, &to) {
                    violations.push(Violation::ForbiddenImport {
                        file: file.to_owned(),
                        line,
                        from: from.to_owned(),
                        to,
                    });
                }
            }
        }
        None if CRATE_ROOT_FILES.contains(&file) => {}
        None => violations.push(Violation::UnknownLayer {
            file: file.to_owned(),
        }),
    }

    for (line, literal) in scanned.literals {
        if is_windows_path_literal(&literal) {
            violations.push(Violation::WindowsPathLiteral {
                file: file.to_owned(),
                line,
                literal,
            });
        }
    }

    if file != COMMAND_FACTORY {
        for line in tauri_command_lines(&tokens) {
            violations.push(Violation::TauriCommandOutsideFactory {
                file: file.to_owned(),
                line,
            });
        }
    }
    violations
}

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, found)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
    Ok(())
}

#[test]
fn source_tree_follows_the_architecture() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&root, &mut files).expect("src-tauri/src is readable");
    assert!(
        files.len() >= LAYERS.len(),
        "expected at least one file per layer"
    );

    let mut violations = Vec::new();
    for path in files {
        let relative = path.strip_prefix(&root).expect("file is under src/");
        let file = relative.to_string_lossy().replace('\\', "/");
        let source = fs::read_to_string(&path).expect("source file is UTF-8");
        violations.extend(check_source(&file, &source));
    }
    let report: Vec<String> = violations.iter().map(ToString::to_string).collect();
    assert!(
        report.is_empty(),
        "architecture violations:\n{}",
        report.join("\n")
    );
}

#[test]
fn matrix_matches_the_documented_table() {
    for from in LAYERS {
        assert!(
            MATRIX.iter().any(|(row, _)| *row == from),
            "{from} has no matrix row"
        );
    }
    // Spot checks straight from 02 §3.2.
    assert!(allowed("ports", "types"));
    assert!(allowed("registry", "adapters"));
    assert!(allowed("app", "ipc"));
    assert!(!allowed("types", "ports"));
    assert!(!allowed("services", "ports"));
    assert!(!allowed("pipeline", "adapters"));
    assert!(!allowed("ipc", "adapters"));
    assert!(!allowed("ipc", "app"));
    assert!(!allowed("registry", "services"));
}

#[test]
fn forbidden_import_fails() {
    let violations = check_source("types/error.rs", "use crate::services::transcripts;\n");
    assert_eq!(
        violations,
        [Violation::ForbiddenImport {
            file: String::from("types/error.rs"),
            line: 1,
            from: String::from("types"),
            to: String::from("services"),
        }]
    );
}

#[test]
fn allowed_imports_pass() {
    let source = "use crate::types::AppError;\nuse crate::{ports::AsrEngine, services};\nfn f() { crate::registry::x(); }\n";
    assert_eq!(check_source("pipeline/polish.rs", source), []);
}

#[test]
fn grouped_inline_and_macro_paths_are_checked() {
    let grouped = "use crate::{types::{A, B}, adapters::Parakeet};\n";
    let inline = "fn f() {\n    let x = crate::app::run();\n}\n";
    let macro_path = "macro_rules! m { () => { $crate::ipc::x!() } }\n";
    let glob = "use crate::*;\n";
    let to = |violations: Vec<Violation>| -> Vec<(usize, String)> {
        violations
            .into_iter()
            .filter_map(|violation| match violation {
                Violation::ForbiddenImport { line, to, .. } => Some((line, to)),
                _ => None,
            })
            .collect()
    };
    assert_eq!(
        to(check_source("pipeline/a.rs", grouped)),
        [(1, String::from("adapters"))]
    );
    assert_eq!(
        to(check_source("services/a.rs", inline)),
        [(2, String::from("app"))]
    );
    assert_eq!(
        to(check_source("registry/a.rs", macro_path)),
        [(1, String::from("ipc"))]
    );
    assert_eq!(
        to(check_source("types/a.rs", glob)),
        [(1, String::from("*"))]
    );
}

#[test]
fn super_chains_that_reach_the_crate_root_are_checked() {
    // pipeline/session/actor.rs is module [pipeline, session, actor]: three hops reach the root.
    let escape = "use super::super::super::adapters::Parakeet;\n";
    assert_eq!(check_source("pipeline/session/actor.rs", escape).len(), 1);
    // Two hops stay inside pipeline.
    assert_eq!(
        check_source("pipeline/session/actor.rs", "use super::super::polish;\n"),
        []
    );
    // Inside an inline test module one more hop is needed: from types::error::tests, two hops reach types.
    let inline = "mod tests {\n    use super::super::super::services::x;\n}\n";
    assert_eq!(check_source("types/error.rs", inline).len(), 1);
    let inline_in_layer = "mod tests {\n    use super::super::ids;\n}\n";
    assert_eq!(check_source("types/error.rs", inline_in_layer), []);
    let inline_shallow = "mod tests {\n    use super::*;\n}\nuse super::ids;\n";
    assert_eq!(check_source("types/error.rs", inline_shallow), []);
}

#[test]
fn comments_and_strings_do_not_count_as_imports() {
    let source = "//! Talks to crate::services.\n/* crate::adapters /* nested */ still comment */\nconst S: &str = \"crate::app\";\nconst R: &str = r#\"use crate::ipc;\"#;\nconst C: char = '\"';\nfn f<'a>(x: &'a str) -> &'a str { x }\n";
    assert_eq!(check_source("types/doc.rs", source), []);
}

#[test]
fn windows_path_literals_fail() {
    let cases = [
        "const P: &str = \"C:\\\\Users\\\\me\";\n",
        "const P: &str = r\"D:\\data\\echo.db\";\n",
        "const P: &str = \"c:/Program Files/Echo\";\n",
        "const P: &str = \"%LOCALAPPDATA%\\\\Echo\";\n",
    ];
    for source in cases {
        let violations = check_source("services/db.rs", source);
        assert!(
            matches!(
                violations.as_slice(),
                [Violation::WindowsPathLiteral { line: 1, .. }]
            ),
            "{source} → {violations:?}"
        );
    }
}

#[test]
fn urls_and_commented_paths_pass() {
    let source = "// Installs to C:\\Users\\me by default.\nconst U: &str = \"https://huggingface.co/x\";\nconst W: &str = \"ws://localhost:1420\";\nconst T: &str = \"12:30\";\n";
    assert_eq!(check_source("adapters/net/http_client.rs", source), []);
}

#[test]
fn tauri_command_outside_the_factory_fails() {
    let qualified = "#[tauri::command]\nfn history_list() {}\n";
    let bare = "use tauri::command;\n#[command]\nfn history_list() {}\n";
    assert_eq!(
        check_source("ipc/commands/history.rs", qualified),
        [Violation::TauriCommandOutsideFactory {
            file: String::from("ipc/commands/history.rs"),
            line: 1,
        }]
    );
    assert_eq!(check_source("ipc/commands/history.rs", bare).len(), 2);
    assert_eq!(check_source(COMMAND_FACTORY, qualified), []);
    assert_eq!(
        check_source(
            "ipc/commands/history.rs",
            "// #[tauri::command] lives in the factory.\n"
        ),
        []
    );
}

#[test]
fn files_outside_the_layers_fail() {
    assert_eq!(
        check_source("helpers/misc.rs", "pub fn f() {}\n"),
        [Violation::UnknownLayer {
            file: String::from("helpers/misc.rs"),
        }]
    );
    assert_eq!(check_source("lib.rs", "pub mod types;\n"), []);
}
