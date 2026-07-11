//! Minimal TUI: agent roster + live message feed + send line, for the bubble
//! (project) of the stored identity.
//!
//! Two threads: this sync UI loop (ratatui + crossterm poll), and the tokio
//! side (`net_task`) doing HTTP/WS. They talk over channels — the UI never
//! blocks on the network.
//!
//! The WS connects with peek=true: observe-only, so the TUI never steals
//! messages/watermark from the identity's real listener or hooks.

use std::collections::HashSet;
use std::sync::mpsc as std_mpsc;

use crossterm::event::{
    Event, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use kore_protocol::api::{Delivery, InstanceSummary};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use tokio::sync::mpsc as tokio_mpsc;

use crate::config;

// ---- Themes: two independent axes, palette (colour) + style (glyphs/border) ----

/// A colour scheme. `bg` is `Color::Reset` for dark themes (keep the terminal
/// background) and a real colour for light — the only theme that must paint.
#[derive(Clone, Copy)]
struct Palette {
    name: &'static str,
    fg: Color,
    fg_dim: Color,
    fg_dark: Color,
    blue: Color,
    cyan: Color,
    green: Color,
    yellow: Color,
    orange: Color,
    magenta: Color,
    selection: Color,
    bg: Color,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

/// Cycled with F2. First = Tokyo Night, the original legacy palette.
const PALETTES: [Palette; 4] = [
    Palette {
        name: "tokyo",
        fg: rgb(192, 202, 245),
        fg_dim: rgb(86, 95, 137),
        fg_dark: rgb(59, 66, 97),
        blue: rgb(122, 162, 247),
        cyan: rgb(125, 207, 255),
        green: rgb(158, 206, 106),
        yellow: rgb(224, 175, 104),
        orange: rgb(255, 158, 100),
        magenta: rgb(187, 154, 247),
        selection: rgb(40, 44, 67),
        bg: Color::Reset,
    },
    Palette {
        name: "gruvbox",
        fg: rgb(235, 219, 178),
        fg_dim: rgb(168, 153, 132),
        fg_dark: rgb(102, 92, 84),
        blue: rgb(131, 165, 152),
        cyan: rgb(142, 192, 124),
        green: rgb(184, 187, 38),
        yellow: rgb(250, 189, 47),
        orange: rgb(254, 128, 25),
        magenta: rgb(211, 134, 155),
        selection: rgb(60, 56, 54),
        bg: Color::Reset,
    },
    Palette {
        name: "dracula",
        fg: rgb(248, 248, 242),
        fg_dim: rgb(98, 114, 164),
        fg_dark: rgb(68, 71, 90),
        blue: rgb(189, 147, 249),
        cyan: rgb(139, 233, 253),
        green: rgb(80, 250, 123),
        yellow: rgb(241, 250, 140),
        orange: rgb(255, 184, 108),
        magenta: rgb(255, 121, 198),
        selection: rgb(68, 71, 90),
        bg: Color::Reset,
    },
    Palette {
        name: "light",
        fg: rgb(52, 59, 88),
        fg_dim: rgb(120, 128, 160),
        fg_dark: rgb(160, 168, 194),
        blue: rgb(52, 84, 138),
        cyan: rgb(15, 117, 140),
        green: rgb(56, 116, 49),
        yellow: rgb(143, 94, 7),
        orange: rgb(181, 85, 20),
        magenta: rgb(92, 74, 140),
        selection: rgb(210, 214, 230),
        bg: rgb(225, 226, 235),
    },
];

/// Border treatment for the two body panels.
#[derive(Clone, Copy, PartialEq, Debug)]
enum BorderKind {
    None,
    Rounded,
    Ascii,
}

/// A glyph set: icons + separator + border. Cycled with F3. The ASCII style
/// is the fallback for terminals without box-drawing / unicode symbols.
#[derive(Clone, Copy)]
struct Glyphs {
    name: &'static str,
    active: &'static str,
    idle: &'static str,
    caret: &'static str,
    block: &'static str,
    sep: &'static str,
    hsep: &'static str,
    human: &'static str,
    bullet: &'static str,
    up: &'static str,
    check: &'static str,
    border: BorderKind,
}

const STYLES: [Glyphs; 3] = [
    Glyphs {
        name: "unicode",
        active: "●",
        idle: "○",
        caret: "❯",
        block: "▏",
        sep: "│",
        hsep: "─",
        human: "✦",
        bullet: "·",
        up: "↑",
        check: "✓",
        border: BorderKind::None,
    },
    Glyphs {
        name: "rounded",
        active: "●",
        idle: "○",
        caret: "❯",
        block: "▏",
        sep: "│",
        hsep: "─",
        human: "✦",
        bullet: "·",
        up: "↑",
        check: "✓",
        border: BorderKind::Rounded,
    },
    Glyphs {
        name: "ascii",
        active: "*",
        idle: "o",
        caret: ">",
        block: "|",
        sep: "|",
        hsep: "-",
        human: "+",
        bullet: "-",
        up: "^",
        check: "x",
        border: BorderKind::Ascii,
    },
];

const ASCII_BORDER: border::Set = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

/// Where the agent panel sits relative to the message feed. Third theme axis,
/// cycled with F4. Pure geometry — the panels' contents don't change.
#[derive(Clone, Copy, PartialEq)]
enum LayoutKind {
    /// Agents in a left sidebar, messages right (the legacy default).
    SidebarLeft,
    /// Mirror: messages left, agents in a right sidebar.
    SidebarRight,
    /// Agents in a short top strip, messages fill the rest.
    Stacked,
}

const LAYOUTS: [(LayoutKind, &str); 3] = [
    (LayoutKind::SidebarLeft, "left"),
    (LayoutKind::SidebarRight, "right"),
    (LayoutKind::Stacked, "stack"),
];

/// The composed active theme. Session-local (no config file — cycle presets,
/// user decision 2026-07-06); indices reset each launch.
struct Theme {
    pal_i: usize,
    style_i: usize,
}

impl Theme {
    fn pal(&self) -> &Palette {
        &PALETTES[self.pal_i]
    }
    fn gl(&self) -> &Glyphs {
        &STYLES[self.style_i]
    }
    /// Base style carrying the background — every Paragraph/Block gets it so a
    /// light theme actually paints; spans keep bg unset and show it through.
    fn base(&self) -> Style {
        Style::default().fg(self.pal().fg).bg(self.pal().bg)
    }
    fn dim(&self) -> Style {
        Style::default().fg(self.pal().fg_dim)
    }
    fn dark(&self) -> Style {
        Style::default().fg(self.pal().fg_dark)
    }
    fn title(&self) -> Style {
        Style::default().fg(self.pal().blue).add_modifier(Modifier::BOLD)
    }
}

enum UiEvent {
    Roster(Vec<InstanceSummary>),
    /// Org project names (GET /v1/projects, once at startup) — feeds the
    /// launch form's Project select.
    Projects(Vec<String>),
    Message(Delivery),
    Info(String),
    Error(String),
}

enum Cmd {
    Send(String),
    Kill(String),
    Retag(String, Option<String>),
}

/// Which pane the keyboard drives when no overlay is open.
#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Input,
    Roster,
}

/// Modal overlays rendered in the input row. `None` = the normal input line.
enum Overlay {
    None,
    Launch(LaunchForm),
    /// Kill confirmation for these names (y/n).
    Confirm(Vec<String>),
    /// Retag prompt: apply `buf` (empty = clear) to these names.
    Retag(Vec<String>, String),
}

const ROSTER_REFRESH: std::time::Duration = std::time::Duration::from_secs(5);
const MAX_FEED: usize = 500;

// ---- Tab launch form (legacy tui/render/launch.rs feel) ----

const TOOLS: [&str; 7] = ["claude", "codex", "gemini", "antigravity", "opencode", "kilo", "cline"];
// "split" divides the terminal the TUI runs in (legacy behavior); the rest
// open a new window. launch errors if the current terminal can't split.
const TERMINALS: [&str; 6] = ["split", "auto", "kitty", "wezterm", "tmux", "foot"];
const FORM_FIELDS: usize = 7; // Tool, Count, Tag, Headless, Terminal, Project, Owner

/// Inline launch form state, opened with Ctrl-N. Project and Owner are
/// SELECTS (←/→ cycles real options), never free text (owner directive
/// 2026-07-07): typos can't invent projects or unbacked owners — index 0 =
/// the default (current project / you), other entries come from the server.
struct LaunchForm {
    tool: usize,
    count: u8,
    tag: String,
    headless: bool,
    terminal: usize,
    /// "(current)" + the org's projects (UiEvent::Projects).
    projects: Vec<String>,
    project_i: usize,
    /// "you" + the other humans in the roster (delegation targets — every
    /// human instance is account-backed since DU-S2).
    owners: Vec<String>,
    owner_i: usize,
    field: usize,
}

impl LaunchForm {
    fn new(owners: Vec<String>, projects: Vec<String>) -> Self {
        Self {
            tool: 0,
            count: 1,
            tag: String::new(),
            headless: false,
            terminal: 0,
            projects,
            project_i: 0,
            owners,
            owner_i: 0,
            field: 0,
        }
    }

    /// Free-text fields: only Tag now (Project/Owner are selects).
    fn text_buf(&mut self) -> Option<&mut String> {
        match self.field {
            2 => Some(&mut self.tag),
            _ => None,
        }
    }

    /// ←/→ on the selected field: cycle choice, bump count, toggle headless.
    fn cycle(&mut self, dir: i32) {
        let step = |i: usize, len: usize| (i as i32 + dir).rem_euclid(len as i32) as usize;
        match self.field {
            0 => self.tool = step(self.tool, TOOLS.len()),
            1 => self.count = (self.count as i32 + dir).clamp(1, 9) as u8,
            3 => self.headless = !self.headless,
            4 => self.terminal = step(self.terminal, TERMINALS.len()),
            5 => self.project_i = step(self.project_i, self.projects.len().max(1)),
            6 => self.owner_i = step(self.owner_i, self.owners.len().max(1)),
            _ => {}
        }
    }

    /// The detached `kore-client launch ...` argv this form describes.
    fn argv(&self) -> Vec<String> {
        let mut v = vec!["launch".to_string(), "--tool".into(), TOOLS[self.tool].into()];
        // Tag = legacy group label, NOT the agent's name: names stay random,
        // the tag is a separate column shown as "{tag}-{name}".
        if !self.tag.is_empty() {
            v.extend(["--tag".into(), self.tag.clone()]);
        }
        if self.count > 1 {
            v.extend(["--count".into(), self.count.to_string()]);
        }
        if self.headless {
            v.push("--headless".into());
        } else {
            v.extend(["--terminal".into(), TERMINALS[self.terminal].into()]);
        }
        // Index 0 = launch defaults: KORE_PROJECT env (else "default"), owner
        // = the caller (server derives it from the token, DU-S2).
        if self.project_i > 0 {
            v.extend(["--project".into(), self.projects[self.project_i].clone()]);
        }
        if self.owner_i > 0 {
            v.extend(["--owner".into(), self.owners[self.owner_i].clone()]);
        }
        v
    }
}

pub async fn run() -> anyhow::Result<()> {
    // No identity → plain-prompt login/join wizard before ratatui takes the
    // screen. ponytail: stdin prompts; ratatui form when someone asks.
    if config::instance_name().is_err() {
        login_wizard().await?;
    }
    let me = config::instance_name()?;

    let (ui_tx, ui_rx) = std_mpsc::channel::<UiEvent>();
    let (cmd_tx, cmd_rx) = tokio_mpsc::unbounded_channel::<Cmd>();

    let net = tokio::spawn(net_task(ui_tx, cmd_rx));
    let ui = tokio::task::spawn_blocking(move || ui_loop(&me, ui_rx, cmd_tx));
    let result = ui.await?;
    net.abort();
    result
}

fn prompt_line(prompt: &str) -> anyhow::Result<String> {
    use std::io::Write;
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

/// GET with the session (login) token — the browse-before-join routes.
async fn user_get<T: serde::de::DeserializeOwned>(session: &str, path: &str) -> anyhow::Result<T> {
    let resp = config::http_client()
        .get(format!("{}{path}", config::server_url()))
        .bearer_auth(session)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("request failed ({}): {}", resp.status(), resp.text().await?);
    }
    Ok(resp.json().await?)
}

/// login (if needed) → pick/create project → register-human (the network
/// name is the account's owner_name, server-assigned). Leaves a stored
/// instance token behind.
async fn login_wizard() -> anyhow::Result<()> {
    println!("no identity yet — let's set one up");
    if config::load_session().is_err() {
        let email = prompt_line("email: ")?;
        crate::commands::auth::login(email).await.map_err(|e| {
            anyhow::anyhow!("{e}\nno account? run: kore-client signup <email> --name <You>")
        })?;
    }
    let session = config::load_session()?;

    let projects: Vec<kore_protocol::api::ProjectSummary> =
        user_get(&session, "/v1/user/projects").await?;
    for (i, p) in projects.iter().enumerate() {
        println!("  {}: {} ({} agents)", i + 1, p.name, p.instance_count);
    }
    let pick = prompt_line("project (number or new name): ")?;
    let project = match pick.parse::<usize>() {
        Ok(n) if n >= 1 && n <= projects.len() => projects[n - 1].name.clone(),
        _ if !pick.is_empty() => pick,
        _ => anyhow::bail!("a project is required"),
    };

    crate::commands::auth::human(Some(project.clone())).await?;

    // The token was saved under this project; the rest of the process must
    // resolve the same path. Safe: UI/net threads don't exist yet.
    unsafe { std::env::set_var("KORE_PROJECT", &project) };
    Ok(())
}

/// Tokio side: initial roster+history, periodic roster refresh, live WS feed,
/// and sends. Every outcome goes to the UI as a UiEvent — no printing.
async fn net_task(ui_tx: std_mpsc::Sender<UiEvent>, mut cmd_rx: tokio_mpsc::UnboundedReceiver<Cmd>) {
    let send_event = |e| {
        let _ = ui_tx.send(e);
    };

    match crate::get_json::<Vec<Delivery>>("/v1/messages?limit=50").await {
        Ok(hist) => {
            for d in hist {
                send_event(UiEvent::Message(d));
            }
        }
        Err(e) => send_event(UiEvent::Error(format!("history: {e}"))),
    }

    // Once at startup: the org's projects feed the launch form's select.
    // ponytail: no refresh — reopen the TUI if a project was created mid-session.
    if let Ok(list) = crate::get_json::<Vec<kore_protocol::api::ProjectSummary>>("/v1/projects").await {
        send_event(UiEvent::Projects(list.into_iter().map(|p| p.name).collect()));
    }

    let mut roster_tick = tokio::time::interval(ROSTER_REFRESH);
    let mut ws = None; // reconnected lazily inside the loop

    loop {
        if ws.is_none() {
            match crate::ws::connect(true).await {
                Ok(s) => ws = Some(s),
                Err(e) => {
                    send_event(UiEvent::Error(format!("ws: {e} — retrying")));
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    continue;
                }
            }
        }
        let socket = ws.as_mut().unwrap();

        tokio::select! {
            _ = roster_tick.tick() => {
                match crate::get_json::<Vec<InstanceSummary>>("/v1/instances").await {
                    Ok(list) => send_event(UiEvent::Roster(list)),
                    Err(e) => send_event(UiEvent::Error(format!("roster: {e}"))),
                }
            }
            delivery = crate::ws::next_delivery(socket) => {
                match delivery {
                    Ok(Some(d)) => send_event(UiEvent::Message(d)),
                    Ok(None) | Err(_) => ws = None, // reconnect
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(Cmd::Send(text)) => send_event(do_send(text).await),
                    Some(Cmd::Kill(name)) => send_event(do_kill(name).await),
                    Some(Cmd::Retag(name, tag)) => send_event(do_retag(name, tag).await),
                    None => return, // UI gone
                }
            }
        }
    }
}

async fn do_send(text: String) -> UiEvent {
    let token = match config::load_token() {
        Ok(t) => t,
        Err(e) => return UiEvent::Error(e.to_string()),
    };
    let resp = config::http_client()
        .post(format!("{}/v1/messages", config::server_url()))
        .bearer_auth(token)
        .json(&kore_protocol::api::SendRequest {
            targets: vec![],
            text,
            intent: None,
            thread: None,
            reply_to: None,
            bundle_id: None,
            go: false,
        })
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => match r.json::<kore_protocol::api::SendResponse>().await {
            Ok(body) => UiEvent::Info(format!("sent #{} [{}]", body.id, body.scope.as_str())),
            Err(e) => UiEvent::Error(format!("send: {e}")),
        },
        Ok(r) => {
            let status = r.status();
            UiEvent::Error(format!("send {}: {}", status, r.text().await.unwrap_or_default()))
        }
        Err(e) => UiEvent::Error(format!("send: {e}")),
    }
}

