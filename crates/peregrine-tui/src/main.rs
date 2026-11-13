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
//! Keys: Tab switches focus (timeline / rooms); in rooms, Up/Down move
//! the cursor and Enter sets the send target (drives the hysteresis
//! window anchor); in timeline, Up/Down/PageUp/PageDown scroll the
//! view. Type to compose, Enter sends, Backspace edits, Esc / Ctrl-C
//! quits. The background sync loop is the core [`syncloop`] task.

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: binaries may print; stdout/stderr are this crate's product
// and error surface. Library crates keep the hygiene lints.

use std::{collections::VecDeque, io::Stdout, sync::Arc, time::Duration};

use peregrine_core::{
    adapter_matrix::MatrixSession,
    protocol::{ProtocolSession, RoomId, TimelineUpdate},
    syncloop::{LoopCfg, LoopEvent, LoopHandle, spawn_sync_loop},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        event::{Event, KeyCode, KeyEvent, KeyModifiers, poll, read},
        execute, terminal,
    },
    layout::{Constraint, Layout, Rect},
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
/// Page size for PgUp/PgDn.
const PAGE: usize = 10;

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

/// Which pane owns the Up/Down/Enter keys.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    /// Timeline viewport (scroll keys).
    Timeline,
    /// Room list (cursor + select keys).
    Rooms,
}

/// UI state model (frontend-only; no protocol types held).
struct Model {
    user: String,
    events: VecDeque<TimelineUpdate>,
    /// Insertion-ordered room ids with event counts.
    rooms: Vec<(String, usize)>,
    /// Send target and timeline filter (`None` = all rooms).
    selected: Option<String>,
    /// Cursor into the rendered room list (index 0 = "<all>").
    room_cursor: usize,
    focus: Focus,
    /// Viewport scroll offset from the newest event.
    scroll: usize,
    input: String,
    log: VecDeque<String>,
    passes: u64,
    shifts: u64,
    errors: u64,
    utd_sessions: usize,
    utd_escalated: usize,
    last_lists: usize,
    last_rooms: usize,
}

