//! Peregrine desktop shell (phase 2) — Slint, per ADR 0001 (Accepted).
//!
//! Modes:
//! - **live** (default): restore-first login from `PEREGRINE_*` env,
//!   sync loop -> Slint models (rooms with unread, timeline,
//!   composer sends through the verified E2EE path).
//! - **--bench**: the ADR 0001 gate benchmark (mock 10k timeline,
//!   scripted scroll). Kept as the gate's regression instrument.
//!
//! Note: this crate is standalone (own lockfile) until ratatui relaxes
//! its `unicode-width =0.2.0` pin — see the workspace members comment.
//! Instant-based instrumentation here is bench-only; when this crate
//! joins the workspace, all time flows through `peregrine-core::Clock`
//! (AGENTS.md hard rule 1).

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: bins are the CLI product; stdout/stderr are the surface.
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
// Bench math converts between f64 seconds and f32 px units by design.

use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::str::FromStr;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use peregrine_core::adapter_matrix::MatrixSession;
use peregrine_core::protocol::{ProtocolSession, RoomId, TimelineUpdate};
use peregrine_core::syncloop::{LoopCfg, LoopEvent, spawn_sync_loop};
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};

slint::include_modules!();

const EVENT_COUNT: usize = 10_000;
const ROOM_COUNT: usize = 20;
// Tick cadence, override with PEREGRINE_BENCH_TICK_MS (runtime read):
// 16 ms mimics a 60 Hz frame budget; smaller ticks isolate render cost
// from tick jitter (vsync caps real frames at the display refresh rate).
fn bench_tick() -> Duration {
    Duration::from_millis(
        u64::from_str(&std::env::var("PEREGRINE_BENCH_TICK_MS").unwrap_or_default())
            .unwrap_or(16),
    )
}
const BENCH_FRAMES: usize = 600;
/// Timeline ring per room (live mode).
const ROOM_BUFFER_CAP: usize = 500;
/// Poll cadence for the tokio -> Slint event bridge.
const BRIDGE_TICK: Duration = Duration::from_millis(50);

fn main() {
    let t0 = Instant::now();
    if std::env::args().any(|a| a == "--bench") {
        bench();
        return;
    }
    live(t0);
}

// ---------------------------------------------------------------- live

enum GuiEvent {
    /// Decrypted timeline updates from the sync loop.
    Updates(Vec<TimelineUpdate>),
    /// A backward history page for one room (oldest first).
    History {
        room: String,
        updates: Vec<TimelineUpdate>,
        exhausted: bool,
    },
    /// Exit path from the pump (loop ended or errored).
    Fatal(String),
}

fn env(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(v) if !v.is_empty() => Some(v),
        _ => None,
    }
}

fn store_path(user: &str) -> String {
    let home = env("HOME").unwrap_or_else(|| '.'.to_string());
    let safe: String = user
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    format!("{home}/.local/share/peregrine/store-{safe}-peregrine-gui")
}

/// Restore-first connect (sidecar re-enters the same device; a fresh
/// login happens once per store). Returns the session and room list.
async fn connect() -> Result<(Arc<MatrixSession>, Vec<(String, String)>), String> {
    let hs = env("PEREGRINE_HS_URL").ok_or("missing PEREGRINE_HS_URL")?;
    let user = env("PEREGRINE_USER").ok_or("missing PEREGRINE_USER")?;
    let pass = env("PEREGRINE_PASS").unwrap_or_default();
    let path = store_path(&user);
    let sidecar = std::path::Path::new(&path).with_extension("session.json");
    if sidecar.exists() {
        if let Ok(s) = MatrixSession::restore(&hs, &path).await {
            // Restored state stores still need a sync pass to load
            // room member state (display names, DM flags) before the
            // room list renders usefully.
            if let Err(e) = s.initial_sync().await {
                eprintln!("initial_sync after restore failed: {e}");
            }
            let rooms = server_rooms(&s).await;
            println!("login   ok (restored {})", s.user_id());
            return Ok((Arc::new(s), rooms));
        }
        eprintln!("restore failed; fresh login");
    }
    let s = MatrixSession::login(&hs, &user, &pass, "peregrine-gui", Some(&path))
        .await
        .map_err(|e| format!("login failed: {e}"))?;
    s.initial_sync().await.map_err(|e| format!("sync: {e}"))?;
    let rooms = server_rooms(&s).await;
    println!("login   ok (fresh {})", s.user_id());
    Ok((Arc::new(s), rooms))
}