/// Real kill: unregister + tombstone (server) + SIGTERM the local process.
async fn do_kill(name: String) -> UiEvent {
    let token = match config::load_token() {
        Ok(t) => t,
        Err(e) => return UiEvent::Error(e.to_string()),
    };
    let resp = config::http_client()
        .delete(format!("{}/v1/instances/{name}?kill=true", config::server_url()))
        .bearer_auth(token)
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => {
            crate::commands::launch::kill_local(&name);
            UiEvent::Info(format!("killed '{name}'"))
        }
        Ok(r) => {
            let status = r.status();
            UiEvent::Error(format!("kill {}: {}", status, r.text().await.unwrap_or_default()))
        }
        Err(e) => UiEvent::Error(format!("kill: {e}")),
    }
}

/// Retag: PATCH the agent's tag column server-side (owner-or-self gated there).
/// `None` clears the tag. Roster refresh (≤5s) reflects the new "{tag}-{name}".
async fn do_retag(name: String, tag: Option<String>) -> UiEvent {
    let token = match config::load_token() {
        Ok(t) => t,
        Err(e) => return UiEvent::Error(e.to_string()),
    };
    let resp = config::http_client()
        .patch(format!("{}/v1/instances/{name}", config::server_url()))
        .bearer_auth(token)
        .json(&kore_protocol::api::SetTagRequest { tag: tag.clone() })
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => match tag {
            Some(t) if !t.is_empty() => UiEvent::Info(format!("retagged '{name}' → {t}")),
            _ => UiEvent::Info(format!("cleared tag on '{name}'")),
        },
        Ok(r) => {
            let status = r.status();
            UiEvent::Error(format!("retag {}: {}", status, r.text().await.unwrap_or_default()))
        }
        Err(e) => UiEvent::Error(format!("retag: {e}")),
    }
}

