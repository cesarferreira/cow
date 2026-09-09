use std::sync::{Arc, OnceLock, atomic::{AtomicBool, Ordering}};

static CANCELLED: OnceLock<Arc<AtomicBool>> = OnceLock::new();

fn flag() -> Arc<AtomicBool> {
    CANCELLED.get_or_init(|| Arc::new(AtomicBool::new(false))).clone()
}

pub(crate) fn requested() -> bool {
    flag().load(Ordering::Relaxed)
}

pub(crate) fn install() -> std::io::Result<()> {
    signal_hook::flag::register(signal_hook::consts::SIGINT, flag()).map(|_| ())
}

#[cfg(test)]
pub(crate) fn request_for_test() {
    flag().store(true, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn reset_for_test() {
    flag().store(false, Ordering::Relaxed);
}