async fn server_rooms(session: &MatrixSession) -> Vec<(String, String)> {
    // Sort DMs first so the seed room (first entry) is the DM.
    let mut rooms: Vec<(bool, (String, String))> = Vec::new();
    for room in session.client().joined_rooms() {
        let id = room.room_id().as_str().to_owned();
        let is_dm = room.is_direct().await.unwrap_or(false);
        let name = match room.display_name().await {
            Ok(dn) => {
                let n = dn.to_string();
                if n.is_empty() || n == "Empty" { id.clone() } else { n }
            }
            Err(_) => id.clone(),
        };
        rooms.push((is_dm, (id, name)));
    }
    rooms.sort_by_key(|(is_dm, _)| std::cmp::Reverse(*is_dm));
    rooms.into_iter().map(|(_, pair)| pair).collect()
}

/// One backward page for a room, delivered through the bridge.
fn request_history(
    rt: &tokio::runtime::Handle,
    session: &Arc<MatrixSession>,
    room: String,
    from: Option<String>,
    tx: mpsc::Sender<GuiEvent>,
) {
    let session = Arc::clone(session);
    let rid = RoomId::new(room.clone());
    rt.spawn(async move {
        let page = match session.paginate_history(&rid, from.as_deref(), 30).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("history {room}: {e}");
                return;
            }
        };
        let _ = tx.send(GuiEvent::History {
            room,
            updates: page.updates,
            exhausted: page.next_from.is_none(),
        });
    });
}

