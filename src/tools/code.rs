//! Code mode tools — filesystem I/O, search, execution, linting, build.
//!
//! ## Guardrails
//! - `code_run` requires `ALLOW_CODE=1` env var (separate from `ALLOW_SHELL`)
//! - All file paths are validated against `DESKMCP_WORKSPACE` root (default `$HOME/Projects`)
//! - Execution tools have a 30-second default timeout, 300-second maximum
//!
//! ## Performance
//! - All file I/O uses `tokio::fs` for non-blocking async operations
//! - `glob_search` uses the native Rust `glob` crate (no `find` subprocess)

use crate::response::ToolResponse;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Resolve the workspace root. Order of precedence:
///   1. `DESKMCP_WORKSPACE` env var
///   2. `$HOME/Projects`
///   3. `/tmp/deskmcp_workspace` (fallback)
fn workspace_root() -> PathBuf {
    if let Ok(root) = std::env::var("DESKMCP_WORKSPACE") {
        return PathBuf::from(root);
    }
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(&home).join("Projects");
        if p.exists() {
            return p;
        }
        return PathBuf::from(home);
    }
    PathBuf::from("/tmp/deskmcp_workspace")
}

/// Validate that a file path is within the workspace root.
/// Returns the canonical resolved path on success.
fn resolve_safe(path: &str) -> Result<PathBuf, String> {
    let root = workspace_root();
    let root = root.canonicalize().unwrap_or_else(|_| root.clone());

    let candidate = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        root.join(path)
    };

    // Canonicalize, falling back to the unresolved path
    let resolved = candidate.canonicalize().unwrap_or_else(|_| {
        if let Some(parent) = candidate.parent() {
            let p = parent.canonicalize().unwrap_or_else(|_| parent.to_path_buf());
            p.join(candidate.file_name().unwrap_or_default())
        } else {
            candidate.clone()
        }
    });

    let root_canon = root.canonicalize().unwrap_or_else(|_| root.clone());

    if !resolved.starts_with(&root_canon) {
        return Err(format!(
            "Path '{}' is outside workspace root '{}'",
            path,
            root_canon.display()
        ));
    }

    Ok(resolved)
}

/// Run a shell command with timeout. Returns (stdout, stderr, exit_code).
fn run_cmd(cmd: &str, args: &[&str], timeout_secs: u64, cwd: Option<&Path>) -> Result<(String, String, i32), String> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .current_dir(cwd.unwrap_or_else(|| Path::new(".")))
        .spawn()
        .map_err(|e| format!("Failed to spawn '{cmd}': {e}"))?;

    let timeout = Duration::from_secs(timeout_secs.min(300).max(1));

    match wait_timeout(&mut child, timeout) {
        Ok(Some(status)) => {
            let stdout = String::from_utf8_lossy(&child.stdout.take().map_or_else(Vec::new, |mut p| {
                let mut buf = String::new();
                std::io::Read::read_to_string(&mut p, &mut buf).unwrap_or_default();
                buf.into_bytes()
            })).to_string();

            let stderr = String::from_utf8_lossy(&child.stderr.take().map_or_else(Vec::new, |mut p| {
                let mut buf = String::new();
                std::io::Read::read_to_string(&mut p, &mut buf).unwrap_or_default();
                buf.into_bytes()
            })).to_string();

            let code = status.code().unwrap_or(-1);
            Ok((stdout, stderr, code))
        }
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(format!("Command timed out after {timeout_secs}s"))
        }
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(format!("Command error: {e}"))
        }
    }
}