/// One rendered feed line: message, own send confirmation, or error.
enum FeedLine {
    Msg(Delivery),
    Note(String),
}

/// All mutable UI state, so the key handlers read/write one struct.
struct Ui {
    roster: Vec<InstanceSummary>,
    /// Org project names for the launch form's Project select.
    projects: Vec<String>,
    feed: Vec<FeedLine>,
    input: String,
    scroll: usize, // lines up from the bottom of the feed
    thread_filter: Option<String>,
    mention_pick: usize, // Tab-cycle position for @mentions
    focus: Focus,
    cursor: usize, // roster row under the caret (roster focus)
    selected: HashSet<String>,
    overlay: Overlay,
    theme: Theme,
    layout_i: usize, // index into LAYOUTS
}

fn ui_loop(
    me: &str,
    ui_rx: std_mpsc::Receiver<UiEvent>,
    cmd_tx: tokio_mpsc::UnboundedSender<Cmd>,
) -> anyhow::Result<()> {
    let mut terminal = ratatui::init();
    // Mouse is a bonus layer: enable capture, but if the terminal refuses
    // (some SSH/tmux setups) swallow the error — keys keep working silently.
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture);
    let result = ui_loop_inner(me, &ui_rx, &cmd_tx, &mut terminal);
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    ratatui::restore();
    result
}

fn ui_loop_inner(
    me: &str,
    ui_rx: &std_mpsc::Receiver<UiEvent>,
    cmd_tx: &tokio_mpsc::UnboundedSender<Cmd>,
    terminal: &mut ratatui::DefaultTerminal,
) -> anyhow::Result<()> {
    let mut ui = Ui {
        roster: vec![],
        projects: vec![],
        feed: vec![],
        input: String::new(),
        scroll: 0,
        thread_filter: None,
        mention_pick: 0,
        focus: Focus::Input,
        cursor: 0,
        selected: HashSet::new(),
        overlay: Overlay::None,
        theme: Theme { pal_i: 0, style_i: 0 },
        layout_i: 0,
    };
    let mut dirty = true;

    loop {
        while let Ok(ev) = ui_rx.try_recv() {
            match ev {
                UiEvent::Roster(list) => ui.roster = list,
                UiEvent::Projects(list) => ui.projects = list,
                UiEvent::Message(d) => ui.feed.push(FeedLine::Msg(d)),
                UiEvent::Info(s) | UiEvent::Error(s) => ui.feed.push(FeedLine::Note(s)),
            }
            if ui.feed.len() > MAX_FEED {
                let excess = ui.feed.len() - MAX_FEED;
                ui.feed.drain(..excess);
            }
            dirty = true;
        }

        if dirty {
            terminal.draw(|f| draw(f, me, &ui))?;
            dirty = false;
        }

        if crossterm::event::poll(std::time::Duration::from_millis(100))? {
            match crossterm::event::read()? {
                Event::Key(key) => {
                    if key.kind != crossterm::event::KeyEventKind::Press {
                        continue;
                    }
                    if handle_key(key.code, key.modifiers, &mut ui, cmd_tx) {
                        return Ok(()); // quit
                    }
                    dirty = true;
                }
                Event::Mouse(m) => {
                    if handle_mouse(m, terminal.get_frame().area(), &mut ui, cmd_tx) {
                        return Ok(()); // a synthesized quit-button click
                    }
                    dirty = true;
                }
                _ => {}
            }
        }
    }
}