fn live(t0: Instant) {
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("peregrine-gui: tokio runtime: {e}");
            std::process::exit(2);
        }
    };

    let (session, rooms) = match rt.block_on(connect()) {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("peregrine-gui: {e}");
            std::process::exit(2);
        }
    };

    // Full mxid (sender comparisons must use the mxid, not the bare
    // localpart from PEREGRINE_USER).
    let me: String = session.user_id().to_owned();

    let (tx, rx) = mpsc::channel::<GuiEvent>();
    // Clones for closures: the pump moves `session` and `tx`.
    let session_for_send = Arc::clone(&session);
    let tx_pages = tx.clone();
    // Sync-loop pump: forward timeline updates into the Slint thread.
    rt.spawn(async move {
        let mut handle = spawn_sync_loop(Arc::clone(&session), &LoopCfg::default());
        loop {
            match handle.next_event().await {
                Some(LoopEvent::Tick { updates, .. }) => {
                    if !updates.is_empty() {
                        let _ = tx.send(GuiEvent::Updates(updates));
                    }
                }
                Some(LoopEvent::Backoff { error, .. }) => {
                    eprintln!("loop backoff: {error}");
                }
                Some(_) => {}
                None => {
                    let _ = tx.send(GuiEvent::Fatal("sync loop ended".into()));
                    return;
                }
            }
        }
    });

    let win = match SpikeWindow::new() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("peregrine-gui: component init: {e}");
            std::process::exit(2);
        }
    };

    // Room pane: ids parallel to the model so selection maps to ids.
    let room_ids: Vec<String> = rooms.iter().map(|(id, _)| id.clone()).collect();
    let room_names: HashMap<String, String> = rooms.iter().cloned().collect();
    let room_model = Rc::new(VecModel::from(
        rooms
            .iter()
            .map(|(_, name)| RoomItem {
                name: name.clone().into(),
                unread: 0,
            })
            .collect::<Vec<_>>(),
    ));
    win.set_rooms(ModelRc::from(room_model.clone()));

    // Per-room timeline buffers; the selected room renders from its
    // buffer on every switch, capped at ROOM_BUFFER_CAP events.
    let buffers: Rc<std::cell::RefCell<HashMap<String, VecDeque<TimelineUpdate>>>> =
        Rc::new(std::cell::RefCell::new(HashMap::new()));
    let selected: Rc<std::cell::RefCell<Option<String>>> =
        Rc::new(std::cell::RefCell::new(room_ids.first().cloned()));
    // Rooms with a history page already fetched (v0: first page only);
    // pending guards duplicate spawns while a page is in flight.
    let pending: Rc<std::cell::RefCell<std::collections::HashSet<String>>> =
        Rc::new(std::cell::RefCell::new(std::collections::HashSet::new()));

    let render_room = {
        let buffers = Rc::clone(&buffers);
        let selected = Rc::clone(&selected);
        let me = me.clone();
        let win = win.as_weak();
        move || {
            let Some(w) = win.upgrade() else {
                return;
            };
            let sel = selected.borrow().clone().unwrap_or_default();
            let items: Vec<TimelineItem> = buffers
                .borrow()
                .get(&sel)
                .map(|buf| buf.iter().map(|u| to_item(u, &me)).collect())
                .unwrap_or_default();
            let model = Rc::new(VecModel::from(items));
            w.set_timeline(ModelRc::from(model));
        }
    };

    // Initial render for the seed room, then selection rewires.
    render_room();
    if let Some(seed) = selected.borrow().clone() {
        pending.borrow_mut().insert(seed.clone());
        request_history(rt.handle(), &session_for_send, seed, None, tx_pages.clone());
    }
    {
        let selected = Rc::clone(&selected);
        let room_ids = room_ids.clone();
        let render_room = render_room.clone();
        let buffers = Rc::clone(&buffers);
        let pending = Rc::clone(&pending);
        let session = Arc::clone(&session_for_send);
        let rt_handle = rt.handle().clone();
        let tx_pages = tx_pages.clone();
        win.on_room_selected(move |i| {
            if let Some(id) = room_ids.get(usize::try_from(i).unwrap_or(usize::MAX)) {
                *selected.borrow_mut() = Some(id.clone());
                render_room();
                let empty = buffers
                    .borrow()
                    .get(id)
                    .is_none_or(|b| b.is_empty());
                if empty && pending.borrow_mut().insert(id.clone()) {
                    request_history(&rt_handle, &session, id.clone(), None, tx_pages.clone());
                }
            }
        });
    }

    // Bridge timer: tokio world -> Slint world.
    let unread: Rc<std::cell::RefCell<HashMap<String, usize>>> =
        Rc::new(std::cell::RefCell::new(HashMap::new()));
    // Toast coalescing: last notification per room (millis). Mention
    // log lines still fire per event; only toasts are rate-limited.
    let last_toast: Rc<std::cell::RefCell<HashMap<String, u128>>> =
        Rc::new(std::cell::RefCell::new(HashMap::new()));
    const TOAST_WINDOW_MS: u128 = 60_000;
    let win_weak = win.as_weak();
    {
        let buffers = Rc::clone(&buffers);
        let selected = Rc::clone(&selected);
        let unread = Rc::clone(&unread);
        let room_model = Rc::clone(&room_model);
        let room_ids = room_ids.clone();
        let me = me.clone();
        let render_room = render_room.clone();
        let pending = Rc::clone(&pending);
        let last_toast = Rc::clone(&last_toast);
        let t0c = t0;
        let mut first = true;
        let bridge = Timer::default();
        bridge.start(
            TimerMode::Repeated,
            BRIDGE_TICK,
            move || {
                if first {
                    first = false;
                    println!(
                        "cold-start event-loop entry: {} ms (budget 200)",
                        t0c.elapsed().as_millis()
                    );
                }
                while let Ok(ev) = rx.try_recv() {
                    match ev {
                        GuiEvent::History {
                            room,
                            updates,
                            exhausted: _,
                        } => {
                            pending.borrow_mut().remove(&room);
                            let mut buffers = buffers.borrow_mut();
                            let entry = buffers.entry(room.clone()).or_default();
                            for u in updates {
                                entry.push_front(u);
                            }
                            while entry.len() > ROOM_BUFFER_CAP {
                                entry.pop_back();
                            }
                            drop(buffers);
                            if selected.borrow().as_deref() == Some(room.as_str()) {
                                render_room();
                            }
                        }
                        GuiEvent::Updates(updates) => {
                            let sel = selected.borrow().clone().unwrap_or_default();
                            let mut buffers = buffers.borrow_mut();
                            let mut unread = unread.borrow_mut();
                            let mut dirty_selected = false;
                            for u in updates {
                                let room_key = u.room.as_str().to_owned();
                                let entry = buffers.entry(room_key.clone()).or_default();
                                if entry.len() >= ROOM_BUFFER_CAP {
                                    entry.pop_front();
                                }
                                entry.push_back(u.clone());
                                if room_key != sel {
                                    *unread.entry(room_key).or_insert(0) += 1;
                                } else {
                                    dirty_selected = true;
                                }
                                if u.sender.as_str() != me
                                    && (u.body.contains("@room")
                                        || u.body.contains("@peregrine-test")
                                        || u.body.contains(&me))
                                {
                                    let room = u.room.as_str().to_owned();
                                    println!("mention  from {} in {}", u.sender, room);
                                    let now_ms = t0.elapsed().as_millis();
                                    let recent = last_toast
                                        .borrow()
                                        .get(&room)
                                        .is_some_and(|t| now_ms.saturating_sub(*t) < TOAST_WINDOW_MS);
                                    if recent {
                                        continue;
                                    }
                                    last_toast.borrow_mut().insert(room.clone(), now_ms);
                                    let title = room_names.get(&room).cloned().unwrap_or(room.clone());
                                    let body = format!("{}: {}", u.sender, u.body);
                                    // Off the UI thread: freedesktop
                                    // Notifications D-Bus round-trip.
                                    std::thread::spawn(move || {
                                        let _ = notify_rust::Notification::new()
                                            .summary("peregrine — mention")
                                            .body(&format!("[{title}] {body}"))
                                            .timeout(8_000)
                                            .show();
                                    });
                                }
                            }
                            // Room badges.
                            for (i, id) in room_ids.iter().enumerate() {
                                let n = unread.get(id).copied().unwrap_or(0);
                                let n = i32::try_from(n).unwrap_or(i32::MAX);
                                if let Some(mut row) = room_model.row_data(i) {
                                    if row.unread != n {
                                        row.unread = n;
                                        room_model.set_row_data(i, row);
                                    }
                                }
                            }
                            // Release the mutable buffer borrow BEFORE
                            // render_room re-borrows it (live-caught
                            // RefCell double-borrow on first echo).
                            drop(buffers);
                            if dirty_selected {
                                render_room();
                            }
                        }
                        GuiEvent::Fatal(e) => {
                            eprintln!("sync loop fatal: {e}");
                            if let Some(w) = win_weak.upgrade() {
                                let _ = w.hide();
                            }
                        }
                    }
                }
            },
        );
        std::mem::forget(bridge); // must outlive the callback borrows
    }

    // Composer: send through the core (E2EE for encrypted rooms).
    {
        let session = session_for_send;
        let selected = Rc::clone(&selected);
        win.on_send(move |text| {
            let text = text.to_string();
            if text.is_empty() {
                return;
            }
            let Some(room) = selected.borrow().clone() else {
                return;
            };
            let session = Arc::clone(&session);
            rt.spawn(async move {
                let room_id = RoomId::new(room.clone());
                match session.send_message(&room_id, &text).await {
                    Ok(_) => println!("sent     txn to {room}"),
                    Err(e) => eprintln!("send failed: {e}"),
                }
            });
        });
    }

    // System tray: Show/Hide/Quit; menu events pumped from the same
    // polling pattern as the sync bridge. GTK must be initialized on
    // the main thread before the appindicator backend touches it —
    // live-verified failure mode: without init the shell panics at
    // startup (gtk-0.18 menu.rs assertion).
    #[cfg(target_os = "linux")]
    if gtk::init().is_err() {
        eprintln!("tray: gtk init failed; continuing without tray");
        return;
    }
    build_tray(win.as_weak());

    let _ = win.run();
}

