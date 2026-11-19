//! Slint spike per ADR 0001 (`docs/adr/0001-desktop-gui-stack.md`).
//!
//! Gate criteria measured by this binary:
//! 1. **Three-pane layout** — rooms / timeline / composer in
//!    `ui/spike.slint` (compiles + renders = layout proof).
//! 2. **60 FPS on a 10k-event timeline** — `--bench` drives the
//!    ListView viewport one step per 16 ms timer tick for 600 frames;
//!    steps/second ≈ frames/second because each `viewport-y` change
//!    forces a repaint.
//! 3. **Cold start < 200 ms** — wall clock from `main` entry to
//!    event-loop entry (first `Timer::single_shot(ZERO)` fires inside
//!    the loop). Pair with `SLINT_DEBUG_PERFORMANCE=console` for
//!    per-frame costs at runtime.
//!
//! ```text
//! cargo run -p peregrine-gui            # eyeball mode
//! cargo run -p peregrine-gui -- --bench # scripted scroll benchmark
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: bins are the CLI product; stdout/stderr are the surface.
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
// Spikes measure wall clock; f32 px conversions are render-domain units.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};

slint::include_modules!();

const EVENT_COUNT: usize = 10_000;
const ROOM_COUNT: usize = 20;
const BENCH_FRAMES: usize = 600;
const BENCH_TICK: Duration = Duration::from_millis(16);

fn main() {
    let t0 = Instant::now();
    let bench = std::env::args().any(|a| a == "--bench");

    let win = SpikeWindow::new().expect("spike: component init failed");

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

    win.on_send(move |text| {
        timeline.push(TimelineItem {
            sender: "you".into(),
            body: text,
            mention: false,
        });
    });

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

    if bench {
        // Scripted scroll: one repaint per tick, BENCH_FRAMES frames.
        let steps = Rc::new(Cell::new(0_usize));
        let win_weak = win.as_weak();
        let steps_c = steps.clone();
        let t0b = t0;
        let timer = Timer::default();
        timer.start(
            TimerMode::Repeated,
            BENCH_TICK,
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
    }

    let _ = win.run();
    if bench {
        println!("spike done: window closed after bench");
    }
}
