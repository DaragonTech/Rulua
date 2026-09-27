//! Generational arena: typed `Vec` storage with generation-checked references.
//!
//! `Arena<T>` provides O(1) allocation, access, and deallocation for
//! GC-managed objects. Each slot has a generation counter that increments
//! on free, invalidating any `GcRef<T>` created before the free.
//!
//! This prevents use-after-free without `unsafe` code: a stale `GcRef`
//! has a generation mismatch and `get()` returns `None`.
//!
//! GC colors are stored in a separate parallel `Vec<u8>` for cache-friendly
//! sweep. The sweep loop iterates the compact color array (1 byte per slot,
//! ~64 per cache line) and only touches the full `Entry<T>` when freeing
//! dead objects.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use super::Color;

// ---------------------------------------------------------------------------
// Color byte encoding
// ---------------------------------------------------------------------------

/// Color stored as `u8` for cache-friendly sweep iteration.
const COLOR_FREE: u8 = 0;
const COLOR_WHITE0: u8 = 1;
const COLOR_WHITE1: u8 = 2;
const COLOR_GRAY: u8 = 3;
const COLOR_BLACK: u8 = 4;

#[inline]
fn color_to_byte(c: Color) -> u8 {
    match c {
        Color::White0 => COLOR_WHITE0,
        Color::White1 => COLOR_WHITE1,
        Color::Gray => COLOR_GRAY,
        Color::Black => COLOR_BLACK,
    }
}

#[inline]
fn byte_to_color(b: u8) -> Option<Color> {
    match b {
        COLOR_WHITE0 => Some(Color::White0),
        COLOR_WHITE1 => Some(Color::White1),
        COLOR_GRAY => Some(Color::Gray),
        COLOR_BLACK => Some(Color::Black),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// GcRef<T>
// ---------------------------------------------------------------------------

/// A generation-checked reference into an `Arena<T>`.
///
/// A `GcRef` is a lightweight handle (8 bytes: two `u32` fields) that
/// identifies a specific allocation in an arena. The generation field
/// is checked on every access to detect stale references.
///
/// Two `GcRef` values are equal if and only if they have the same index
/// and generation, meaning they refer to the same allocation.
pub struct GcRef<T> {
    index: u32,
    generation: u32,
    _marker: PhantomData<T>,
}

// Manual trait impls because derive would add bounds on T.
// GcRef is just two u32 indices -- it does not contain or reference T data
// directly, so it is safe to Send/Sync regardless of T's bounds.
#[cfg(feature = "send")]
#[allow(unsafe_code)]
unsafe impl<T> Send for GcRef<T> {}
#[cfg(feature = "send")]
#[allow(unsafe_code)]
unsafe impl<T> Sync for GcRef<T> {}

impl<T> Clone for GcRef<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for GcRef<T> {}

impl<T> PartialEq for GcRef<T> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}

impl<T> Eq for GcRef<T> {}

impl<T> Hash for GcRef<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}

impl<T> fmt::Debug for GcRef<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GcRef({}, gen={})", self.index, self.generation)
    }
}

impl<T> GcRef<T> {
    /// Returns the raw slot index.
    #[inline]
    pub fn index(self) -> u32 {
        self.index
    }

    /// Returns the generation counter.
    #[inline]
    pub fn generation(self) -> u32 {
        self.generation
    }
}

// ---------------------------------------------------------------------------
// Arena<T> internals
// ---------------------------------------------------------------------------

/// Slot state: either occupied with a value or on the free list.
enum EntryState<T> {
    Occupied { value: T },
    Free { next_free: Option<u32> },
}

/// A single arena slot with a generation counter.
struct Entry<T> {
    /// Incremented on each free, invalidating old `GcRef` handles.
    generation: u32,
    state: EntryState<T>,
}

// ---------------------------------------------------------------------------
// Arena<T>
// ---------------------------------------------------------------------------

/// A typed arena with generational indices for GC-managed objects.
///
/// Objects are stored in a `Vec` and accessed via `GcRef<T>` handles.
/// Freed slots are recycled through a free list. Each slot tracks a
/// generation counter that increments on free, invalidating stale
/// references.
///
/// GC colors are stored in a parallel `Vec<u8>` (`colors`) that is
/// always the same length as `entries`. This layout allows the sweep
/// to iterate a compact byte array without loading full `Entry<T>`
/// values for surviving objects.
pub struct Arena<T> {
    entries: Vec<Entry<T>>,
    /// Parallel color array. `colors[i]` holds the color byte for
    /// `entries[i]`. `COLOR_FREE` (0) for free slots.
    colors: Vec<u8>,
    free_head: Option<u32>,
    len: u32,
}