/// Real falcon icon, 64x64 RGBA generated from assets/tray-icon.svg
/// (inkscape render -> PIL resize -> const array in tray_icon_data.rs).
mod tray_icon_data;

fn tray_icon_rgba() -> tray_icon::Icon {
    tray_icon::Icon::from_rgba(
        tray_icon_data::TRAY_ICON_RGBA.to_vec(),
        64,
        64,
    )
    .expect("tray icon: static RGBA")
}

fn build_tray(win: slint::Weak<SpikeWindow>) {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    let show = MenuItem::new("Show", true, None);
    let hide = MenuItem::new("Hide", true, None);
    let show_id = show.id().clone();
    let hide_id = hide.id().clone();
    let menu = Menu::new();
    let _ = menu.append(&show);
    let _ = menu.append(&hide);
    let tray = tray_icon::TrayIconBuilder::new()
        .with_tooltip("peregrine")
        .with_icon(tray_icon_rgba())
        .with_menu(Box::new(menu))
        .build();
    let tray = match tray {
        Ok(t) => t,
        Err(e) => {
            eprintln!("tray: unavailable ({e}); continuing without");
            return;
        }
    };
    std::mem::forget(tray); // tray must outlive the window

    let menu_rx = MenuEvent::receiver();
    let pump = Timer::default();
    pump.start(
        TimerMode::Repeated,
        Duration::from_millis(100),
        move || {
            while let Ok(ev) = menu_rx.try_recv() {
                let Some(w) = win.upgrade() else {
                    return;
                };
                if ev.id() == &show_id {
                    let _ = w.show();
                } else if ev.id() == &hide_id {
                    let _ = w.hide();
                }
            }
        },
    );
    std::mem::forget(pump); // must outlive the callback borrows
}