/// Central key router. Returns true to quit. Overlays capture keys first;
/// then theme hotkeys (global); then per-focus handling.
fn handle_key(
    code: KeyCode,
    mods: KeyModifiers,
    ui: &mut Ui,
    cmd_tx: &tokio_mpsc::UnboundedSender<Cmd>,
) -> bool {
    // Ctrl-C quits from anywhere.
    if let KeyCode::Char('c') = code {
        if mods.contains(KeyModifiers::CONTROL) {
            return true;
        }
    }
    // Overlays swallow everything else while open.
    if !matches!(ui.overlay, Overlay::None) {
        handle_overlay_key(code, ui, cmd_tx);
        return false;
    }
    // Global theme cycling + launch form, available in either focus.
    match code {
        KeyCode::F(2) => {
            ui.theme.pal_i = (ui.theme.pal_i + 1) % PALETTES.len();
            return false;
        }
        KeyCode::F(3) => {
            ui.theme.style_i = (ui.theme.style_i + 1) % STYLES.len();
            return false;
        }
        KeyCode::F(4) => {
            ui.layout_i = (ui.layout_i + 1) % LAYOUTS.len();
            return false;
        }
        KeyCode::Char('n') if mods.contains(KeyModifiers::CONTROL) => {
            ui.overlay = Overlay::Launch(LaunchForm::new(owner_options(ui), project_options(ui)));
            return false;
        }
        _ => {}
    }
    match ui.focus {
        Focus::Input => handle_input_key(code, ui, cmd_tx),
        Focus::Roster => handle_roster_key(code, ui),
    }
    false
}

/// Owner select options: "you" first (launch's default — the server derives
/// the owner from the caller's account, DU-S2), then the OTHER humans in the
/// roster as delegation targets (account-backed since DU-S2). Free text is
/// gone: an owner that isn't a real account can't be typed in.
fn owner_options(ui: &Ui) -> Vec<String> {
    let me = config::instance_name().unwrap_or_default();
    let mut v = vec![format!("you ({me})")];
    v.extend(
        ui.roster
            .iter()
            .filter(|i| i.kind == "human" && i.name != me)
            .map(|i| i.name.clone()),
    );
    v
}

/// Project select options: "(current)" first (launch default = KORE_PROJECT),
/// then the org's other projects from GET /v1/projects.
fn project_options(ui: &Ui) -> Vec<String> {
    let mut v = vec!["(current)".to_string()];
    v.extend(ui.projects.iter().cloned());
    v
}

/// Mouse: wheel scrolls the feed; a left click routes by region. Clicks reuse
/// the keyboard handlers (footer hints synthesize their key) so there's one
/// source of behaviour. Returns true only if a footer button synthesizes quit.
fn handle_mouse(
    m: MouseEvent,
    area: Rect,
    ui: &mut Ui,
    cmd_tx: &tokio_mpsc::UnboundedSender<Cmd>,
) -> bool {
    match m.kind {
        MouseEventKind::ScrollUp => {
            ui.scroll = (ui.scroll + 3).min(ui.feed.len().saturating_sub(1));
            return false;
        }
        MouseEventKind::ScrollDown => {
            ui.scroll = ui.scroll.saturating_sub(3);
            return false;
        }
        MouseEventKind::Down(MouseButton::Left) => {}
        _ => return false,
    }
    let (col, row) = (m.column, m.row);
    let rows = vlayout(area, ui);
    let (agents, feed) = agents_feed_rects(rows[2], ui);
    let footer = rows[5];

    // Footer hints act as buttons: find the one under the cursor and press it.
    if hit(footer, col, row) {
        let (_, hits) = footer_spans_hits(ui);
        for (start, end, code, mods) in hits {
            if col >= footer.x + start && col < footer.x + end {
                return handle_key(code, mods, ui, cmd_tx);
            }
        }
        return false;
    }
    // Click an agent row → focus roster, move the cursor there, toggle select.
    if hit(agents, col, row) {
        if let Some(idx) = agent_at_line(ui, row - agents.y) {
            ui.focus = Focus::Roster;
            ui.cursor = idx;
            let name = ui.roster[idx].name.clone();
            if !ui.selected.remove(&name) {
                ui.selected.insert(name);
            }
        }
        return false;
    }
    // Click the messages pane or input line → back to typing.
    if hit(feed, col, row) || hit(rows[4], col, row) {
        ui.focus = Focus::Input;
    }
    false
}

fn hit(r: Rect, col: u16, row: u16) -> bool {
    col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
}

/// Which roster agent occupies a rendered line inside the agents pane. Each
/// agent takes one line, plus a second when it has a status_context — mirror
/// of `draw_agents`, so clicks land on the right row.
fn agent_at_line(ui: &Ui, rel: u16) -> Option<usize> {
    let mut line = 0u16;
    for (idx, i) in ui.roster.iter().enumerate() {
        if line == rel {
            return Some(idx);
        }
        line += 1;
        if !i.status_context.is_empty() {
            if line == rel {
                return Some(idx);
            }
            line += 1;
        }
    }
    None
}

/// The six vertical regions (status/blank/body/blank/input/footer). One source
/// of geometry for both `draw` and `handle_mouse` so clicks match what's drawn.
fn vlayout(area: Rect, ui: &Ui) -> std::rc::Rc<[Rect]> {
    let overlay_h = match &ui.overlay {
        Overlay::Launch(_) => FORM_FIELDS as u16 + 1,
        _ => 1,
    };
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
            Constraint::Length(overlay_h),
            Constraint::Length(1),
        ])
        .split(area)
}

/// Outer rects of the two panels + optional separator, per the active layout
/// (F4) and border style (F3). ONE geometry source for `draw_body` and the
/// mouse hit-tester, so clicks always land where things are painted. The sep
/// is `None` when bordered (the frames divide) — otherwise a width-1 column
/// (side layouts) or height-1 row (stacked).
fn body_outer(area: Rect, ui: &Ui) -> (Rect, Rect, Option<Rect>) {
    let bordered = ui.theme.gl().border != BorderKind::None;
    let a_w = if bordered { 30 } else { 26 };
    let f_min = if bordered { 22 } else { 20 };
    let split = |dir, c: Vec<Constraint>| Layout::default().direction(dir).constraints(c).split(area);
    use Constraint::{Length, Min};
    match LAYOUTS[ui.layout_i].0 {
        LayoutKind::SidebarLeft if bordered => {
            let c = split(Direction::Horizontal, vec![Length(a_w), Min(f_min)]);
            (c[0], c[1], None)
        }
        LayoutKind::SidebarLeft => {
            let c = split(Direction::Horizontal, vec![Length(a_w), Length(1), Min(f_min)]);
            (c[0], c[2], Some(c[1]))
        }
        LayoutKind::SidebarRight if bordered => {
            let c = split(Direction::Horizontal, vec![Min(f_min), Length(a_w)]);
            (c[1], c[0], None)
        }
        LayoutKind::SidebarRight => {
            let c = split(Direction::Horizontal, vec![Min(f_min), Length(1), Length(a_w)]);
            (c[2], c[0], Some(c[1]))
        }
        // ponytail: agents strip fixed height; a long roster clips — add a
        // scroll only if that ever bites.
        LayoutKind::Stacked if bordered => {
            let c = split(Direction::Vertical, vec![Length(9), Min(3)]);
            (c[0], c[1], None)
        }
        LayoutKind::Stacked => {
            let c = split(Direction::Vertical, vec![Length(8), Length(1), Min(3)]);
            (c[0], c[2], Some(c[1]))
        }
    }
}

/// Content rects of the agents and messages panes (inner of the border when
/// bordered) — the click targets for the two panels.
fn agents_feed_rects(body: Rect, ui: &Ui) -> (Rect, Rect) {
    let (a_outer, f_outer, _) = body_outer(body, ui);
    if ui.theme.gl().border == BorderKind::None {
        (a_outer, f_outer)
    } else {
        let ab = panel_block(&ui.theme, "agents", false);
        let fb = panel_block(&ui.theme, "messages", false);
        (ab.inner(a_outer), fb.inner(f_outer))
    }
}

