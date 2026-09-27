//! Trace trait: implemented by GC-managed types to mark reachable objects.
//!
//! All types stored in GC arenas must implement [`Trace`] so the mark
//! phase can discover reachable objects. The tracing infrastructure
//! (mark methods, gray stack) is built alongside the collector in
//! Phase 7. For now, this module defines the trait interface.

/// Trait for types that participate in garbage collection.
///
/// All types stored in GC arenas must implement `Trace`. During the
/// mark phase, the collector calls `trace()` on each gray object to
/// discover its references to other GC-managed objects.
///
/// Types that hold no GC references (like `LuaString`) implement this
/// as a no-op. Types that hold references (like `Table`, `Closure`)
/// must report each one so the collector can mark them reachable.
///
/// The `trace` method signature will receive a tracer/marker parameter
/// when the collector is implemented. For now it is parameterless to
/// establish the trait contract without depending on collector types.
pub trait Trace {
    /// Report all GC references reachable from this value.
    ///
    /// Called by the collector during the mark phase. Implementations
    /// should report each `GcRef` this value holds.
    fn trace(&self);

    /// Returns `true` if this type can contain GC references.
    ///
    /// Types that never hold GC references (e.g. strings) return
    /// `false` to skip tracing entirely. Defaults to `true`.
    fn needs_trace(&self) -> bool {
        true
    }
}
