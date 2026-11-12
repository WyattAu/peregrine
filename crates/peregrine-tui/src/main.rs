//! Peregrine TUI: the phase-1 daily-driver frontend (kestrel-tui
//! pattern). Thin by contract (AGENTS rule 6): all protocol logic lives
//! in `peregrine-core`; this binary owns layout, key handling, and the
//! loop wiring only.
//!
//! ```text
//! PEREGRINE_HS_URL=https://matrix.example.org \
//! PEREGRINE_USER=alice PEREGRINE_PASS=... \
//! [PEREGRINE_ROOM=!room:server] [PEREGRINE_STORE_PATH=/tmp/store] \
//! cargo run -p peregrine-tui
//! ```
//!
//! Keys: type to compose, Enter sends (requires `PEREGRINE_ROOM`),
//! Backspace edits, Esc / Ctrl-C quits. The background sync loop is the
//! core [`syncloop`] task; the UI consumes its events and never touches
//! the protocol substrate directly beyond `send_message`.

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: binaries may print; stdout/stderr are this crate's product
// and error surface. Library crates keep the hygiene lints.

use std::{collections::VecDeque, io::Stdout, sync::Arc, time::Duration};

use peregrine_core::{
    adapter_matrix::MatrixSession,
    protocol::{ProtocolSession, RoomId},
    syncloop::{LoopCfg, LoopEvent, LoopHandle, spawn_sync_loop},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        event::{Event, KeyCode, KeyModifiers, poll, read},
        execute, terminal,
    },
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use tokio::sync::mpsc;

/// Ring capacity for displayed timeline events (oldest dropped).
const EVENT_CAP: usize = 512;
/// Ring capacity for the status log.
const LOG_CAP: usize = 64;
/// Redraw cadence; draw is cheap, events drive state.
const REDRAW_MS: u64 = 100;

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn require(name: &str) -> String {
    env(name).unwrap_or_else(|| {
        eprintln!("peregrine: missing env {name}");
        std::process::exit(2);
    })
}

/// Terminal lifecycle guard: raw mode + alternate screen, restored on
/// drop so panics and error paths never strand the shell.
struct TuiGuard;

impl TuiGuard {
    fn enter() -> std::io::Result<(TuiGuard, Terminal<CrosstermBackend<Stdout>>)> {
        terminal::enable_raw_mode()?;
        let mut out = std::io::stdout();
        execute!(out, terminal::EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(out))?;
        Ok((TuiGuard, terminal))
    }
}

impl Drop for TuiGuard {
    fn drop(&mut self) {
        // INVARIANT: drop-path restoration is best-effort; error surfaces
        // belong to the explicit restore at normal exit, and an error
        // here cannot be meaningfully handled mid-unwind.
        let _ = terminal::disable_raw_mode();
        let _ = execute!(std::io::stdout(), terminal::LeaveAlternateScreen);
    }
}

/// UI state model (frontend-only; no protocol types held).
struct Model {
    user: String,
    room: Option<String>,
    events: VecDeque<peregrine_core::protocol::TimelineUpdate>,
    input: String,
    log: VecDeque<String>,
    passes: u64,
    shifts: u64,
    errors: u64,
    utd_sessions: usize,
    last_lists: usize,
    last_rooms: usize,
}

impl Model {
    fn new(user: String, room: Option<String>) -> Self {
        Self {
            user,
            room,
            events: VecDeque::new(),
            input: String::new(),
            log: VecDeque::new(),
            passes: 0,
            shifts: 0,
            errors: 0,
            utd_sessions: 0,
            last_lists: 0,
            last_rooms: 0,
        }
    }

    fn log(&mut self, line: impl Into<String>) {
        if self.log.len() >= LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(line.into());
    }

    fn push_event(&mut self, u: peregrine_core::protocol::TimelineUpdate) {
        if self.events.len() >= EVENT_CAP {
            self.events.pop_front();
        }
        self.events.push_back(u);
    }
}

