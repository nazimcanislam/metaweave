//! Progress messages on stderr (only shown in an interactive terminal).

use std::sync::atomic::{AtomicBool, Ordering};

use colored::Colorize;

static ENABLED: AtomicBool = AtomicBool::new(false);

pub fn init(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn step(msg: impl AsRef<str>) {
    if ENABLED.load(Ordering::Relaxed) {
        eprintln!("{} {}", "›".cyan().bold(), msg.as_ref().dimmed());
    }
}
