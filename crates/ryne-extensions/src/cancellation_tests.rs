use super::*;
use std::sync::atomic::AtomicUsize;

#[test]
fn cancellation_relays_are_scoped_and_run_once_across_clones() {
    for cancelled_first in [false, true] {
        let control = CancelControl::default();
        let calls = Arc::new(AtomicUsize::new(0));
        if cancelled_first {
            control.cancel();
        }
        let observed = calls.clone();
        let _registration = control.on_cancel(move || {
            observed.fetch_add(1, Ordering::Relaxed);
        });
        control.clone().cancel();
        control.cancel();
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
    let control = CancelControl::default();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    drop(control.on_cancel(move || {
        observed.fetch_add(1, Ordering::Relaxed);
    }));
    control.cancel();
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn cancellation_registration_cannot_miss_a_concurrent_cancel() {
    for _ in 0..64 {
        let control = CancelControl::default();
        let worker = control.clone();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let start = barrier.clone();
        let cancel = std::thread::spawn(move || {
            start.wait();
            worker.cancel();
        });
        barrier.wait();
        let _registration = control.on_cancel(move || {
            observed.fetch_add(1, Ordering::Relaxed);
        });
        cancel.join().unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
    let expired = CancelControl::with_timeout(Duration::ZERO);
    assert!(expired.remaining().is_err());
}

#[test]
fn cancellation_contains_panics_and_allows_reentrant_relays() {
    let control = CancelControl::default();
    let _failure = control.on_cancel(|| panic!("backend cancellation failed"));
    let reentrant = control.clone();
    let _registration = control.on_cancel(move || reentrant.cancel());
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| control.cancel())).is_ok());
    assert!(control.is_cancelled());
}
