use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

static LOG_SEQ: AtomicU64 = AtomicU64::new(0);
static LOG_BUFFER: OnceLock<Mutex<VecDeque<DebugLogEntry>>> = OnceLock::new();

const MAX_LOG_LINES: usize = 2000;

#[derive(Clone)]
pub struct DebugLogEntry {
    pub id: u64,
    pub message: String,
}

fn log_buffer() -> &'static Mutex<VecDeque<DebugLogEntry>> {
    LOG_BUFFER.get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_LOG_LINES)))
}

pub fn write(message: &str) {
    let id = LOG_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    let entry = DebugLogEntry {
        id,
        message: message.to_string(),
    };

    if let Ok(mut logs) = log_buffer().lock() {
        logs.push_back(entry.clone());
        while logs.len() > MAX_LOG_LINES {
            let _ = logs.pop_front();
        }
    }

}

pub fn read_since(last_id: u64) -> Vec<DebugLogEntry> {
    match log_buffer().lock() {
        Ok(logs) => logs
            .iter()
            .filter(|entry| entry.id > last_id)
            .cloned()
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {{
        $crate::debug_logs::write(&format!($($arg)*));
    }};
}
