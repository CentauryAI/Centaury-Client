use std::path::PathBuf;
use std::process::Command;

/// $KORE_DIR (env) or ~/.kore — mirrors kore-client config.rs.
fn kore_dir() -> Result<PathBuf, String> {
    if let Ok(d) = std::env::var("KORE_DIR") {
        return Ok(PathBuf::from(d));
    }
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".kore"))
        .ok_or_else(|| "no HOME dir".into())
}

// ponytail: unix-only PATH probe; add .exe suffix when a Windows build exists
fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

#[derive(serde::Serialize)]
struct ToolHit {
    name: String,
    found: bool,
}

/// Which of the 12 supported tool CLIs are installed. Mirrors kore-client's
/// SUPPORTED_TOOLS + tool_binary (cursor's binary is cursor-agent).
#[tauri::command]
fn detect_tools() -> Vec<ToolHit> {
    const TOOLS: &[(&str, &str)] = &[
        ("claude", "claude"),
        ("gemini", "gemini"),
        ("antigravity", "antigravity"),
        ("codex", "codex"),
        ("cursor", "cursor-agent"),
        ("copilot", "copilot"),
        ("kimi", "kimi"),
        ("pi", "pi"),
        ("omp", "omp"),
        ("opencode", "opencode"),
        ("kilo", "kilo"),
        ("cline", "cline"),
    ];
    TOOLS
        .iter()
        .map(|(name, bin)| ToolHit {
            name: name.to_string(),
            found: on_path(bin),
        })
        .collect()
}

/// Materialize the app's instance token at $KORE_DIR/<project>/token — the
/// exact file `kore-client human` writes, so the CLI acts as the logged-in
/// human (one shared identity, see spawn_launch).
fn write_project_token(project: &str, token: &str) -> Result<(), String> {
    let proj_dir = kore_dir()?.join(project);
    std::fs::create_dir_all(&proj_dir).map_err(|e| e.to_string())?;
    std::fs::write(proj_dir.join("token"), token).map_err(|e| e.to_string())
}

/// kore-client binary: bundled sidecar first (tauri externalBin lands it next
/// to the app exe — app-only users never install the CLI, C1), PATH fallback
/// for dev shells.
fn kore_bin() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let sib = exe.with_file_name("kore-client");
        if sib.exists() {
            return sib;
        }
    }
    "kore-client".into()
}

fn run_kore(cmd: &mut Command) -> Result<String, String> {
    let out = cmd
        .output()
        .map_err(|e| format!("kore-client not found on PATH? {e}"))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if out.status.success() {
        Ok(text)
    } else {
        Err(if text.trim().is_empty() {
            format!("kore-client failed ({})", out.status)
        } else {
            text
        })
    }
}

/// Kill an agent (DU-D6): shells to `kore-client kill <name>` — server row
/// delete + WS kick + name tombstone + SIGTERM of the locally recorded pid
/// (launch wrote agents/<name>.pid). Reuse over reimplementing the pid-file
/// dance. No --go needed: the app shell has no KORE_NAME (D10 gates agents,
/// not humans).
#[tauri::command]
fn kill_agent(
    server: String,
    token: String,
    project: String,
    name: String,
) -> Result<String, String> {
    write_project_token(&project, &token)?;
    let mut cmd = Command::new(kore_bin());
    cmd.arg("kill")
        .arg(&name)
        .env("KORE_SERVER_URL", &server)
        .env("KORE_PROJECT", &project);
    run_kore(&mut cmd)
}

#[derive(serde::Deserialize)]
struct LaunchOpts {
    server: String,
    token: String,
    project: String,
    tool: String,
    count: u8,
    tag: Option<String>,
    /// None = the launcher's account owns the agent (server default, DU-S2);
    /// a name DELEGATES to another org account (server-validated).
    owner: Option<String>,
    headless: bool,
    dir: Option<String>,
    args: Vec<String>,
}

/// Summon agents by shelling to `kore-client launch` (DU-D1: reuse the CLI's
/// launcher + 12-dialect hook installer, never reimplement them here). The
/// app's instance token is materialized at $KORE_DIR/<project>/token — the
/// exact file `kore-client human` writes — so launch authenticates as the
/// logged-in human and the CLI and app share one identity.
#[tauri::command]
fn spawn_launch(opts: LaunchOpts) -> Result<String, String> {
    write_project_token(&opts.project, &opts.token)?;

    let mut cmd = Command::new(kore_bin());
    cmd.arg("launch")
        .arg("--tool")
        .arg(&opts.tool)
        .arg("--project")
        .arg(&opts.project)
        .arg("--count")
        .arg(opts.count.to_string());
    if let Some(t) = opts.tag.as_deref().filter(|t| !t.is_empty()) {
        cmd.arg("--tag").arg(t);
    }
    if let Some(o) = opts.owner.as_deref().filter(|o| !o.is_empty()) {
        cmd.arg("--owner").arg(o);
    }
    if opts.headless {
        // --stay (DU-C1, live-verified): persistent background agent — Stop
        // hook waits for messages between turns instead of exiting after one.
        cmd.arg("--headless").arg("--stay");
    } else {
        cmd.arg("--terminal"); // no value = auto-detect (kitty|wezterm|tmux|foot)
    }
    if !opts.args.is_empty() {
        cmd.arg("--");
        cmd.args(&opts.args);
    }
    if let Some(d) = opts.dir.as_deref().filter(|d| !d.is_empty()) {
        cmd.current_dir(d);
    }
    cmd.env("KORE_SERVER_URL", &opts.server)
        .env("KORE_PROJECT", &opts.project);
    run_kore(&mut cmd)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // http plugin: frontend fetches the kore server through Rust → no CORS wall.
        .plugin(tauri_plugin_http::init())
        // ws plugin: sets the Bearer header the server requires (browser WS can't).
        .plugin(tauri_plugin_websocket::init())
        // C5: updater (About.tsx drives check/install) + process (relaunch after install).
        // Registered in setup because both are desktop-only target deps.
        .setup(|app| {
            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
                app.handle().plugin(tauri_plugin_process::init())?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            detect_tools,
            spawn_launch,
            kill_agent
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
