//! Progress of the job that is running right now, for the app window to poll.
//!
//! Features report real work done (pages processed, files finished); anything
//! that cannot measure itself leaves the fraction unset and the window shows an
//! indeterminate bar instead of a made-up number. One job runs at a time.

use std::sync::Mutex;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    /// 0.0 to 1.0, or `None` when the current step cannot be measured.
    pub fraction: Option<f64>,
    pub label: String,
}

struct State {
    snap: Snapshot,
    /// A step's own 0..1 is mapped into `offset .. offset + span` of the job.
    offset: f64,
    span: f64,
    prefix: String,
}

static STATE: Mutex<State> = Mutex::new(State {
    snap: Snapshot { fraction: None, label: String::new() },
    offset: 0.0,
    span: 1.0,
    prefix: String::new(),
});

fn lock() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Forget everything: a new job is starting.
pub fn reset() {
    let mut s = lock();
    s.snap = Snapshot::default();
    s.offset = 0.0;
    s.span = 1.0;
    s.prefix.clear();
}

/// Map following reports into `offset .. offset + span` of the whole job and
/// put `prefix` in front of their labels (used by batch and multi-step jobs).
pub fn window(offset: f64, span: f64, prefix: &str) {
    let mut s = lock();
    s.offset = offset.clamp(0.0, 1.0);
    s.span = span.clamp(0.0, 1.0);
    s.prefix = prefix.to_string();
}

/// Report progress of the current step (`fraction` is the step's own 0..1).
pub fn set(fraction: Option<f64>, label: &str) {
    let mut s = lock();
    let mapped = fraction.map(|f| (s.offset + s.span * f.clamp(0.0, 1.0)).clamp(0.0, 1.0));
    let label = format!("{}{}", s.prefix, label);
    s.snap = Snapshot { fraction: mapped, label };
}

pub fn snapshot() -> Snapshot {
    lock().snap.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test only: the state is process-wide and tests run in parallel.
    #[test]
    fn windows_scale_and_clamp() {
        reset();
        set(Some(0.5), "half");
        assert_eq!(snapshot(), Snapshot { fraction: Some(0.5), label: "half".into() });
        window(0.25, 0.5, "File 2 of 4 · ");
        set(Some(0.5), "page 3");
        let s = snapshot();
        assert_eq!(s.fraction, Some(0.5));
        assert_eq!(s.label, "File 2 of 4 · page 3");
        set(Some(2.0), "over");
        assert_eq!(snapshot().fraction, Some(0.75));
        set(None, "unknown");
        assert_eq!(snapshot().fraction, None);
        reset();
        assert_eq!(snapshot(), Snapshot::default());
    }
}