/// Keys while the message input is focused (the default).
fn handle_input_key(code: KeyCode, ui: &mut Ui, cmd_tx: &tokio_mpsc::UnboundedSender<Cmd>) {
    match code {
        KeyCode::Esc => { /* nothing to cancel; quit is Ctrl-C */ }
        KeyCode::PageUp => ui.scroll = (ui.scroll + 5).min(ui.feed.len().saturating_sub(1)),
        KeyCode::PageDown => ui.scroll = ui.scroll.saturating_sub(5),
        KeyCode::End => ui.scroll = 0,
        // Empty line: hand focus to the roster. With text: cycle @mentions.
        KeyCode::Tab if ui.input.is_empty() => {
            if !ui.roster.is_empty() {
                ui.focus = Focus::Roster;
                ui.cursor = ui.cursor.min(ui.roster.len() - 1);
            }
        }
        KeyCode::Tab => {
            if !ui.roster.is_empty() {
                ui.mention_pick %= ui.roster.len();
                ui.input = cycle_mention(&ui.input, &ui.roster[ui.mention_pick].name);
                ui.mention_pick += 1;
            }
        }
        KeyCode::Enter => {
            let text = ui.input.trim().to_string();
            ui.input.clear();
            if let Some(cmd) = text.strip_prefix('/') {
                handle_slash(cmd, &mut ui.thread_filter, &mut ui.feed, cmd_tx);
            } else if !text.is_empty() {
                let _ = cmd_tx.send(Cmd::Send(text));
            }
            ui.scroll = 0; // acting = back to live tail
        }
        KeyCode::Backspace => {
            ui.input.pop();
        }
        KeyCode::Char(c) => ui.input.push(c),
        _ => {}
    }
}

/// Keys while the roster is focused: navigate, multi-select, act.
fn handle_roster_key(code: KeyCode, ui: &mut Ui) {
    let len = ui.roster.len();
    if len == 0 {
        ui.focus = Focus::Input;
        return;
    }
    ui.cursor = ui.cursor.min(len - 1);
    match code {
        KeyCode::Esc | KeyCode::Tab => ui.focus = Focus::Input,
        KeyCode::Down | KeyCode::Char('j') => ui.cursor = (ui.cursor + 1) % len,
        KeyCode::Up | KeyCode::Char('J') => ui.cursor = (ui.cursor + len - 1) % len,
        KeyCode::Char(' ') => {
            let name = ui.roster[ui.cursor].name.clone();
            if !ui.selected.remove(&name) {
                ui.selected.insert(name);
            }
        }
        KeyCode::Char('k') => {
            let targets = self_or_selected(ui);
            if !targets.is_empty() {
                ui.overlay = Overlay::Confirm(targets);
            }
        }
        KeyCode::Char('t') => {
            let targets = self_or_selected(ui);
            if !targets.is_empty() {
                ui.overlay = Overlay::Retag(targets, String::new());
            }
        }
        // Drop the cursor agent into the input as an @mention, back to typing.
        KeyCode::Enter => {
            ui.input = cycle_mention(&ui.input, &ui.roster[ui.cursor].name);
            ui.focus = Focus::Input;
        }
        KeyCode::PageUp => ui.scroll = (ui.scroll + 5).min(ui.feed.len().saturating_sub(1)),
        KeyCode::PageDown => ui.scroll = ui.scroll.saturating_sub(5),
        _ => {}
    }
}

/// Action targets: the multi-selection if any, else just the cursor agent.
fn self_or_selected(ui: &Ui) -> Vec<String> {
    if ui.selected.is_empty() {
        vec![ui.roster[ui.cursor].name.clone()]
    } else {
        // Keep roster order for a stable prompt.
        ui.roster.iter().map(|i| i.name.clone()).filter(|n| ui.selected.contains(n)).collect()
    }
}

/// Keys while an overlay (launch form / kill confirm / retag prompt) is open.
fn handle_overlay_key(code: KeyCode, ui: &mut Ui, cmd_tx: &tokio_mpsc::UnboundedSender<Cmd>) {
    match &mut ui.overlay {
        Overlay::Launch(form) => {
            if handle_form_key(code, form, &mut ui.feed) {
                ui.overlay = Overlay::None;
            }
        }
        Overlay::Confirm(targets) => match code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                for name in targets.drain(..) {
                    let _ = cmd_tx.send(Cmd::Kill(name));
                }
                ui.selected.clear();
                ui.overlay = Overlay::None;
            }
            _ => ui.overlay = Overlay::None, // any other key = cancel
        },
        Overlay::Retag(targets, buf) => match code {
            KeyCode::Esc => ui.overlay = Overlay::None,
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Enter => {
                let tag = if buf.trim().is_empty() { None } else { Some(buf.trim().to_string()) };
                for name in targets.drain(..) {
                    let _ = cmd_tx.send(Cmd::Retag(name, tag.clone()));
                }
                ui.selected.clear();
                ui.overlay = Overlay::None;
            }
            KeyCode::Char(c) => buf.push(c),
            _ => {}
        },
        Overlay::None => {}
    }
}

/// Keys while the launch form is open. Returns true when the form closes.
fn handle_form_key(code: KeyCode, form: &mut LaunchForm, feed: &mut Vec<FeedLine>) -> bool {
    match code {
        KeyCode::Esc => return true,
        KeyCode::Up => form.field = form.field.checked_sub(1).unwrap_or(FORM_FIELDS - 1),
        KeyCode::Down | KeyCode::Tab => form.field = (form.field + 1) % FORM_FIELDS,
        KeyCode::Left => form.cycle(-1),
        KeyCode::Right | KeyCode::Char(' ') if form.text_buf().is_none() => form.cycle(1),
        KeyCode::Backspace => {
            if let Some(buf) = form.text_buf() {
                buf.pop();
            }
        }
        KeyCode::Char(c) => {
            if let Some(buf) = form.text_buf() {
                buf.push(c);
            }
        }
        KeyCode::Enter => {
            feed.push(FeedLine::Note(run_form_launch(form)));
            return true;
        }
        _ => {}
    }
    false
}

/// Run `kore-client launch ...` for the form. Both --terminal and --headless
/// spawn-and-return, so a blocking .output() here is milliseconds.
fn run_form_launch(form: &LaunchForm) -> String {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "kore-client".into());
    match std::process::Command::new(exe).args(form.argv()).output() {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim().replace('\n', " · ")
        }
        Ok(out) => format!("launch failed: {}", String::from_utf8_lossy(&out.stderr).trim()),
        Err(e) => format!("launch failed: {e}"),
    }
}

/// `/t [thread]`, `/kill <name>`, `/launch <name>` — everything else
/// prints usage. Slash commands are a text path to the same actions.
fn handle_slash(
    cmd: &str,
    thread_filter: &mut Option<String>,
    feed: &mut Vec<FeedLine>,
    cmd_tx: &tokio_mpsc::UnboundedSender<Cmd>,
) {
    let mut parts = cmd.split_whitespace();
    let note = match parts.next() {
        Some("t") => {
            *thread_filter = parts.next().map(String::from);
            match thread_filter {
                Some(t) => format!("thread filter: {t}"),
                None => "thread filter cleared".into(),
            }
        }
        Some("kill") => match parts.next() {
            Some(name) => {
                let _ = cmd_tx.send(Cmd::Kill(name.to_string()));
                return;
            }
            None => "usage: /kill <name>".into(),
        },
        Some("launch") => match parts.next() {
            Some(name) => launch_note(name, parts.next()),
            None => "usage: /launch <name>".into(),
        },
        _ => "commands: /t [thread] · /kill <name> · /launch <name>".into(),
    };
    feed.push(FeedLine::Note(note));
}

