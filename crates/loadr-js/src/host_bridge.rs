//! The host bridge: lets `'static` JS host functions call back into a
//! non-`'static` `&mut dyn ScriptHost` borrow.
//!
//! [`VuScript::call_function`](loadr_core::script::VuScript::call_function)
//! receives the host as a short-lived `&mut dyn ScriptHost`, but functions
//! registered on a QuickJS context must be `'static`. We bridge the two by
//! storing a lifetime-erased raw pointer to the host for exactly the duration
//! of one script call, guarded by an RAII type that always clears the pointer
//! (including on unwind).
//!
//! All `unsafe` code of the crate is concentrated in this module.

use std::sync::Arc;

use loadr_core::script::ScriptHost;
use parking_lot::Mutex;

/// Error returned by [`HostCell::with_host`] when no host is attached, i.e.
/// when script code runs outside of an engine-driven call (such as module
/// top-level code executed while a VU is being instantiated).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoHostError;

impl std::fmt::Display for NoHostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no script host is attached")
    }
}

impl std::error::Error for NoHostError {}

type HostRawPtr = *mut (dyn ScriptHost + 'static);

/// Newtype around the raw host pointer so it can live inside `Arc<Mutex<..>>`.
struct HostSlot(Option<HostRawPtr>);

// SAFETY: the pointer is only ever dereferenced via `HostCell::with_host`,
// which is called synchronously from native functions while QuickJS executes
// a script call on the current thread. The pointer is installed by
// `HostCell::attach` at the start of `call_function`/`eval` and removed by
// the returned `HostGuard` before those methods return (drop runs on panics
// and early returns too). A `JsVuScript` is `Send` but is driven through
// `&mut self`, so only one thread can execute script code — and therefore
// reach the pointer — at a time.
unsafe impl Send for HostSlot {}

/// A shared, clearable slot holding the currently attached script host.
///
/// One cell is created per VU script; clones of it are captured by the native
/// functions registered on that VU's JS context.
#[derive(Clone)]
pub struct HostCell {
    slot: Arc<Mutex<HostSlot>>,
}

impl Default for HostCell {
    fn default() -> Self {
        Self::new()
    }
}

impl HostCell {
    /// Create an empty cell (no host attached).
    pub fn new() -> Self {
        HostCell {
            slot: Arc::new(Mutex::new(HostSlot(None))),
        }
    }

    /// Attach `host` for the duration of the returned guard.
    ///
    /// # Safety
    ///
    /// The returned [`HostGuard`] must be dropped before the `host` borrow
    /// ends (in practice: keep it as a stack local for the duration of the
    /// script call and let it drop at scope exit). While the guard is alive,
    /// the host must not be accessed through any other path, and script
    /// execution against this cell must stay on the calling thread.
    pub unsafe fn attach(&self, host: &mut dyn ScriptHost) -> HostGuard {
        // SAFETY: this transmute only erases lifetimes on the (fat) reference;
        // the layout is identical. The guard returned below removes the
        // pointer from the slot on drop, upholding the contract above.
        let erased: &'static mut (dyn ScriptHost + 'static) = unsafe {
            std::mem::transmute::<&mut dyn ScriptHost, &'static mut (dyn ScriptHost + 'static)>(
                host,
            )
        };
        let ptr: HostRawPtr = erased;
        let prev = self.slot.lock().0.replace(ptr);
        HostGuard {
            cell: self.clone(),
            prev,
        }
    }

    /// Run `f` with the currently attached host.
    ///
    /// Fails with [`NoHostError`] when no host is attached (e.g. module
    /// top-level code running during instantiation).
    pub fn with_host<R>(&self, f: impl FnOnce(&mut dyn ScriptHost) -> R) -> Result<R, NoHostError> {
        let ptr = self.slot.lock().0.ok_or(NoHostError)?;
        // The lock is released before invoking the host so that host
        // implementations may freely use their own locks; exclusivity of the
        // `&mut` is guaranteed by single-threaded QuickJS execution (see the
        // `Send` justification on `HostSlot`).
        //
        // SAFETY: `ptr` was installed by `attach` from a live `&mut dyn
        // ScriptHost` and is removed when the corresponding `HostGuard`
        // drops, so it is valid here. Native functions (and through them this
        // method) only run synchronously inside the script call that owns the
        // guard, on the same thread, so no aliasing `&mut` can exist.
        let host = unsafe { &mut *ptr };
        Ok(f(host))
    }

    /// True when a host is currently attached.
    #[cfg(test)]
    pub fn is_attached(&self) -> bool {
        self.slot.lock().0.is_some()
    }
}

/// RAII guard returned by [`HostCell::attach`]; restores the previous
/// (usually empty) slot contents on drop.
pub struct HostGuard {
    cell: HostCell,
    prev: Option<HostRawPtr>,
}

impl Drop for HostGuard {
    fn drop(&mut self) {
        self.cell.slot.lock().0 = self.prev;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loadr_core::metrics::MetricKind;
    use loadr_core::script::{HostHttpRequest, HostHttpResponse, ScriptLogLevel};

    struct CountingHost {
        sleeps: Vec<f64>,
    }

    impl ScriptHost for CountingHost {
        fn http_request(&mut self, _req: HostHttpRequest) -> HostHttpResponse {
            HostHttpResponse::default()
        }
        fn sleep(&mut self, seconds: f64) {
            self.sleeps.push(seconds);
        }
        fn check(&mut self, _name: &str, _pass: bool) {}
        fn metric_add(
            &mut self,
            _metric: &str,
            _kind: MetricKind,
            _value: f64,
            _tags: &[(String, String)],
        ) -> Result<(), String> {
            Ok(())
        }
        fn group_push(&mut self, _name: &str) {}
        fn group_pop(&mut self) {}
        fn log(&mut self, _level: ScriptLogLevel, _message: &str) {}
        fn env_var(&self, _name: &str) -> Option<String> {
            None
        }
        fn open_file(&self, _path: &str) -> Result<Vec<u8>, String> {
            Err("not found".into())
        }
        fn get_var(&self, _name: &str) -> Option<serde_json::Value> {
            None
        }
        fn set_var(&mut self, _name: &str, _value: serde_json::Value) {}
        fn cookie_get(&self, _url: &str, _name: &str) -> Option<String> {
            None
        }
        fn cookie_set(&mut self, _url: &str, _name: &str, _value: &str) {}
        fn cookies_clear(&mut self) {}
        fn vu_info(&self) -> (u64, u64, String) {
            (0, 0, String::new())
        }
        fn data_row(&mut self, _source: &str) -> Result<serde_json::Value, String> {
            Err("no data".into())
        }
    }

    #[test]
    fn with_host_errors_when_detached() {
        let cell = HostCell::new();
        assert!(!cell.is_attached());
        assert_eq!(cell.with_host(|_| ()), Err(NoHostError));
    }

    #[test]
    fn attach_and_clear() {
        let cell = HostCell::new();
        let mut host = CountingHost { sleeps: vec![] };
        {
            let _guard = unsafe { cell.attach(&mut host) };
            assert!(cell.is_attached());
            cell.with_host(|h| h.sleep(1.5)).expect("host attached");
        }
        assert!(!cell.is_attached());
        assert_eq!(host.sleeps, vec![1.5]);
    }

    #[test]
    fn guard_clears_on_panic() {
        let cell = HostCell::new();
        let mut host = CountingHost { sleeps: vec![] };
        let cell2 = cell.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = unsafe { cell2.attach(&mut host) };
            panic!("boom");
        }));
        assert!(result.is_err());
        assert!(!cell.is_attached());
    }
}