/// Blocking input pump: polls crossterm on a dedicated OS thread and
/// forwards events over a bounded channel (estate policy: backpressure
/// is designed state — a stalled UI stalls the pump, the pump never
/// balloons). Runs for the process lifetime.
fn spawn_input_pump() -> mpsc::Receiver<Event> {
    let (tx, rx) = mpsc::channel(128);
    std::thread::spawn(move || {
        loop {
            match poll(Duration::from_millis(50)) {
                Ok(true) => match read() {
                    Ok(ev) => {
                        if tx.blocking_send(ev).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                },
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });
    rx
}

/// Spawn one send through the core [`ProtocolSession`]; the receipt or
/// error lands back in the UI log channel (bounded: a UI that stops
/// draining backpressures send completions — they are never dropped).
/// Fire-and-forget otherwise: the outbox ledger carries the truth (txn
/// id is deterministic).
fn spawn_send(
    session: &Arc<MatrixSession>,
    room: &str,
    body: String,
    log_tx: mpsc::Sender<String>,
) {
    let session = Arc::clone(session);
    let room = RoomId::new(room.to_owned());
    tokio::spawn(async move {
        let line = match session.send_message(&room, &body).await {
            Ok(receipt) => format!("sent txn {}", receipt.txn_id),
            Err(e) => format!("send failed: {e}"),
        };
        let _ = log_tx.send(line).await;
    });
}

/// Handle one key event; returns true when the UI should quit.
fn handle_key(
    model: &mut Model,
    key: ratatui::crossterm::event::KeyEvent,
    session: &Arc<MatrixSession>,
    log_tx: &mpsc::Sender<String>,
) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
        (KeyCode::Enter, _) => {
            let body = std::mem::take(&mut model.input);
            match model.room.as_deref() {
                Some(room) if !body.is_empty() => {
                    spawn_send(session, room, body, log_tx.clone());
                    true
                }
                Some(_) => {
                    model.log("empty message; not sent");
                    false
                }
                None => {
                    model.log("no PEREGRINE_ROOM; read-only");
                    false
                }
            }
        }
        (KeyCode::Backspace, _) => {
            model.input.pop();
            false
        }
        (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
            model.input.push(c);
            false
        }
        _ => false,
    }
}

/// Render one frame. Layout: header, timeline, composer, status.
fn draw(f: &mut ratatui::Frame<'_>, model: &Model) {
    let [header, timeline, composer, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(f.area());

    let title = Span::styled(
        format!(
            " peregrine {} | {} | {}",
            peregrine_core::CORE_VERSION,
            model.user,
            model
                .room
                .as_deref()
                .unwrap_or("read-only (no PEREGRINE_ROOM)")
        ),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(Paragraph::new(Line::from(title)), header);

    let visible = usize::from(timeline.height);
    let skip = model.events.len().saturating_sub(visible);
    let lines: Vec<Line<'_>> = model
        .events
        .iter()
        .skip(skip)
        .map(|u| {
            Line::from(vec![
                Span::styled(format!("{} ", u.sender), Style::default().fg(Color::Blue)),
                Span::raw(&u.body),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" timeline ")),
        timeline,
    );

    f.render_widget(
        Paragraph::new(model.input.as_str()).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" compose (Enter send, Esc quit) "),
        ),
        composer,
    );

    let last_log = model.log.back().cloned().unwrap_or_default();
    let status_line = Line::from(format!(
        " passes {} | shifts {} | lists {} | rooms {} | errors {} | utd {} | {}",
        model.passes,
        model.shifts,
        model.last_lists,
        model.last_rooms,
        model.errors,
        model.utd_sessions,
        last_log
    ));
    f.render_widget(
        Paragraph::new(status_line).style(Style::default().fg(Color::DarkGray)),
        status,
    );
}

fn main() {
    // INVARIANT: binary entry expects runtime construction; failure is a
    // process-level configuration error surfaced by the exit message.
    #[allow(clippy::expect_used)]
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    rt.block_on(run());
}

async fn run() {
    let hs = require("PEREGRINE_HS_URL");
    let user = require("PEREGRINE_USER");
    let pass = require("PEREGRINE_PASS");
    let room = env("PEREGRINE_ROOM");

    let session = match MatrixSession::login(
        &hs,
        &user,
        &pass,
        "peregrine-tui",
        env("PEREGRINE_STORE_PATH").as_deref(),
    )
    .await
    {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("peregrine: login failed: {e}");
            std::process::exit(1);
        }
    };

    let (log_tx, mut log_rx) = mpsc::channel(16);
    let mut loop_handle: LoopHandle = spawn_sync_loop(Arc::clone(&session), &LoopCfg::default());
    let mut input_rx = spawn_input_pump();

    let (guard, mut terminal) = match TuiGuard::enter() {
        Ok(g) => g,
        Err(e) => {
            eprintln!("peregrine: terminal setup failed: {e}");
            loop_handle.stop();
            std::process::exit(1);
        }
    };
    let guard = guard;

    let mut model = Model::new(session.user_id().to_owned(), room);
    model.log("connected; sync loop running");

    let mut redraw = tokio::time::interval(Duration::from_millis(REDRAW_MS));
    loop {
        tokio::select! {
            ev = loop_handle.next_event() => match ev {
                Some(LoopEvent::Tick { window_shift, lists, rooms, updates }) => {
                    model.passes += 1;
                    if window_shift.is_some() {
                        model.shifts += 1;
                    }
                    model.last_lists = lists;
                    model.last_rooms = rooms;
                    for u in updates {
                        model.push_event(u);
                    }
                    if let Ok(utd) = session.utd_pending() {
                        model.utd_sessions = utd.len();
                    }
                }
                Some(LoopEvent::Backoff { cadence_ms, error }) => {
                    model.errors += 1;
                    model.log(format!("sync backoff {cadence_ms} ms: {error}"));
                }
                None => {
                    model.log("sync loop ended");
                }
            },
            ev = log_rx.recv() => {
                if let Some(line) = ev {
                    model.log(line);
                }
            },
            ev = input_rx.recv() => {
                match ev {
                    Some(Event::Key(key)) => {
                        if handle_key(&mut model, key, &session, &log_tx) {
                            break;
                        }
                    }
                    Some(_) => {}
                    None => break,
                }
            }
            _ = redraw.tick() => {
                if terminal.draw(|f| draw(f, &model)).is_err() {
                    break;
                }
            }
        }
    }

    loop_handle.stop();
    drop(terminal);
    drop(guard);
    let _ = terminal::disable_raw_mode();
    let _ = execute!(std::io::stdout(), terminal::LeaveAlternateScreen);
    println!(
        "peregrine: session summary — passes {} | shifts {} | errors {} | events {}",
        model.passes,
        model.shifts,
        model.errors,
        model.events.len()
    );
}