impl Model {
    fn new(user: String, seed_room: Option<String>) -> Self {
        Self {
            user,
            events: VecDeque::new(),
            rooms: Vec::new(),
            selected: seed_room,
            room_cursor: 0,
            focus: Focus::Timeline,
            scroll: 0,
            input: String::new(),
            log: VecDeque::new(),
            passes: 0,
            shifts: 0,
            errors: 0,
            utd_sessions: 0,
            utd_escalated: 0,
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

    fn note_room(&mut self, room: &str) {
        if let Some(slot) = self.rooms.iter_mut().find(|(r, _)| r == room) {
            slot.1 += 1;
        } else {
            self.rooms.push((room.to_owned(), 1));
        }
    }

    fn push_event(&mut self, u: TimelineUpdate) {
        self.note_room(u.room.as_str());
        if self.events.len() >= EVENT_CAP {
            self.events.pop_front();
        }
        self.events.push_back(u);
        // New content pins the view only when the user sits at the
        // bottom (anchor-pinning lite; full kernel lands with layout).
        if self.scroll == 0 {
            return;
        }
        self.scroll = self.scroll.saturating_sub(0);
    }

    /// Events visible under the current selection filter.
    fn filtered(&self) -> Vec<&TimelineUpdate> {
        match &self.selected {
            Some(room) => self
                .events
                .iter()
                .filter(|u| u.room.as_str() == room.as_str())
                .collect(),
            None => self.events.iter().collect(),
        }
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
    key: KeyEvent,
    session: &Arc<MatrixSession>,
    log_tx: &mpsc::Sender<String>,
    loop_handle: &LoopHandle,
) -> bool {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => true,
        (KeyCode::Tab, _) => {
            model.focus = if model.focus == Focus::Timeline {
                Focus::Rooms
            } else {
                Focus::Timeline
            };
            false
        }
        (KeyCode::Enter, _) if model.focus == Focus::Rooms => {
            let entry = select_entry(model.room_cursor, &model.rooms);
            model.selected.clone_from(&entry);
            model.log(match entry {
                Some(r) => format!("target -> {r}"),
                None => "target -> all (read-only)".to_owned(),
            });
            false
        }
        (KeyCode::Enter, _) => {
            let body = std::mem::take(&mut model.input);
            match model.selected.as_deref() {
                Some(room) if !body.is_empty() => {
                    spawn_send(session, room, body, log_tx.clone());
                    true
                }
                Some(_) => {
                    model.log("empty message; not sent");
                    false
                }
                None => {
                    model.log("no room selected (Tab -> rooms -> Enter)");
                    false
                }
            }
        }
        (KeyCode::Up, _) if model.focus == Focus::Rooms => {
            model.room_cursor = model.room_cursor.saturating_sub(1);
            sync_anchor(model.room_cursor, loop_handle);
            false
        }
        (KeyCode::Down, _) if model.focus == Focus::Rooms => {
            let max = model.rooms.len();
            model.room_cursor = (model.room_cursor + 1).min(max);
            sync_anchor(model.room_cursor, loop_handle);
            false
        }
        (KeyCode::Up, _) => {
            model.scroll = model.scroll.saturating_add(1);
            false
        }
        (KeyCode::Down, _) => {
            model.scroll = model.scroll.saturating_sub(1);
            false
        }
        (KeyCode::PageUp, _) => {
            model.scroll = model.scroll.saturating_add(PAGE);
            false
        }
        (KeyCode::PageDown, _) => {
            model.scroll = model.scroll.saturating_sub(PAGE);
            false
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

/// Room-list entry under `cursor` (0 = "<all>").
fn select_entry(cursor: usize, rooms: &[(String, usize)]) -> Option<String> {
    if cursor == 0 {
        None
    } else {
        rooms.get(cursor - 1).map(|(r, _)| r.clone())
    }
}

/// Publish the room-list cursor as the hysteresis window anchor — the
/// first live wiring of D2#13 to a UI signal.
fn sync_anchor(cursor: usize, loop_handle: &LoopHandle) {
    // INVARIANT: cursor values are small list indices; the u64->f32
    // conversion cannot lose them in practice, and precision at absurd
    // magnitudes is harmless for a scroll anchor.
    #[allow(clippy::cast_precision_loss)]
    let pos = cursor as f32;
    loop_handle.set_anchor(pos);
}

/// Render one frame: rooms pane on the left, header/timeline/composer/
/// status on the right.
fn draw(f: &mut ratatui::Frame<'_>, model: &Model) {
    let [rooms, main] =
        Layout::horizontal([Constraint::Length(28), Constraint::Min(20)]).areas(f.area());
    let [header, timeline, composer, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(main);

    let title = Span::styled(
        format!(
            " peregrine {} | {} | {}",
            peregrine_core::CORE_VERSION,
            model.user,
            model.selected.as_deref().unwrap_or("all (read-only)")
        ),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    f.render_widget(Paragraph::new(Line::from(title)), header);

    draw_rooms(f, model, rooms);

    let view = model.filtered();
    let visible = usize::from(timeline.height).saturating_sub(2);
    let skip_from_bottom = model.scroll.min(view.len().saturating_sub(visible));
    let end = view.len().saturating_sub(skip_from_bottom);
    let start = end.saturating_sub(visible);
    let lines: Vec<Line<'_>> = view[start..end]
        .iter()
        .map(|u| {
            Line::from(vec![
                Span::styled(format!("{} ", u.sender), Style::default().fg(Color::Blue)),
                Span::raw(&u.body),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(
            if model.focus == Focus::Timeline {
                " timeline (Up/Dn/PgUp/PgDn) "
            } else {
                " timeline "
            },
        )),
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

    let esc_banner = if model.utd_escalated > 0 {
        format!(" | ESCALATED {} (keys unavailable)", model.utd_escalated)
    } else {
        String::new()
    };
    let last_log = model.log.back().cloned().unwrap_or_default();
    let status_line = Line::from(format!(
        " passes {} | shifts {} | lists {} | rooms {} | errors {} | utd {}{} | {}",
        model.passes,
        model.shifts,
        model.last_lists,
        model.last_rooms,
        model.errors,
        model.utd_sessions,
        esc_banner,
        last_log
    ));
    let status_style = if model.utd_escalated > 0 {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(Paragraph::new(status_line).style(status_style), status);
}

/// Room list pane: "<all>" plus rooms with counts; cursor + focus
/// styling.
fn draw_rooms(f: &mut ratatui::Frame<'_>, model: &Model, area: Rect) {
    let focused = model.focus == Focus::Rooms;
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let title = if focused {
        " rooms (Up/Dn/Enter) "
    } else {
        " rooms (Tab) "
    };

    let mut lines = vec![Line::from(Span::styled(
        "<all rooms>",
        if model.selected.is_none() {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        },
    ))];
    for (room, count) in &model.rooms {
        let is_cursor = model.room_cursor == lines.len();
        let is_selected = model.selected.as_deref() == Some(room.as_str());
        let mut span_text = format!("{room} ({count})");
        if is_selected {
            span_text.insert(0, '*');
        }
        let style = if is_cursor && focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(span_text, style)));
    }

    let visible = usize::from(area.height).saturating_sub(2);
    let skip = lines.len().saturating_sub(visible);
    f.render_widget(
        Paragraph::new(lines.into_iter().skip(skip).collect::<Vec<_>>()).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(border_style),
        ),
        area,
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
    let seed_room = env("PEREGRINE_ROOM");

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

    let mut model = Model::new(session.user_id().to_owned(), seed_room);
    model.log("connected; sync loop running");

    let mut redraw = tokio::time::interval(Duration::from_millis(REDRAW_MS));
    loop {
        tokio::select! {
            ev = loop_handle.next_event() => match ev {
                Some(LoopEvent::Tick { window_shift, lists, rooms, updates, utd_pending, utd_escalated }) => {
                    model.passes += 1;
                    if window_shift.is_some() {
                        model.shifts += 1;
                    }
                    model.last_lists = lists;
                    model.last_rooms = rooms;
                    model.utd_sessions = utd_pending;
                    model.utd_escalated = utd_escalated;
                    for u in updates {
                        model.push_event(u);
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
                        if handle_key(&mut model, key, &session, &log_tx, &loop_handle) {
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