impl<T> Arena<T> {
    /// Creates a new empty arena.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            colors: Vec::new(),
            free_head: None,
            len: 0,
        }
    }

    /// Returns the number of occupied slots.
    #[inline]
    pub fn len(&self) -> u32 {
        self.len
    }

    /// Returns `true` if no slots are occupied.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the total number of slots (occupied + free).
    #[inline]
    pub fn capacity(&self) -> u32 {
        self.entries.len() as u32
    }

    /// Allocates a new slot with the given value and initial color.
    ///
    /// Reuses a slot from the free list if available, otherwise appends.
    /// Returns a `GcRef` that is valid until the slot is freed.
    pub fn alloc(&mut self, value: T, color: Color) -> GcRef<T> {
        self.len += 1;
        let color_byte = color_to_byte(color);

        if let Some(free_idx) = self.free_head {
            let entry = &mut self.entries[free_idx as usize];

            // Advance the free list head before overwriting the slot.
            if let EntryState::Free { next_free } = entry.state {
                self.free_head = next_free;
            }

            let generation = entry.generation;
            entry.state = EntryState::Occupied { value };
            self.colors[free_idx as usize] = color_byte;

            GcRef {
                index: free_idx,
                generation,
                _marker: PhantomData,
            }
        } else {
            let index = self.entries.len() as u32;
            self.entries.push(Entry {
                generation: 0,
                state: EntryState::Occupied { value },
            });
            self.colors.push(color_byte);

            GcRef {
                index,
                generation: 0,
                _marker: PhantomData,
            }
        }
    }

    /// Returns a reference to the value if the `GcRef` is valid.
    ///
    /// Returns `None` if the generation does not match (stale ref)
    /// or the index is out of bounds.
    #[inline]
    pub fn get(&self, r: GcRef<T>) -> Option<&T> {
        let entry = self.entries.get(r.index as usize)?;
        if entry.generation != r.generation {
            return None;
        }
        match &entry.state {
            EntryState::Occupied { value } => Some(value),
            EntryState::Free { .. } => None,
        }
    }

    /// Returns a mutable reference to the value if the `GcRef` is valid.
    #[inline]
    pub fn get_mut(&mut self, r: GcRef<T>) -> Option<&mut T> {
        let entry = self.entries.get_mut(r.index as usize)?;
        if entry.generation != r.generation {
            return None;
        }
        match &mut entry.state {
            EntryState::Occupied { value } => Some(value),
            EntryState::Free { .. } => None,
        }
    }

    /// Frees the slot and returns the owned value.
    ///
    /// The slot's generation increments, invalidating this `GcRef` and
    /// any copies. The freed slot joins the free list for reuse.
    ///
    /// Returns `None` if the ref is stale or out of bounds.
    pub fn free(&mut self, r: GcRef<T>) -> Option<T> {
        let entry = self.entries.get_mut(r.index as usize)?;
        if entry.generation != r.generation {
            return None;
        }

        let old_state = std::mem::replace(
            &mut entry.state,
            EntryState::Free {
                next_free: self.free_head,
            },
        );

        entry.generation = entry.generation.wrapping_add(1);
        self.free_head = Some(r.index);
        self.len -= 1;
        self.colors[r.index as usize] = COLOR_FREE;

        match old_state {
            EntryState::Occupied { value } => Some(value),
            EntryState::Free { .. } => None,
        }
    }

    /// Returns `true` if the `GcRef` points to a valid occupied slot.
    #[inline]
    pub fn is_valid(&self, r: GcRef<T>) -> bool {
        self.get(r).is_some()
    }

    /// Returns the GC color of the object, or `None` if the ref is stale.
    #[inline]
    pub fn color(&self, r: GcRef<T>) -> Option<Color> {
        let entry = self.entries.get(r.index as usize)?;
        if entry.generation != r.generation {
            return None;
        }
        byte_to_color(self.colors[r.index as usize])
    }

    /// Sets the GC color of the object. Returns `true` if successful.
    #[inline]
    pub fn set_color(&mut self, r: GcRef<T>, new_color: Color) -> bool {
        let Some(entry) = self.entries.get(r.index as usize) else {
            return false;
        };
        if entry.generation != r.generation {
            return false;
        }
        if self.colors[r.index as usize] == COLOR_FREE {
            return false;
        }
        self.colors[r.index as usize] = color_to_byte(new_color);
        true
    }

    /// Iterates over all occupied entries.
    ///
    /// Yields `(GcRef<T>, &T, Color)` for each live object. Used by the
    /// GC propagate phase to examine all objects in this arena.
    pub fn iter(&self) -> ArenaIter<'_, T> {
        ArenaIter {
            entries: &self.entries,
            colors: &self.colors,
            pos: 0,
        }
    }

    /// Resets the color of all occupied entries to the given color.
    ///
    /// Used during GC cycle initialization to reset all objects to the
    /// current white.
    pub fn reset_colors(&mut self, color: Color) {
        let byte = color_to_byte(color);
        for c in &mut self.colors {
            if *c != COLOR_FREE {
                *c = byte;
            }
        }
    }

    /// Sweeps the arena: frees objects with `dead_color`, resets survivors
    /// to `new_color`. Returns the number of freed objects.
    ///
    /// This is the core GC sweep operation. Dead objects (still bearing the
    /// "other white" from the previous cycle) are freed. Live objects
    /// (marked during the current cycle) are reset to the new current white.
    pub fn sweep(&mut self, dead_color: Color, new_color: Color) -> u32 {
        let dead_byte = color_to_byte(dead_color);
        let new_byte = color_to_byte(new_color);
        let mut local_free_head = self.free_head;
        let mut local_len = self.len;
        let mut freed = 0u32;

        for (i, (entry, color_byte)) in self
            .entries
            .iter_mut()
            .zip(self.colors.iter_mut())
            .enumerate()
        {
            if *color_byte == COLOR_FREE {
                continue;
            }

            if *color_byte == dead_byte {
                entry.state = EntryState::Free {
                    next_free: local_free_head,
                };
                entry.generation = entry.generation.wrapping_add(1);
                local_free_head = Some(i as u32);
                local_len -= 1;
                *color_byte = COLOR_FREE;
                freed += 1;
            } else {
                *color_byte = new_byte;
            }
        }

        self.free_head = local_free_head;
        self.len = local_len;
        freed
    }

    /// Sweeps up to `max_count` occupied slots starting from `start`,
    /// freeing dead objects and resetting survivors to `new_color`.
    ///
    /// Returns `(freed_count, next_position, is_done)`:
    /// - `freed_count`: number of objects freed in this batch
    /// - `next_position`: slot to resume from on the next call
    /// - `is_done`: `true` if the entire arena has been swept
    ///
    /// Used by the incremental GC to spread sweep work across multiple steps.
    #[inline]
    pub fn sweep_partial(
        &mut self,
        dead_color: Color,
        new_color: Color,
        start: u32,
        max_count: u32,
    ) -> (u32, u32, bool) {
        let start_usize = start as usize;
        let total = self.entries.len() as u32;
        let dead_byte = color_to_byte(dead_color);
        let new_byte = color_to_byte(new_color);
        let mut local_free_head = self.free_head;
        let mut local_len = self.len;
        let mut freed = 0u32;
        let mut occupied_seen = 0u32;
        let mut scanned = 0u32;

        let entries = &mut self.entries[start_usize..];
        let colors = &mut self.colors[start_usize..];

        // Iterator-based loop over zip of entries and colors eliminates
        // per-access bounds checks. Only Occupied entries count toward
        // max_count. Free entries are skipped at near-zero cost.
        for (entry, color_byte) in entries.iter_mut().zip(colors.iter_mut()) {
            if occupied_seen >= max_count {
                break;
            }
            let abs_index = start + scanned;
            scanned += 1;

            if *color_byte == COLOR_FREE {
                continue;
            }

            if *color_byte == dead_byte {
                entry.state = EntryState::Free {
                    next_free: local_free_head,
                };
                entry.generation = entry.generation.wrapping_add(1);
                local_free_head = Some(abs_index);
                local_len -= 1;
                *color_byte = COLOR_FREE;
                freed += 1;
            } else {
                *color_byte = new_byte;
            }
            occupied_seen += 1;
        }

        self.free_head = local_free_head;
        self.len = local_len;

        let next_pos = start + scanned;
        let is_done = next_pos >= total;
        (freed, next_pos, is_done)
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T> IntoIterator for &'a Arena<T> {
    type Item = (GcRef<T>, &'a T, Color);
    type IntoIter = ArenaIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

// ---------------------------------------------------------------------------
// ArenaIter
// ---------------------------------------------------------------------------

/// Iterator over occupied arena entries.
pub struct ArenaIter<'a, T> {
    entries: &'a [Entry<T>],
    colors: &'a [u8],
    pos: u32,
}

impl<'a, T> Iterator for ArenaIter<'a, T> {
    type Item = (GcRef<T>, &'a T, Color);

    fn next(&mut self) -> Option<Self::Item> {
        while (self.pos as usize) < self.entries.len() {
            let idx = self.pos;
            self.pos += 1;
            let color_byte = self.colors[idx as usize];
            if let Some(color) = byte_to_color(color_byte) {
                let entry = &self.entries[idx as usize];
                if let EntryState::Occupied { value } = &entry.state {
                    return Some((
                        GcRef {
                            index: idx,
                            generation: entry.generation,
                            _marker: PhantomData,
                        },
                        value,
                        color,
                    ));
                }
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.entries.len() - self.pos as usize;
        (0, Some(remaining))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_and_get() {
        let mut arena: Arena<i32> = Arena::new();
        let r = arena.alloc(42, Color::White0);
        assert_eq!(arena.get(r), Some(&42));
        assert_eq!(arena.len(), 1);
        assert!(!arena.is_empty());
    }

    #[test]
    fn alloc_and_get_mut() {
        let mut arena: Arena<i32> = Arena::new();
        let r = arena.alloc(42, Color::White0);
        if let Some(val) = arena.get_mut(r) {
            *val = 100;
        }
        assert_eq!(arena.get(r), Some(&100));
    }

    #[test]
    fn free_returns_value() {
        let mut arena: Arena<i32> = Arena::new();
        let r = arena.alloc(42, Color::White0);
        let val = arena.free(r);
        assert_eq!(val, Some(42));
        assert_eq!(arena.len(), 0);
        assert!(arena.is_empty());
    }

    #[test]
    fn stale_ref_after_free() {
        let mut arena: Arena<i32> = Arena::new();
        let r = arena.alloc(42, Color::White0);
        arena.free(r);
        assert_eq!(arena.get(r), None);
        assert_eq!(arena.get_mut(r), None);
        assert!(!arena.is_valid(r));
    }

    #[test]
    fn generation_prevents_aba() {
        let mut arena: Arena<i32> = Arena::new();
        let r1 = arena.alloc(42, Color::White0);
        arena.free(r1);
        let r2 = arena.alloc(99, Color::White0);
        // r1 and r2 point to the same index but different generations.
        assert_eq!(r1.index(), r2.index());
        assert_ne!(r1.generation(), r2.generation());
        assert_eq!(arena.get(r1), None);
        assert_eq!(arena.get(r2), Some(&99));
    }

    #[test]
    fn free_list_reuse() {
        let mut arena: Arena<i32> = Arena::new();
        let r1 = arena.alloc(1, Color::White0);
        let r2 = arena.alloc(2, Color::White0);
        let idx1 = r1.index();
        arena.free(r1);
        let r3 = arena.alloc(3, Color::White0);
        // Freed slot is reused.
        assert_eq!(r3.index(), idx1);
        assert_eq!(arena.get(r3), Some(&3));
        assert_eq!(arena.get(r2), Some(&2));
        assert_eq!(arena.capacity(), 2); // no growth
    }

    #[test]
    fn color_get_and_set() {
        let mut arena: Arena<i32> = Arena::new();
        let r = arena.alloc(42, Color::White0);
        assert_eq!(arena.color(r), Some(Color::White0));

        assert!(arena.set_color(r, Color::Gray));
        assert_eq!(arena.color(r), Some(Color::Gray));

        assert!(arena.set_color(r, Color::Black));
        assert_eq!(arena.color(r), Some(Color::Black));

        assert!(arena.set_color(r, Color::White1));
        assert_eq!(arena.color(r), Some(Color::White1));
    }

    #[test]
    fn color_stale_ref() {
        let mut arena: Arena<i32> = Arena::new();
        let r = arena.alloc(42, Color::White0);
        arena.free(r);
        assert_eq!(arena.color(r), None);
        assert!(!arena.set_color(r, Color::Gray));
    }

    #[test]
    fn double_free_returns_none() {
        let mut arena: Arena<i32> = Arena::new();
        let r = arena.alloc(42, Color::White0);
        assert_eq!(arena.free(r), Some(42));
        assert_eq!(arena.free(r), None);
    }

    #[test]
    fn multiple_alloc_free_cycles() {
        let mut arena: Arena<i32> = Arena::new();
        let mut refs = Vec::new();

        // Allocate 100 objects.
        for i in 0..100 {
            refs.push(arena.alloc(i, Color::White0));
        }
        assert_eq!(arena.len(), 100);
        assert_eq!(arena.capacity(), 100);

        // Free every other one.
        for i in (0..100).step_by(2) {
            arena.free(refs[i]);
        }
        assert_eq!(arena.len(), 50);

        // Allocate 50 more (should reuse freed slots, no growth).
        for i in 100..150 {
            arena.alloc(i, Color::White0);
        }
        assert_eq!(arena.len(), 100);
        assert_eq!(arena.capacity(), 100);
    }

    #[test]
    fn empty_arena() {
        let arena: Arena<i32> = Arena::new();
        assert!(arena.is_empty());
        assert_eq!(arena.len(), 0);
        assert_eq!(arena.capacity(), 0);
    }

    #[test]
    fn default_creates_empty() {
        let arena: Arena<i32> = Arena::default();
        assert!(arena.is_empty());
    }

    #[test]
    fn iter_yields_occupied_only() {
        let mut arena: Arena<i32> = Arena::new();
        let r1 = arena.alloc(10, Color::White0);
        let _r2 = arena.alloc(20, Color::Gray);
        let r3 = arena.alloc(30, Color::Black);

        // Free the middle one.
        arena.free(r1);

        let items: Vec<_> = arena.iter().collect();
        assert_eq!(items.len(), 2);
        // Remaining items are at indices 1 and 2.
        assert_eq!(*items[0].1, 20);
        assert_eq!(items[0].2, Color::Gray);
        assert_eq!(*items[1].1, 30);
        assert_eq!(items[1].2, Color::Black);
        // Verify the GcRef is valid.
        assert!(arena.is_valid(items[1].0));
        assert_eq!(items[1].0.index(), r3.index());
    }

    #[test]
    fn reset_colors() {
        let mut arena: Arena<i32> = Arena::new();
        let r1 = arena.alloc(1, Color::White0);
        let r2 = arena.alloc(2, Color::Black);
        let r3 = arena.alloc(3, Color::Gray);

        arena.reset_colors(Color::White1);

        assert_eq!(arena.color(r1), Some(Color::White1));
        assert_eq!(arena.color(r2), Some(Color::White1));
        assert_eq!(arena.color(r3), Some(Color::White1));
    }

    #[test]
    fn gcref_debug_format() {
        let r: GcRef<i32> = GcRef {
            index: 5,
            generation: 3,
            _marker: PhantomData,
        };
        let debug = format!("{r:?}");
        assert!(debug.contains('5'));
        assert!(debug.contains('3'));
    }

    #[test]
    fn gcref_equality_and_hash() {
        use std::collections::HashSet;

        let mut arena: Arena<i32> = Arena::new();
        let r1 = arena.alloc(1, Color::White0);
        let r2 = arena.alloc(2, Color::White0);

        assert_ne!(r1, r2);

        let r1_copy = r1;
        assert_eq!(r1, r1_copy);

        let mut set = HashSet::new();
        set.insert(r1);
        set.insert(r2);
        set.insert(r1_copy); // duplicate
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn generation_wrapping() {
        let mut arena: Arena<i32> = Arena::new();
        let r = arena.alloc(1, Color::White0);
        // Simulate many free/realloc cycles on the same slot.
        let mut last_ref = r;
        for i in 0..10 {
            arena.free(last_ref);
            last_ref = arena.alloc(i, Color::White0);
        }
        // Original ref is stale.
        assert_eq!(arena.get(r), None);
        // Latest ref is valid.
        assert_eq!(arena.get(last_ref), Some(&9));
    }
}
