//! Shared launch progress. The bootstrapper writes here from its worker thread and the launch
//! window reads it every frame.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub message: String,
    /// 0.0..=1.0, `None` while the length of the work is unknown.
    pub fraction: Option<f32>,
    pub can_cancel: bool,
    pub error: Option<String>,
    /// Exit code once the bootstrapper is done.
    pub finished: Option<i32>,
}

struct Inner {
    snap: Snapshot,
    stage: usize,
    stages: usize,
}

static STATE: Mutex<Inner> = Mutex::new(Inner {
    snap: Snapshot { message: String::new(), fraction: None, can_cancel: true, error: None, finished: None },
    stage: 0,
    stages: 1,
});
static CANCEL: AtomicBool = AtomicBool::new(false);

fn with<R>(f: impl FnOnce(&mut Inner) -> R) -> R {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut g)
}

pub fn snapshot() -> Snapshot {
    with(|s| s.snap.clone())
}

pub fn set_message(msg: &str) {
    with(|s| s.snap.message = msg.to_string());
}

/// Work is split into `total` equal stages; `index` is the one that is running now.
pub fn set_stage(index: usize, total: usize) {
    with(|s| {
        s.stage = index;
        s.stages = total.max(1);
        s.snap.fraction = Some(index as f32 / s.stages as f32);
    });
}

/// Progress (0..1) inside the running stage.
pub fn set_stage_progress(f: f32) {
    with(|s| s.snap.fraction = Some((s.stage as f32 + f.clamp(0.0, 1.0)) / s.stages as f32));
}

pub fn set_indeterminate() {
    with(|s| s.snap.fraction = None);
}

pub fn set_can_cancel(v: bool) {
    with(|s| s.snap.can_cancel = v);
}

pub fn set_error(msg: &str) {
    with(|s| s.snap.error = Some(msg.to_string()));
}

pub fn finish(code: i32) {
    with(|s| s.snap.finished = Some(code));
}

pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

pub fn cancelled() -> bool {
    CANCEL.load(Ordering::SeqCst)
}
