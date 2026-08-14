//! Progress tracking for the background metadata passes.
//!
//! The two sources get a slot each rather than sharing one. Steam needs no credentials
//! and finishes in seconds; IGDB needs credentials and takes minutes. Sharing a slot
//! would mean a user with no IGDB keys got no tags at all, and would queue a three-second
//! pass behind a four-minute one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::{AppHandle, Emitter};

use ugly_core::models::EnrichmentJob;

pub struct JobSlot {
    state: Mutex<EnrichmentJob>,
    running: AtomicBool,
    /// The event this slot pushes progress on, so the UI can tell the two apart.
    event: &'static str,
}

impl JobSlot {
    pub fn new(event: &'static str) -> Self {
        Self {
            state: Mutex::new(EnrichmentJob::default()),
            running: AtomicBool::new(false),
            event,
        }
    }

    pub fn snapshot(&self) -> EnrichmentJob {
        self.state.lock().unwrap().clone()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Claims the slot. Returns false if a pass is already under way, in which case the
    /// caller should leave the existing job alone.
    pub fn try_start(&self, total: usize) -> bool {
        // swap rather than load-then-store: two callers must not both see "not running".
        if self.running.swap(true, Ordering::SeqCst) {
            return false;
        }
        let mut job = self.state.lock().unwrap();
        job.running = true;
        job.total = total;
        job.completed = 0;
        job.error = None;
        true
    }

    pub fn advance(&self, app: &AppHandle, by: usize) {
        let snapshot = {
            let mut job = self.state.lock().unwrap();
            job.completed = (job.completed + by).min(job.total);
            job.clone()
        };
        let _ = app.emit(self.event, snapshot);
    }

    pub fn finish(&self, app: &AppHandle, error: Option<String>) {
        let snapshot = {
            let mut job = self.state.lock().unwrap();
            job.running = false;
            if error.is_some() {
                job.error = error;
            }
            job.clone()
        };
        self.running.store(false, Ordering::SeqCst);
        let _ = app.emit(self.event, snapshot);
    }
}
