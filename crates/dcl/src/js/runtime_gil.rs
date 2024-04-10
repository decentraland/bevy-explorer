// global interpreter lock, prevent multithreaded re-entrant v8 crashes on linux

#[cfg(target_os = "linux")]
pub(crate) use linux::*;

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static RUNTIME_GIL: OnceLock<Mutex<()>> = OnceLock::new();

    pub struct RuntimeGil(pub MutexGuard<'static, ()>);

    #[inline(always)]
    pub(crate) fn lock_runtime() -> RuntimeGil {
        RuntimeGil(RUNTIME_GIL.get_or_init(Default::default).lock().unwrap())
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) use not_linux::*;

#[cfg(not(target_os = "linux"))]
mod not_linux {
    pub struct RuntimeGil;

    #[inline(always)]
    pub(crate) fn lock_runtime() -> RuntimeGil {
        // noop
        RuntimeGil
    }
}