/// Spawn `kore-client launch --terminal` detached — the TUI owns this screen,
/// so new agents get their own window (same trick as the legacy TUI).
fn launch_note(name: &str, project: Option<&str>) -> String {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "kore-client".into());
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["launch", "--name", name, "--terminal"]);
    if let Some(p) = project {
        cmd.args(["--project", p]);
    }
    cmd.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    match cmd.spawn() {
        Ok(_) => format!("launching '{name}' in a new terminal window"),
        Err(e) => format!("launch failed: {e}"),
    }
}

/// Tab-cycle: replace the leading @mention (or prepend one), keep the rest.
fn cycle_mention(input: &str, name: &str) -> String {
    let rest = match input.strip_prefix('@') {
        Some(r) => r.split_once(' ').map(|(_, tail)| tail).unwrap_or(""),
        None => input,
    };
    format!("@{name} {rest}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_ui(roster: Vec<InstanceSummary>) -> Ui {
        Ui {
            roster,
            projects: vec![],
            feed: vec![],
            input: String::new(),
            scroll: 0,
            thread_filter: None,
            mention_pick: 0,
            focus: Focus::Input,
            cursor: 0,
            selected: HashSet::new(),
            overlay: Overlay::None,
            theme: Theme { pal_i: 0, style_i: 0 },
            layout_i: 0,
        }
    }

    fn agent(name: &str, ctx: &str) -> InstanceSummary {
        InstanceSummary {
            name: name.into(),
            tag: None,
            status: "active".into(),
            kind: "agent".into(),
            owner: None,
            tool: Some("claude".into()),
            directory: None,
            status_context: ctx.into(),
            last_seen_msg_id: 0,
        }
    }

    #[test]
    fn agent_at_line_accounts_for_status_context_rows() {
        // luna has a "doing" line (2 rows), nova doesn't (1 row).
        let ui = test_ui(vec![agent("luna", "building"), agent("nova", "")]);
        assert_eq!(agent_at_line(&ui, 0), Some(0)); // luna name
        assert_eq!(agent_at_line(&ui, 1), Some(0)); // luna doing
        assert_eq!(agent_at_line(&ui, 2), Some(1)); // nova name
        assert_eq!(agent_at_line(&ui, 3), None); // past the end
    }

    #[test]
    fn footer_hits_are_clickable_ordered_and_within_render() {
        let ui = test_ui(vec![]); // Input focus
        let (spans, hits) = footer_spans_hits(&ui);
        let rendered: u16 = spans.iter().map(|s| s.content.chars().count() as u16).sum();
        assert!(!hits.is_empty());
        let mut prev_end = 0;
        for (start, end, _, _) in &hits {
            assert!(start < end, "empty hit range");
            assert!(*start >= prev_end, "hit ranges overlap/out of order");
            assert!(*end <= rendered, "hit range exceeds rendered width");
            prev_end = *end;
        }
    }

    #[test]
    fn cycle_mention_replaces_leading_target_only() {
        assert_eq!(cycle_mention("", "luna"), "@luna ");
        assert_eq!(cycle_mention("fix the build", "luna"), "@luna fix the build");
        assert_eq!(cycle_mention("@luna fix the build", "nova"), "@nova fix the build");
        assert_eq!(cycle_mention("@luna", "nova"), "@nova ");
    }

    #[test]
    fn launch_form_cycles_and_builds_argv() {
        let mut f = LaunchForm::new(
            vec!["you (me)".into(), "boss".into()],
            vec!["(current)".into(), "research".into()],
        );
        f.cycle(-1); // tool wraps backwards
        assert_eq!(TOOLS[f.tool], "cline");
        f.field = 1;
        f.cycle(-1); // count clamps at 1
        assert_eq!(f.count, 1);
        f.cycle(1);
        f.cycle(1);
        assert_eq!(f.count, 3);
        f.field = 3;
        f.cycle(1); // headless toggles
        assert!(f.headless);

        f.tag = "team".into();
        let argv = f.argv();
        assert_eq!(
            argv,
            ["launch", "--tool", "cline", "--tag", "team", "--count", "3", "--headless"]
        );
        f.headless = false;
        // default terminal option is "split" — divide the TUI's own terminal
        assert!(f.argv().ends_with(&["--terminal".to_string(), "split".to_string()]));

        // Project/Owner are SELECTS: index 0 = defaults (nothing in argv);
        // cycling to a real entry rides into the argv; free text is impossible.
        f.field = 5;
        f.cycle(1);
        assert!(f.argv().contains(&"research".to_string()));
        f.field = 6;
        f.cycle(1);
        assert!(f.argv().ends_with(&["--owner".to_string(), "boss".to_string()]));
        f.cycle(1); // wraps back to "you" → --owner gone (server default = caller)
        assert!(!f.argv().iter().any(|a| a == "--owner"));
    }

    #[test]
    fn themes_have_every_axis() {
        // Three cycle axes non-empty and the composed theme indexes safely.
        assert_eq!(PALETTES.len(), 4);
        assert_eq!(STYLES.len(), 3);
        assert_eq!(LAYOUTS.len(), 3);
        let th = Theme { pal_i: PALETTES.len() - 1, style_i: STYLES.len() - 1 };
        assert_eq!(th.pal().name, "light");
        assert_eq!(th.gl().name, "ascii");
        assert_eq!(th.gl().border, BorderKind::Ascii);
    }

    #[test]
    fn every_layout_places_two_disjoint_panels() {
        // Each layout must yield non-empty agents/feed rects that don't overlap,
        // in both borderless and bordered styles — the mouse relies on it.
        let area = Rect::new(0, 0, 80, 24);
        for style_i in 0..STYLES.len() {
            for layout_i in 0..LAYOUTS.len() {
                let mut ui = test_ui(vec![]);
                ui.theme.style_i = style_i;
                ui.layout_i = layout_i;
                let (a, fd) = agents_feed_rects(area, &ui);
                assert!(a.width > 0 && a.height > 0, "empty agents {style_i}/{layout_i}");
                assert!(fd.width > 0 && fd.height > 0, "empty feed {style_i}/{layout_i}");
                let disjoint = a.x + a.width <= fd.x
                    || fd.x + fd.width <= a.x
                    || a.y + a.height <= fd.y
                    || fd.y + fd.height <= a.y;
                assert!(disjoint, "panels overlap at {style_i}/{layout_i}");
            }
        }
    }
}

// ---- Rendering ----

/// Borderless (legacy) or bordered layout depending on the style axis: status
/// bar, blank, body (agents │ messages), blank, input/overlay, footer.
fn draw(f: &mut ratatui::Frame, me: &str, ui: &Ui) {
    let th = &ui.theme;
    // Paint the base (matters only for the light theme; Reset elsewhere).
    f.render_widget(Block::default().style(th.base()), f.area());

    let rows = vlayout(f.area(), ui);

    draw_status_bar(f, rows[0], ui);
    draw_body(f, rows[2], me, ui);
    draw_overlay_row(f, rows[4], ui);
    draw_footer(f, rows[5], ui);
}