fn to_item(u: &TimelineUpdate, me_mxid: &str) -> TimelineItem {
    // Mention heuristic mirrors the TUI rule v0: other senders calling
    // @room or our localpart (mxid comparison, not the bare env name).
    let my_localpart = me_mxid
        .split(':')
        .next()
        .unwrap_or("")
        .trim_start_matches('@');
    let at_localpart = format!("@{my_localpart}");
    let mention = u.sender.as_str() != me_mxid
        && (u.body.contains("@room") || u.body.contains(&at_localpart));
    TimelineItem {
        sender: u.sender.clone().into(),
        body: u.body.clone().into(),
        mention,
    }
}

// --------------------------------------------------------------- bench

/// ADR 0001 gate benchmark (unchanged instrument; see ADR spike table).
fn bench() {
    let t0 = Instant::now();
    let win = match SpikeWindow::new() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("peregrine-gui: component init: {e}");
            std::process::exit(2);
        }
    };

    let rooms = Rc::new(VecModel::from(
        (0..ROOM_COUNT)
            .map(|i| RoomItem {
                name: format!("room {i:02}").into(),
                unread: i32::try_from(i % 4).unwrap_or(0),
            })
            .collect::<Vec<_>>(),
    ));
    win.set_rooms(ModelRc::from(rooms.clone()));

    let timeline = Rc::new(VecModel::from(
        (0..EVENT_COUNT)
            .map(|i| {
                let i = u32::try_from(i).unwrap_or(0);
                TimelineItem {
                    sender: format!("user{:02}", i % 7).into(),
                    body: format!("event {i:05} — scrolling under load").into(),
                    mention: i % 500 == 0,
                }
            })
            .collect::<Vec<_>>(),
    ));
    win.set_timeline(ModelRc::from(timeline.clone()));

    win.on_send(|_text| {});

    // Cold start: this fires on the first trip through the event loop.
    let cold = Rc::new(Cell::new(0_u128));
    let t0c = t0;
    let cold_cb = move || {
        if cold.get() == 0 {
            cold.set(t0c.elapsed().as_millis());
            println!(
                "cold-start event-loop entry: {} ms (budget 200)",
                cold.get()
            );
        }
    };
    Timer::single_shot(Duration::ZERO, cold_cb);

    // Scripted scroll: one repaint per tick, BENCH_FRAMES frames.
    let steps = Rc::new(Cell::new(0_usize));
    let win_weak = win.as_weak();
    let steps_c = steps.clone();
    let t0b = t0;
    let timer = Timer::default();
    timer.start(
        TimerMode::Repeated,
        bench_tick(),
        move || {
            let Some(w) = win_weak.upgrade() else {
                return;
            };
            let range = f64::from(w.get_scroll_range());
            let step = range / f64::from(u32::try_from(BENCH_FRAMES).unwrap_or(1));
            let n = steps_c.get();
            if n >= BENCH_FRAMES {
                return;
            }
            w.set_timeline_scroll_y((step * n as f64) as f32);
            steps_c.set(n + 1);
            if n + 1 == BENCH_FRAMES {
                let elapsed = t0b.elapsed().as_secs_f64();
                println!(
                    "scroll bench: {BENCH_FRAMES} frames in {elapsed:.2}s -> {:.1} fps (budget 60)",
                    f64::from(u32::try_from(BENCH_FRAMES).unwrap_or(1)) / elapsed
                );
                let _ = w.hide(); // ends run(); main prints and exits
            }
        },
    );
    std::mem::forget(timer); // must outlive the callback borrow

    let _ = win.run();
    println!("spike done: window closed after bench");
}