/// Wait for a child process with a timeout.
/// Returns `Ok(None)` on timeout, `Ok(Some(status))` on completion.
fn wait_timeout(child: &mut std::process::Child, timeout: Duration) -> Result<Option<std::process::ExitStatus>, String> {
    let start = std::time::Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_e) => {
                if start.elapsed() >= timeout {
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Main handler dispatch
pub async fn handle(name: &str, args: Value) -> ToolResponse {
    let result = handle_inner(name, args).await;
    match result {
        Ok(v) => crate::response::ok(&v),
        Err(e) => crate::response::err("CODE_ERROR", &e),
    }
}

async fn handle_inner(name: &str, args: Value) -> Result<Value, String> {
    match name {
        "file_read" => file_read(args).await,
        "file_write" => file_write(args).await,
        "file_edit" => file_edit(args).await,
        "grep" => grep(args).await,
        "glob" => glob_search(args).await,
        "code_run" => code_run(args).await,
        "code_lint" => code_lint(args).await,
        "code_build" => code_build(args).await,
        _ => Err(format!("Unknown code tool: {name}")),
    }
}

// ═══════════════════ FILE TOOLS ═══════════════════

async fn file_read(args: Value) -> Result<Value, String> {
    let path_str = args["path"].as_str().ok_or("Missing 'path'")?;
    let path = resolve_safe(path_str)?;

    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Cannot read '{}': {e}", path.display()))?;

    // Return with line numbers
    let lines: Vec<Value> = content
        .lines()
        .enumerate()
        .map(|(i, line)| {
            serde_json::json!({
                "line": i + 1,
                "text": line,
            })
        })
        .collect();

    let offset = args["offset"].as_u64().unwrap_or(0) as usize;
    let limit = args["limit"].as_u64().unwrap_or(lines.len() as u64) as usize;
    let total = lines.len();
    let slice: Vec<_> = lines.into_iter().skip(offset).take(limit).collect();

    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "lines": slice.len(),
        "total_lines": total,
        "content": slice,
    }))
}

async fn file_write(args: Value) -> Result<Value, String> {
    let path_str = args["path"].as_str().ok_or("Missing 'path'")?;
    let content = args["content"].as_str().ok_or("Missing 'content'")?;
    let path = resolve_safe(path_str)?;

    // Create parent directories (sync — dir creation is fast)
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create parent dir: {e}"))?;
    }

    tokio::fs::write(&path, content)
        .await
        .map_err(|e| format!("Cannot write '{}': {e}", path.display()))?;

    let size = content.len();
    let lines = content.lines().count();

    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "bytes_written": size,
        "lines": lines,
    }))
}

async fn file_edit(args: Value) -> Result<Value, String> {
    let path_str = args["path"].as_str().ok_or("Missing 'path'")?;
    let old_string = args["old_string"].as_str().ok_or("Missing 'old_string'")?;
    let new_string = args["new_string"].as_str().ok_or("Missing 'new_string'")?;
    let replace_all = args["replace_all"].as_bool().unwrap_or(false);
    let path = resolve_safe(path_str)?;

    let original = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("Cannot read '{}': {e}", path.display()))?;

    let count = if replace_all {
        original.matches(old_string).count()
    } else if original.contains(old_string) {
        1
    } else {
        0
    };

    if count == 0 {
        return Err(format!("String not found in '{}': {}", path.display(), old_string));
    }

    if !replace_all && count > 1 {
        return Err(format!(
            "Found {} occurrences of '{}' in '{}'. Set replace_all=true to replace all, or make old_string more specific.",
            count, old_string, path.display()
        ));
    }

    let modified = if replace_all {
        original.replace(old_string, new_string)
    } else {
        original.replacen(old_string, new_string, 1)
    };

    tokio::fs::write(&path, &modified)
        .await
        .map_err(|e| format!("Cannot write '{}': {e}", path.display()))?;

    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "replacements": count,
        "replace_all": replace_all,
    }))
}

// ═══════════════════ SEARCH TOOLS ═══════════════════

