//! Fires one desktop notification through the same notify-rust call
//! the live shell uses for mentions — verifies the freedesktop
//! D-Bus -> compositor path in isolation.
//!
//! ```text
//! cargo run -p peregrine-gui --example notify_probe
//! ```

#![allow(clippy::print_stdout, clippy::print_stderr)]
// INVARIANT: example bins are CLI products; stdout/stderr are the surface.

fn main() {
    match notify_rust::Notification::new()
        .summary("peregrine — notification probe")
        .body("If you can read this, the desktop notification path works.")
        .timeout(8_000)
        .show()
    {
        Ok(_) => println!("probe   ok — notification handed to the desktop daemon"),
        Err(e) => {
            eprintln!("probe   failed: {e}");
            std::process::exit(1);
        }
    }
}