/// The two body panels, placed by the active layout (F4) and framed per the
/// border style (F3). Geometry comes from `body_outer` — same source the mouse
/// hit-tester uses.
fn draw_body(f: &mut ratatui::Frame, area: ratatui::layout::Rect, me: &str, ui: &Ui) {
    let th = &ui.theme;
    let (a_outer, f_outer, sep) = body_outer(area, ui);
    if th.gl().border == BorderKind::None {
        draw_agents(f, a_outer, ui);
        if let Some(s) = sep {
            draw_sep(f, s, th);
        }
        draw_feed(f, f_outer, me, ui);
    } else {
        let ab = panel_block(th, "agents", ui.focus == Focus::Roster);
        let fb = panel_block(th, "messages", ui.focus == Focus::Input);
        let ai = ab.inner(a_outer);
        let fi = fb.inner(f_outer);
        f.render_widget(ab, a_outer);
        f.render_widget(fb, f_outer);
        draw_agents(f, ai, ui);
        draw_feed(f, fi, me, ui);
    }
}

/// The borderless divider: a vertical rule for side layouts (width 1), a
/// horizontal rule for the stacked layout (height 1).
fn draw_sep(f: &mut ratatui::Frame, s: Rect, th: &Theme) {
    let para = if s.width <= s.height {
        let lines: Vec<Line> =
            (0..s.height).map(|_| Line::from(Span::styled(th.gl().sep, th.dark()))).collect();
        Paragraph::new(lines)
    } else {
        Paragraph::new(Line::from(Span::styled(th.gl().hsep.repeat(s.width as usize), th.dark())))
    };
    f.render_widget(para.style(th.base()), s);
}

/// A bordered panel in the active style; `active` highlights the title.
fn panel_block(th: &Theme, title: &str, active: bool) -> Block<'static> {
    let mut b = Block::default()
        .borders(Borders::ALL)
        .style(th.base())
        .border_style(if active { th.title() } else { th.dark() })
        .title(Span::styled(format!(" {title} "), if active { th.title() } else { th.dim() }));
    b = match th.gl().border {
        BorderKind::Rounded => b.border_type(BorderType::Rounded),
        BorderKind::Ascii => b.border_set(ASCII_BORDER),
        BorderKind::None => b,
    };
    b
}

/// "  kore  ●N active ○N idle  ·  tokyo/unicode" — status + theme readout.
fn draw_status_bar(f: &mut ratatui::Frame, area: ratatui::layout::Rect, ui: &Ui) {
    let th = &ui.theme;
    let active = ui.roster.iter().filter(|i| i.status == "active").count();
    let idle = ui.roster.len() - active;
    let mut spans = vec![
        Span::raw("  "),
        Span::styled("kore", th.title()),
        Span::raw("  "),
        Span::styled(format!("{} {active}", th.gl().active), Style::default().fg(th.pal().green)),
        Span::styled(" active  ", th.dark()),
        Span::styled(format!("{} {idle}", th.gl().idle), th.dim()),
        Span::styled(" idle", th.dark()),
    ];
    if let Some(t) = &ui.thread_filter {
        spans.push(Span::styled(format!("  thread:{t}"), Style::default().fg(th.pal().yellow)));
    }
    if ui.scroll > 0 {
        spans.push(Span::styled(format!("  {}{}", th.gl().up, ui.scroll), Style::default().fg(th.pal().orange)));
    }
    // Theme readout, right-ish: palette/style/layout, dim.
    spans.push(Span::styled(
        format!("   {} {}/{}/{}", th.gl().bullet, th.pal().name, th.gl().name, LAYOUTS[ui.layout_i].1),
        th.dark(),
    ));
    f.render_widget(Paragraph::new(Line::from(spans)).style(th.base()), area);
}

/// Agent rows: status icon + name + tool, dim "doing" beneath. Roster focus
/// adds a caret on the cursor row and a check on selected rows.
fn draw_agents(f: &mut ratatui::Frame, area: ratatui::layout::Rect, ui: &Ui) {
    let th = &ui.theme;
    let mut lines: Vec<Line> = vec![];
    for (idx, i) in ui.roster.iter().enumerate() {
        let (icon, style) = if i.status == "active" {
            (th.gl().active, Style::default().fg(th.pal().green))
        } else {
            (th.gl().idle, th.dim())
        };
        let on_cursor = ui.focus == Focus::Roster && idx == ui.cursor;
        let selected = ui.selected.contains(&i.name);
        let caret = if on_cursor { th.gl().caret } else { " " };
        let mut spans = vec![
            Span::styled(format!(" {caret}"), Style::default().fg(th.pal().blue)),
            Span::styled(icon.to_string(), style),
            Span::raw(" "),
            // "{tag}-{name}" like legacy: the tag is a group label column.
            Span::styled(
                match i.tag.as_deref() {
                    Some(t) if !t.is_empty() => format!("{t}-{}", i.name),
                    _ => i.name.clone(),
                },
                Style::default().fg(th.pal().fg),
            ),
        ];
        if selected {
            spans.push(Span::styled(format!(" {}", th.gl().check), Style::default().fg(th.pal().cyan)));
        }
        if i.kind == "human" {
            spans.push(Span::styled(format!(" {}", th.gl().human), Style::default().fg(th.pal().magenta)));
        } else if let Some(t) = &i.tool {
            spans.push(Span::styled(format!(" {t}"), th.dark()));
        }
        let mut line = Line::from(spans);
        if on_cursor {
            line = line.style(Style::default().bg(th.pal().selection));
        }
        lines.push(line);
        if !i.status_context.is_empty() {
            let doing: String = i.status_context.chars().take(20).collect();
            lines.push(Line::from(Span::styled(
                format!("      {doing}"),
                th.dark().add_modifier(Modifier::ITALIC),
            )));
        }
    }
    f.render_widget(Paragraph::new(lines).style(th.base()), area);
}

/// Message feed: dim id, colored sender, @mentions highlighted.
fn draw_feed(f: &mut ratatui::Frame, area: ratatui::layout::Rect, me: &str, ui: &Ui) {
    let th = &ui.theme;
    let shown: Vec<&FeedLine> = ui
        .feed
        .iter()
        .filter(|l| match (l, &ui.thread_filter) {
            (FeedLine::Msg(d), Some(t)) => d.message.thread.as_deref() == Some(t.as_str()),
            _ => true,
        })
        .collect();
    let visible = area.height as usize;
    let end = shown.len().saturating_sub(ui.scroll.min(shown.len()));
    let lines: Vec<Line> = shown[..end]
        .iter()
        .rev()
        .take(visible)
        .rev()
        .map(|l| match *l {
            FeedLine::Msg(d) => {
                let sender = if d.message.from == me { th.pal().blue } else { th.pal().cyan };
                let mut spans = vec![
                    Span::styled(format!("#{:<4} ", d.id), th.dark()),
                    Span::styled(d.message.from.clone(), Style::default().fg(sender).add_modifier(Modifier::BOLD)),
                    Span::styled("  ", th.dark()),
                ];
                spans.extend(mention_spans(&d.message.text, th));
                Line::from(spans)
            }
            FeedLine::Note(s) => Line::from(vec![
                Span::raw("      "),
                Span::styled(s.clone(), th.dim().add_modifier(Modifier::ITALIC)),
            ]),
        })
        .collect();
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }).style(th.base()), area);
}

/// Highlight @mentions in the theme's orange.
fn mention_spans(text: &str, th: &Theme) -> Vec<Span<'static>> {
    let fg = Style::default().fg(th.pal().fg);
    let mention = Style::default().fg(th.pal().orange).add_modifier(Modifier::BOLD);
    text.split_inclusive(' ')
        .map(|w| {
            if w.starts_with('@') && w.len() > 1 {
                Span::styled(w.to_string(), mention)
            } else {
                Span::styled(w.to_string(), fg)
            }
        })
        .collect()
}

