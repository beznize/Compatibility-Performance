//! Optional per-process pipeline inventory. Each event has a sequence number.
//! A recording command is distinct from a GPU completion or image comparison.
use std::fmt::Display;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

struct Journal {
    file: BufWriter<File>,
    number: u64,
    flushed: Instant,
}

fn journal() -> Option<&'static Mutex<Journal>> {
    static FILE: OnceLock<Option<Mutex<Journal>>> = OnceLock::new();
    FILE.get_or_init(|| {
        let path = std::env::var_os("VICTORY_PIPELINE_AUDIT")?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap_or_else(|e| panic!("cannot start requested pipeline audit at {path:?}: {e}"));
        writeln!(file, "sequence\tframe\tsource\tevent\tidentity\tdetail")
            .expect("pipeline audit header");
        Some(Mutex::new(Journal {
            file: BufWriter::with_capacity(128 << 10, file),
            number: 0,
            flushed: Instant::now(),
        }))
    })
    .as_ref()
}

pub fn enabled() -> bool {
    journal().is_some()
}

/// Free-text fields are escaped so archive names and driver messages cannot
/// introduce columns or extra events. The fast path does no allocation.
pub fn event(source: &str, event: &str, id: &str, detail: impl Display) {
    let Some(output) = journal() else { return };
    let detail = detail.to_string();
    let escaped;
    let detail = if detail.contains(['\\', '\t', '\n', '\r']) {
        escaped = detail
            .replace('\\', "\\\\")
            .replace('\t', "\\t")
            .replace('\r', "\\r")
            .replace('\n', "\\n");
        &escaped
    } else {
        &detail
    };
    let mut journal = output.lock().unwrap_or_else(|e| e.into_inner());
    journal.number += 1;
    let n = journal.number;
    let frame = crate::frame::current();
    writeln!(
        journal.file,
        "{n}\t{frame}\t{source}\t{event}\t{id}\t{detail}"
    )
    .expect("write requested pipeline audit");
    if journal.flushed.elapsed() >= Duration::from_millis(250) || event == "rejected" {
        journal
            .file
            .flush()
            .expect("flush requested pipeline audit");
        journal.flushed = Instant::now();
    }
}

/// A removed device reports u64::MAX. Its unfinished work is not covered.
#[cfg(windows)]
pub(crate) fn completed(fence: u64) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static MAX: AtomicU64 = AtomicU64::new(0);
    if enabled() && fence != u64::MAX && fence > MAX.fetch_max(fence, Ordering::Relaxed) {
        event("queue", "completed_through", "-", fence);
    }
}

pub fn flush() {
    if let Some(output) = journal() {
        output
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .file
            .flush()
            .expect("flush requested pipeline audit");
    }
}
