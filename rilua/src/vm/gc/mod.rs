//! Garbage collector: arena-based mark-sweep with generational indices.
//!
//! The GC uses typed arenas to store all heap-allocated Lua objects.
//! Each arena entry has a generational index that prevents
//! use-after-free without requiring `unsafe` code.
//!
//! ## Tri-Color Marking
//!
//! Objects have one of four colors:
//!
//! - **White0/White1** -- unmarked (potentially dead). Two white colors
//!   alternate each GC cycle so newly allocated objects during sweep
//!   are distinguishable from dead objects.
//! - **Gray** -- marked but children not yet traced.
//! - **Black** -- fully traced (all children are reachable).

pub mod arena;
pub mod collector;
pub mod trace;

/// GC object color for tri-color mark-sweep.
///
/// Two white colors (`White0`/`White1`) alternate each GC cycle. The
/// "current white" identifies live unmarked objects; the "other white"
/// identifies dead objects from the previous cycle. This avoids needing
/// a separate pass to reset colors after sweep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// White (generation 0). Used in even GC cycles.
    White0,
    /// White (generation 1). Used in odd GC cycles.
    White1,
    /// Gray: marked, children not yet traced.
    Gray,
    /// Black: fully traced.
    Black,
}

impl Color {
    /// Returns `true` if this color is either `White0` or `White1`.
    #[inline]
    pub fn is_white(self) -> bool {
        matches!(self, Self::White0 | Self::White1)
    }

    /// Returns `true` if this color is `Black`.
    #[inline]
    pub fn is_black(self) -> bool {
        self == Self::Black
    }

    /// Returns `true` if this color is `Gray`.
    #[inline]
    pub fn is_gray(self) -> bool {
        self == Self::Gray
    }

    /// Returns the other white color (`White0` <-> `White1`).
    ///
    /// Returns `self` unchanged if not white.
    #[inline]
    #[must_use]
    pub fn other_white(self) -> Self {
        match self {
            Self::White0 => Self::White1,
            Self::White1 => Self::White0,
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_detection() {
        assert!(Color::White0.is_white());
        assert!(Color::White1.is_white());
        assert!(!Color::Gray.is_white());
        assert!(!Color::Black.is_white());
    }

    #[test]
    fn black_detection() {
        assert!(Color::Black.is_black());
        assert!(!Color::White0.is_black());
        assert!(!Color::Gray.is_black());
    }

    #[test]
    fn gray_detection() {
        assert!(Color::Gray.is_gray());
        assert!(!Color::White0.is_gray());
        assert!(!Color::Black.is_gray());
    }

    #[test]
    fn other_white_flips() {
        assert_eq!(Color::White0.other_white(), Color::White1);
        assert_eq!(Color::White1.other_white(), Color::White0);
    }

    #[test]
    fn other_white_non_white_unchanged() {
        assert_eq!(Color::Gray.other_white(), Color::Gray);
        assert_eq!(Color::Black.other_white(), Color::Black);
    }
}
