use std::sync::{
    Arc, OnceLock,
    atomic::{AtomicBool, Ordering},
};

static CANCELLED: OnceLock<Arc<AtomicBool>> = OnceLock::new();

fn flag() -> Arc<AtomicBool> {
    CANCELLED
        .get_or_init(|| Arc::new(AtomicBool::new(false)))
        .clone()
}

pub(crate) fn requested() -> bool {
    #[cfg(test)]
    {
        if test_cancelled() {
            return true;
        }
    }
    flag().load(Ordering::Relaxed)
}

pub(crate) fn install() -> std::io::Result<()> {
    signal_hook::flag::register(signal_hook::consts::SIGINT, flag()).map(|_| ())
}

#[cfg(test)]
thread_local! {
    static TEST_CANCELLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
fn test_cancelled() -> bool {
    TEST_CANCELLED.with(|flag| flag.get())
}

#[cfg(test)]
pub(crate) fn request_for_test() {
    TEST_CANCELLED.with(|flag| flag.set(true));
}

#[cfg(test)]
pub(crate) fn reset_for_test() {
    TEST_CANCELLED.with(|flag| flag.set(false));
}