async fn grep(args: Value) -> Result<Value, String> {
    let pattern = args["pattern"].as_str().ok_or("Missing 'pattern'")?;
    let path_str = args["path"].as_str().unwrap_or(".");
    let _path = resolve_safe(path_str)?;
    let case_insensitive = args["case_insensitive"].as_bool().unwrap_or(false);
    let glob_filter = args["glob"].as_str();

    let mut cmd_args: Vec<&str> = vec!["--line-number", "--no-heading", "--color=never"];
    if case_insensitive {
        cmd_args.push("--ignore-case");
    }

    let (prog, has_glob_flag) = if which_exists("rg") {
        ("rg", true)
    } else {
        ("grep", false)
    };

    let mut full_args: Vec<String> = Vec::new();
    for a in &cmd_args {
        full_args.push(a.to_string());
    }
    if let Some(g) = glob_filter {
        if has_glob_flag {
            full_args.push("--glob".to_string());
            full_args.push(g.to_string());
        }
    }
    full_args.push(pattern.to_string());
    full_args.push(path_str.to_string());

    let args_refs: Vec<&str> = full_args.iter().map(|s| s.as_str()).collect();

    let output = std::process::Command::new(prog)
        .args(&args_refs)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run {prog}: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let results: Vec<Value> = stdout
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(3, ':').collect();
            serde_json::json!({
                "file": parts.first().unwrap_or(&""),
                "line": parts.get(1).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0),
                "text": parts.get(2).unwrap_or(&""),
            })
        })
        .collect();

    Ok(serde_json::json!({
        "pattern": pattern,
        "matches": results.len(),
        "results": results,
        "stderr": if stderr.is_empty() { Value::Null } else { Value::String(stderr) },
    }))
}

async fn glob_search(args: Value) -> Result<Value, String> {
    let pattern = args["pattern"].as_str().ok_or("Missing 'pattern'")?;
    let path_str = args["path"].as_str().unwrap_or(".");
    let _ = resolve_safe(path_str)?; // validate path

    // Build the glob pattern
    let glob_pattern = format!("{path_str}/{pattern}");

    // Native Rust glob (fast, no subprocess)
    let mut files: Vec<Value> = Vec::with_capacity(128);

    match glob::glob(&glob_pattern) {
        Ok(paths) => {
            for entry in paths.flatten() {
                // Only return files, not directories
                match std::fs::metadata(&entry) {
                    Ok(meta) if meta.is_file() => {
                        let mtime = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs_f64())
                            .unwrap_or(0.0);

                        files.push(serde_json::json!({
                            "path": entry.display().to_string(),
                            "mtime": mtime,
                        }));
                    }
                    _ => {}
                }
            }
        }
        Err(e) => {
            // Fall back to find subprocess on glob pattern error
            let output = std::process::Command::new("find")
                .args([path_str, "-name", pattern, "-type", "f", "-printf", "%T@ %p\\n"])
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output()
                .map_err(|_e| format!("Glob error: {e}"))?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            files = stdout
                .lines()
                .filter(|l| !l.is_empty())
                .map(|line| {
                    let parts: Vec<&str> = line.splitn(2, ' ').collect();
                    serde_json::json!({
                        "path": parts.get(1).unwrap_or(&""),
                        "mtime": parts.first().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0),
                    })
                })
                .collect();
        }
    }

    // Sort by mtime descending (newest first)
    files.sort_by(|a, b| {
        b["mtime"].as_f64().unwrap_or(0.0)
            .partial_cmp(&a["mtime"].as_f64().unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Return max 100
    let total = files.len();
    files.truncate(100);

    Ok(serde_json::json!({
        "pattern": pattern,
        "path": path_str,
        "results": total,
        "returned": files.len(),
        "files": files,
    }))
}

// ═══════════════════ EXECUTION TOOLS ═══════════════════

fn code_allowed() -> bool {
    std::env::var("ALLOW_CODE")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
}

async fn code_run(args: Value) -> Result<Value, String> {
    if !code_allowed() {
        return Err("ALLOW_CODE=1 is required for code execution. Set it in the MCP server environment.".into());
    }

    let language = args["language"].as_str().ok_or("Missing 'language' (e.g. 'python', 'bash', 'node')")?;
    let code = args["code"].as_str().ok_or("Missing 'code'")?;
    let timeout = args["timeout"].as_u64().unwrap_or(30).min(300).max(1);
    let cwd_str = args["cwd"].as_str();
    let cwd = cwd_str.map(|c| resolve_safe(c)).transpose()?;

    let (ext, interpreter): (&str, &str) = match language {
        "python" | "py" => ("py", "python3"),
        "bash" | "sh" => ("sh", "bash"),
        "node" | "javascript" | "js" => ("js", "node"),
        "ruby" | "rb" => ("rb", "ruby"),
        "perl" | "pl" => ("pl", "perl"),
        "php" => ("php", "php"),
        _ => return Err(format!("Unsupported language: {language}. Supported: python, bash, node, ruby, perl, php")),
    };

    let tmp_dir = std::env::temp_dir().join("deskmcp_code");
    std::fs::create_dir_all(&tmp_dir)
        .map_err(|e| format!("Cannot create temp dir: {e}"))?;

    let file_path = tmp_dir.join(format!("code_{}.{}", std::process::id(), ext));
    tokio::fs::write(&file_path, code)
        .await
        .map_err(|e| format!("Cannot write temp file: {e}"))?;

    let (stdout, stderr, exit_code) = run_cmd(
        interpreter,
        &[file_path.to_str().unwrap_or("")],
        timeout,
        cwd.as_deref(),
    )?;

    // Clean up temp file
    let _ = tokio::fs::remove_file(&file_path).await;

    Ok(serde_json::json!({
        "language": language,
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "timeout": timeout,
    }))
}

async fn code_lint(args: Value) -> Result<Value, String> {
    let path_str = args["path"].as_str().or(args["file"].as_str()).ok_or("Missing 'path'")?;
    let path = resolve_safe(path_str)?;

    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let (linter, args_vec): (&str, Vec<&str>) = match ext {
        "rs" => ("cargo", vec!["clippy", "--message-format=json"]),
        "py" => ("ruff", vec!["check", "--output-format=json"]),
        "js" | "ts" | "jsx" | "tsx" => ("npx", vec!["eslint", "--format=json"]),
        "sh" | "bash" => ("shellcheck", vec!["-f", "json"]),
        "go" => ("go", vec!["vet"]),
        _ => return Err(format!("No linter configured for '.{ext}' files. Supported: .rs, .py, .js/.ts, .sh, .go")),
    };

    let cwd = if ext == "rs" {
        path.parent().map(|p| p.to_path_buf())
    } else {
        None
    };

    let file_arg = path.display().to_string();
    let mut final_args: Vec<&str> = args_vec.clone();

    if ext != "rs" {
        final_args.push(&file_arg);
    }

    let (stdout, stderr, exit_code) = run_cmd(
        linter,
        &final_args,
        60,
        cwd.as_deref(),
    ).unwrap_or_else(|e| {
        (String::new(), e, -1)
    });

    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "linter": linter,
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    }))
}