/// The input row: normal prompt, launch form, kill confirm, or retag prompt.
fn draw_overlay_row(f: &mut ratatui::Frame, area: ratatui::layout::Rect, ui: &Ui) {
    let th = &ui.theme;
    match &ui.overlay {
        Overlay::Launch(form) => {
            f.render_widget(Paragraph::new(form_lines(form, th)).style(th.base()), area);
        }
        Overlay::Confirm(targets) => {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("  kill ", Style::default().fg(th.pal().orange).add_modifier(Modifier::BOLD)),
                    Span::styled(targets.join(", "), Style::default().fg(th.pal().fg)),
                    Span::styled("  (y/n)", th.dim()),
                ]))
                .style(th.base()),
                area,
            );
        }
        Overlay::Retag(targets, buf) => {
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("  tag ", Style::default().fg(th.pal().yellow).add_modifier(Modifier::BOLD)),
                    Span::styled(targets.join(", "), th.dim()),
                    Span::styled(": ", th.dark()),
                    Span::styled(buf.clone(), Style::default().fg(th.pal().fg)),
                    Span::styled(th.gl().block, Style::default().fg(th.pal().blue)),
                    Span::styled("   (empty = clear · esc cancel)", th.dark()),
                ]))
                .style(th.base()),
                area,
            );
        }
        Overlay::None => {
            // Roster focus dims the prompt so focus is obvious.
            let caret_fg = if ui.focus == Focus::Input { th.pal().blue } else { th.pal().fg_dark };
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("  {} ", th.gl().caret), Style::default().fg(caret_fg)),
                    Span::styled(ui.input.clone(), Style::default().fg(th.pal().fg)),
                    Span::styled(th.gl().block, Style::default().fg(caret_fg)),
                ]))
                .style(th.base()),
                area,
            );
        }
    }
}

/// A footer hint. `synth` is the key a mouse click "presses" — `None` = shown
/// but not clickable (pure-info hints like ↑↓ move).
struct Hint {
    key: &'static str,
    desc: &'static str,
    synth: Option<(KeyCode, KeyModifiers)>,
}

const CTRL: KeyModifiers = KeyModifiers::CONTROL;

/// Footer hints for the current focus/overlay. Clickable ones carry the key
/// they emulate, so a click reuses the keyboard path (one behaviour source).
fn footer_items(ui: &Ui) -> Vec<Hint> {
    let none = KeyModifiers::NONE;
    let h = |key, desc, synth| Hint { key, desc, synth };
    match (&ui.overlay, ui.focus) {
        (Overlay::Launch(_), _) => vec![
            h("↑↓", "field", None),
            h("←→", "value", None),
            h("enter", "launch", Some((KeyCode::Enter, none))),
            h("esc", "cancel", Some((KeyCode::Esc, none))),
        ],
        (Overlay::Confirm(_), _) => vec![
            h("y", "kill", Some((KeyCode::Char('y'), none))),
            h("n/esc", "cancel", Some((KeyCode::Esc, none))),
        ],
        (Overlay::Retag(..), _) => vec![
            h("type", "tag", None),
            h("enter", "apply", Some((KeyCode::Enter, none))),
            h("esc", "cancel", Some((KeyCode::Esc, none))),
        ],
        (Overlay::None, Focus::Roster) => vec![
            h("↑↓/jJ", "move", None),
            h("space", "select", Some((KeyCode::Char(' '), none))),
            h("k", "kill", Some((KeyCode::Char('k'), none))),
            h("t", "retag", Some((KeyCode::Char('t'), none))),
            h("enter", "@", Some((KeyCode::Enter, none))),
            h("esc", "input", Some((KeyCode::Esc, none))),
        ],
        (Overlay::None, Focus::Input) => vec![
            h("enter", "send", Some((KeyCode::Enter, none))),
            h("tab", "roster", Some((KeyCode::Tab, none))),
            h("^n", "launch", Some((KeyCode::Char('n'), CTRL))),
            h("F2", "palette", Some((KeyCode::F(2), none))),
            h("F3", "style", Some((KeyCode::F(3), none))),
            h("F4", "layout", Some((KeyCode::F(4), none))),
            h("^c", "quit", Some((KeyCode::Char('c'), CTRL))),
        ],
    }
}

/// Build the footer's spans plus, for each clickable hint, its absolute-in-row
/// column range `[start, end)` and the key it emulates. `draw_footer` renders
/// the spans; `handle_mouse` hit-tests the ranges — same width math, no drift.
#[allow(clippy::type_complexity)]
fn footer_spans_hits(ui: &Ui) -> (Vec<Span<'static>>, Vec<(u16, u16, KeyCode, KeyModifiers)>) {
    let th = &ui.theme;
    let (key, lbl) = (th.dim(), th.dark());
    let width = |s: &str| s.chars().count() as u16; // footer glyphs are 1-col
    let mut spans = vec![Span::raw("  ")];
    let mut hits = vec![];
    let mut x = 2u16;
    for (i, it) in footer_items(ui).into_iter().enumerate() {
        if i > 0 {
            let sep = format!("{} ", th.gl().bullet);
            x += width(&sep);
            spans.push(Span::styled(sep, lbl));
        }
        let start = x;
        spans.push(Span::styled(it.key, key));
        x += width(it.key);
        let label = format!(" {} ", it.desc);
        x += width(&label);
        spans.push(Span::styled(label, lbl));
        if let Some((code, mods)) = it.synth {
            hits.push((start, x, code, mods));
        }
    }
    (spans, hits)
}

/// Key hints, contextual to focus/overlay — clickable (see `footer_items`).
fn draw_footer(f: &mut ratatui::Frame, area: ratatui::layout::Rect, ui: &Ui) {
    let (spans, _) = footer_spans_hits(ui);
    f.render_widget(Paragraph::new(Line::from(spans)).style(ui.theme.base()), area);
}

/// The `label  value` form rows, selected row on SELECTION bg with a caret.
fn form_lines(form: &LaunchForm, th: &Theme) -> Vec<Line<'static>> {
    let tag = if form.tag.is_empty() {
        "(optional group label, shown as tag-name)".to_string()
    } else {
        form.tag.clone()
    };
    let rows = [
        ("Tool", TOOLS[form.tool].to_string()),
        ("Count", form.count.to_string()),
        ("Tag", tag),
        ("Headless", if form.headless { th.gl().check.to_string() } else { " ".into() }),
        ("Terminal", TERMINALS[form.terminal].to_string()),
        ("Project", form.projects.get(form.project_i).cloned().unwrap_or_else(|| "(current)".into())),
        ("Owner", form.owners.get(form.owner_i).cloned().unwrap_or_else(|| "you".into())),
    ];
    let mut lines = vec![Line::from(vec![
        Span::styled("  ── ", th.dark()),
        Span::styled("launch ", Style::default().fg(th.pal().yellow)),
        Span::styled("─".repeat(40), th.dark()),
    ])];
    lines.extend(rows.into_iter().enumerate().map(|(i, (label, value))| {
        let selected = i == form.field;
        let cursor = if selected { format!("{} ", th.gl().caret) } else { "  ".into() };
        let mut line = Line::from(vec![
            Span::raw("  "),
            Span::styled(cursor, Style::default().fg(th.pal().blue)),
            Span::styled(format!("{label:<9}"), if selected { Style::default().fg(th.pal().fg) } else { th.dim() }),
            Span::styled(value, Style::default().fg(if selected { th.pal().cyan } else { th.pal().fg })),
        ]);
        if selected {
            line = line.style(Style::default().bg(th.pal().selection));
        }
        line
    }));
    lines
}
