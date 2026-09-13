//! Derived screen models that are computed when a screen first needs them.
//!
//! Every screen renders from a model the engine derives from the household
//! (the overview's forecast, the timeline's expansion, the tax assessment …).
//! The models used to be recomputed *all together* after any edit — eleven
//! engine runs for a change on one screen, 20 ms on the sample household and
//! growing with the series, the audit log and the horizon — so every dialog
//! ended in a hitch that had nothing to do with the screen the person was on.
//!
//! A [`Lazy`] model is instead dropped when its inputs change and computed on
//! first use, by whichever code asks for it: the visible screen's render, a
//! form that needs a figure, or a test. gpui entities are single-threaded, so
//! a [`OnceCell`] gives that from a shared reference — the accessors on
//! `AtlasApp` keep their `&self` signatures and the screens keep reading
//! plain references.

use std::cell::OnceCell;

use atlas_core::EngineError;

/// A model computed on first use and dropped by [`Lazy::invalidate`].
pub struct Lazy<M> {
    cell: OnceCell<Result<M, EngineError>>,
}

impl<M> Default for Lazy<M> {
    fn default() -> Self {
        Self::stale()
    }
}

impl<M> Lazy<M> {
    /// A model that will be computed on first use.
    pub fn stale() -> Self {
        Lazy { cell: OnceCell::new() }
    }

    /// The model, computing it with `compute` if it is stale. `compute` runs
    /// at most once per invalidation, even when it fails: an engine error is
    /// kept and shown until the inputs change again.
    pub fn get(&self, compute: impl FnOnce() -> Result<M, EngineError>) -> &Result<M, EngineError> {
        self.cell.get_or_init(compute)
    }

    /// Whether the model is currently computed (tests and the perf log).
    pub fn is_computed(&self) -> bool {
        self.cell.get().is_some()
    }

    /// Drops the model; the next [`Lazy::get`] computes it again.
    pub fn invalidate(&mut self) {
        self.cell.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn computes_once_until_invalidated() {
        let runs = Cell::new(0);
        let compute = || {
            runs.set(runs.get() + 1);
            Ok::<u32, EngineError>(runs.get())
        };
        let mut lazy: Lazy<u32> = Lazy::stale();
        assert!(!lazy.is_computed());
        assert_eq!(lazy.get(compute).as_ref().ok(), Some(&1));
        assert_eq!(lazy.get(compute).as_ref().ok(), Some(&1), "a second read does not recompute");
        assert!(lazy.is_computed());
        lazy.invalidate();
        assert!(!lazy.is_computed());
        assert_eq!(lazy.get(compute).as_ref().ok(), Some(&2), "invalidation recomputes on the next read");
        assert_eq!(runs.get(), 2);
    }

    #[test]
    fn a_failure_is_kept_until_invalidated() {
        let runs = Cell::new(0);
        let compute = || {
            runs.set(runs.get() + 1);
            Err::<u32, EngineError>(EngineError::Insufficient("no data".into()))
        };
        let lazy: Lazy<u32> = Lazy::stale();
        assert!(lazy.get(compute).is_err());
        assert!(lazy.get(compute).is_err());
        assert_eq!(runs.get(), 1, "the failing computation is not retried every frame");
    }
}