async fn code_build(args: Value) -> Result<Value, String> {
    let path_str = args["path"].as_str().ok_or("Missing 'path' (project directory)")?;
    let path = resolve_safe(path_str)?;
    let command = args["command"].as_str().unwrap_or("auto");

    let (cmd, build_args): (&str, Vec<&str>) = if command == "auto" {
        if path.join("Cargo.toml").exists() {
            ("cargo", vec!["build", "--message-format=json"])
        } else if path.join("package.json").exists() {
            ("npm", vec!["run", "build"])
        } else if path.join("Makefile").exists() || path.join("makefile").exists() {
            ("make", vec![])
        } else if path.join("go.mod").exists() {
            ("go", vec!["build", "./..."])
        } else if path.join("setup.py").exists() || path.join("pyproject.toml").exists() {
            ("python3", vec!["-m", "compileall", "."])
        } else {
            return Err("Could not auto-detect build system. Use 'command' parameter for custom build.".into());
        }
    } else {
        let parts: Vec<&str> = command.split_whitespace().collect();
        let prog = parts.first().ok_or("Empty command")?;
        let rest: Vec<&str> = parts[1..].to_vec();
        (prog, rest)
    };

    let timeout = args["timeout"].as_u64().unwrap_or(120).min(300).max(5);

    let (stdout, stderr, exit_code) = run_cmd(
        cmd,
        &build_args,
        timeout,
        Some(&path),
    )?;

    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "command": format!("{} {}", cmd, build_args.join(" ")),
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    }))
}

fn which_exists(name: &str) -> bool {
    which::which(name).is_ok()
}
