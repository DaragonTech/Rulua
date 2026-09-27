//! String interning with cached hash.
//!
//! [`LuaString`] stores an immutable byte sequence with a precomputed hash.
//! All strings are interned through a [`StringTable`]: creating a string
//! that already exists returns the existing `GcRef` instead of allocating
//! a new one. This makes string equality an O(1) index comparison.
//!
//! ## Hash Algorithm
//!
//! The hash matches PUC-Rio's algorithm from `lstring.c`: seed with the
//! string length, then XOR-fold sampled characters walking backward with
//! step `(len >> 5) + 1`. For strings <= 31 bytes, every character is
//! hashed. For longer strings, approximately 32 characters are sampled.
//!
//! ## Interning Table
//!
//! [`StringTable`] is a power-of-2 hash table that maps string content
//! to `GcRef<LuaString>`. It starts at 32 buckets (`MINSTRTABSIZE`)
//! and doubles when the load factor exceeds 1.0.

use std::fmt;

use super::gc::Color;
use super::gc::arena::{Arena, GcRef};
use super::gc::trace::Trace;

// ---------------------------------------------------------------------------
// LuaString
// ---------------------------------------------------------------------------

/// A GC-managed Lua string with a cached hash.
///
/// Strings are immutable after creation. The `hash` field is precomputed
/// by [`lua_hash`] and cached for O(1) hash lookups. String content may
/// contain embedded null bytes (Lua strings are length-delimited, not
/// null-terminated).
pub struct LuaString {
    /// Precomputed hash (PUC-Rio algorithm).
    hash: u32,
    /// String content as a byte slice, followed by one extra NUL byte so
    /// that the C API (`lua_tolstring`) can hand out a stable,
    /// NUL-terminated pointer. The terminator is not part of the string.
    data: Box<[u8]>,
}

impl LuaString {
    /// Creates a new `LuaString` from raw bytes with the given precomputed hash.
    ///
    /// This is an internal constructor. Use [`StringTable::intern`] for
    /// proper interning and deduplication.
    pub(crate) fn new(data: &[u8], hash: u32) -> Self {
        let mut buf = Vec::with_capacity(data.len() + 1);
        buf.extend_from_slice(data);
        buf.push(0);
        Self {
            hash,
            data: buf.into_boxed_slice(),
        }
    }

    /// Returns the precomputed hash value.
    #[inline]
    pub fn hash(&self) -> u32 {
        self.hash
    }

    /// Returns the string content as a byte slice.
    #[inline]
    pub fn data(&self) -> &[u8] {
        &self.data[..self.data.len() - 1]
    }

    /// Returns a pointer to the NUL-terminated string bytes.
    ///
    /// The pointer stays valid for as long as the string object is alive
    /// (strings are immutable and their buffer is never reallocated).
    #[inline]
    pub fn as_c_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }

    /// Returns the length of the string in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len() - 1
    }

    /// Returns `true` if the string is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.len() == 1
    }

    /// Returns the string content as a `&str` if it is valid UTF-8.
    pub fn as_str(&self) -> Option<&str> {
        std::str::from_utf8(self.data()).ok()
    }
}

impl PartialEq for LuaString {
    /// Two `LuaString` values are equal if they have the same hash and
    /// identical byte content. In practice, interned strings are compared
    /// by `GcRef` identity (O(1)); this content comparison is a fallback
    /// for the interning lookup itself.
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.data == other.data
    }
}

impl Eq for LuaString {}

impl fmt::Debug for LuaString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.as_str() {
            Some(s) => write!(f, "LuaString({s:?})"),
            None => write!(f, "LuaString({:?})", self.data()),
        }
    }
}

impl fmt::Display for LuaString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(s) = self.as_str() {
            write!(f, "{s}")
        } else {
            // Non-UTF-8 strings: write bytes as lossy.
            let s = String::from_utf8_lossy(self.data());
            write!(f, "{s}")
        }
    }
}

impl Trace for LuaString {
    /// No-op: strings contain no GC references.
    fn trace(&self) {}

    fn needs_trace(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Hash algorithm (PUC-Rio lstring.c)
// ---------------------------------------------------------------------------

/// Compute the PUC-Rio Lua 5.1.1 string hash.
///
/// Algorithm from `luaS_newlstr` in `lstring.c`:
///
/// 1. Seed `h` with the string length.
/// 2. Compute step = `(len >> 5) + 1`.
/// 3. Walk backward from the end, XOR-folding sampled characters:
///    `h = h ^ ((h << 5) + (h >> 2) + byte)`.
///
/// For strings <= 31 bytes, every character is hashed (step = 1).
/// For longer strings, approximately 32 characters are sampled.
pub fn lua_hash(data: &[u8]) -> u32 {
    let len = data.len();
    let mut h = len as u32;
    let step = (len >> 5) + 1;
    let mut l1 = len;
    while l1 >= step {
        h ^= (h << 5)
            .wrapping_add(h >> 2)
            .wrapping_add(u32::from(data[l1 - 1]));
        l1 -= step;
    }
    h
}

// ---------------------------------------------------------------------------
// StringTable
// ---------------------------------------------------------------------------

/// Minimum number of buckets (matches PUC-Rio's `MINSTRTABSIZE`).
const MIN_STR_TAB_SIZE: usize = 32;

/// String interning table.
///
/// Maps string content to `GcRef<LuaString>` using a power-of-2 hash
/// table. Each bucket is a `Vec` of references whose hashes map to that
/// bucket index. The table doubles when the count exceeds the number of
/// buckets (100% load factor, matching PUC-Rio).
pub struct StringTable {
    buckets: Vec<Vec<GcRef<LuaString>>>,
    count: usize,
}

impl StringTable {
    /// Creates a new string table with `MINSTRTABSIZE` (32) buckets.
    pub fn new() -> Self {
        Self {
            buckets: vec![Vec::new(); MIN_STR_TAB_SIZE],
            count: 0,
        }
    }

    /// Interns a string: returns the existing `GcRef` if the same content
    /// exists, otherwise allocates a new `LuaString` in the arena.
    ///
    /// `current_white` is the GC color assigned to newly allocated strings.
    pub fn intern(
        &mut self,
        data: &[u8],
        arena: &mut Arena<LuaString>,
        current_white: Color,
    ) -> GcRef<LuaString> {
        let h = lua_hash(data);
        let bucket_idx = (h as usize) & (self.buckets.len() - 1);

        // Search existing strings in this bucket.
        // Check hash first, then length, then content (cheapest to most
        // expensive comparison). PUC-Rio checks length then memcmp
        // without an explicit hash check; both orders are correct since
        // all entries in a bucket already share the same bucket index.
        for &r in &self.buckets[bucket_idx] {
            if let Some(s) = arena.get(r)
                && s.hash == h
                && s.len() == data.len()
                && s.data() == data
            {
                // Found existing string.
                // Resurrection of dead strings will be added with the
                // GC collector (Phase 7). For now, all valid refs are
                // returned as-is.
                return r;
            }
        }

        // Not found: create a new interned string.
        let s = LuaString::new(data, h);
        let r = arena.alloc(s, current_white);
        self.buckets[bucket_idx].push(r);
        self.count += 1;

        // Resize if load factor exceeds 1.0.
        if self.count > self.buckets.len() {
            self.grow(arena);
        }

        r
    }

    /// Returns the number of interned strings.
    #[inline]
    pub fn count(&self) -> usize {
        self.count
    }

    /// Returns the number of buckets.
    #[inline]
    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }

    /// Doubles the bucket count and rehashes all entries.
    fn grow(&mut self, arena: &Arena<LuaString>) {
        let new_size = self.buckets.len().saturating_mul(2);
        // Guard against overflow (matches PUC-Rio's MAX_INT/2 check).
        if new_size <= self.buckets.len() {
            return;
        }
        self.rehash(new_size, arena);
    }

    /// Shrinks the table if the load factor is below 25%.
    ///
    /// Called after GC sweep to reclaim bucket memory. Does not shrink
    /// below `MINSTRTABSIZE * 2` (64 buckets, matching PUC-Rio).
    pub fn maybe_shrink(&mut self, arena: &Arena<LuaString>) {
        if self.count < self.buckets.len() / 4 && self.buckets.len() > MIN_STR_TAB_SIZE * 2 {
            self.rehash(self.buckets.len() / 2, arena);
        }
    }

    /// Removes a `GcRef` from its bucket.
    ///
    /// Called during GC sweep when a string is collected. Decrements the
    /// count. The caller is responsible for freeing the arena slot.
    pub fn remove(&mut self, r: GcRef<LuaString>, arena: &Arena<LuaString>) {
        let Some(s) = arena.get(r) else {
            return;
        };
        let bucket_idx = (s.hash as usize) & (self.buckets.len() - 1);
        let bucket = &mut self.buckets[bucket_idx];
        if let Some(pos) = bucket.iter().position(|&entry| entry == r) {
            bucket.swap_remove(pos);
            self.count -= 1;
        }
    }

    /// Retains only entries for which `predicate` returns `true`.
    ///
    /// Used by the GC sweep phase to remove entries whose arena slots
    /// have been freed.
    pub fn retain<F>(&mut self, predicate: F)
    where
        F: Fn(GcRef<LuaString>) -> bool,
    {
        let mut removed = 0usize;
        for bucket in &mut self.buckets {
            let before = bucket.len();
            bucket.retain(|&r| predicate(r));
            removed += before - bucket.len();
        }
        self.count = self.count.saturating_sub(removed);
    }

    /// Removes intern table entries that reference dead (freed) strings.
    ///
    /// After the GC sweep frees dead strings from the arena, those GcRefs
    /// become stale. This method removes them from the intern table so
    /// future lookups won't find freed slots.
    pub fn sweep_dead(&mut self, arena: &Arena<LuaString>) {
        self.retain(|r| arena.get(r).is_some());
    }

    /// Rehashes all entries into a new bucket array of the given size.
    fn rehash(&mut self, new_size: usize, arena: &Arena<LuaString>) {
        let mut new_buckets = vec![Vec::new(); new_size];
        for bucket in &self.buckets {
            for &r in bucket {
                if let Some(s) = arena.get(r) {
                    let new_idx = (s.hash as usize) & (new_size - 1);
                    new_buckets[new_idx].push(r);
                }
            }
        }
        self.buckets = new_buckets;
    }
}

impl Default for StringTable {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- lua_hash tests --

    #[test]
    fn hash_empty_string() {
        // Empty string: h = 0 (length), loop never runs (0 < step=1 is false).
        assert_eq!(lua_hash(b""), 0);
    }

    #[test]
    fn hash_single_char() {
        // len=1, h=1, step=1, l1=1: h = 1 ^ ((1<<5) + (1>>2) + 'a')
        // = 1 ^ (32 + 0 + 97) = 1 ^ 129 = 128
        assert_eq!(lua_hash(b"a"), 128);
    }

    #[test]
    fn hash_deterministic() {
        let h1 = lua_hash(b"hello");
        let h2 = lua_hash(b"hello");
        assert_eq!(h1, h2);
    }

    #[test]
    fn hash_different_strings_differ() {
        // Different content should (almost certainly) produce different hashes.
        assert_ne!(lua_hash(b"hello"), lua_hash(b"world"));
        assert_ne!(lua_hash(b"a"), lua_hash(b"b"));
    }

    #[test]
    fn hash_short_strings_sample_all() {
        // For len <= 31, step = 1, so all characters are hashed.
        // Changing any byte should change the hash.
        let h1 = lua_hash(b"abc");
        let h2 = lua_hash(b"aXc");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_long_string_samples() {
        // For a 64-byte string, step = (64 >> 5) + 1 = 3.
        // Only ~21 characters are sampled, not all 64.
        let mut data = vec![b'a'; 64];
        let h1 = lua_hash(&data);

        // Change a byte at a position that is NOT sampled.
        // Sampled positions walk backward from 63 with step 3:
        // 63, 60, 57, 54, 51, 48, 45, 42, 39, 36, 33, 30, 27, 24,
        // 21, 18, 15, 12, 9, 6, 3, 0.
        // Position 1 is NOT in that list.
        data[1] = b'Z';
        let h2 = lua_hash(&data);
        assert_eq!(h1, h2, "unsampled position should not affect hash");

        // Change a byte at a position that IS sampled (position 63).
        data[1] = b'a'; // reset
        data[63] = b'Z';
        let h3 = lua_hash(&data);
        assert_ne!(h1, h3, "sampled position should affect hash");
    }

    #[test]
    fn hash_embedded_null() {
        // Lua strings may contain embedded nulls.
        let h1 = lua_hash(b"ab\0cd");
        let h2 = lua_hash(b"ab\0ce");
        assert_ne!(h1, h2);
    }

    // -- LuaString tests --

    #[test]
    fn lua_string_accessors() {
        let s = LuaString::new(b"hello", lua_hash(b"hello"));
        assert_eq!(s.data(), b"hello");
        assert_eq!(s.len(), 5);
        assert!(!s.is_empty());
        assert_eq!(s.as_str(), Some("hello"));
        assert_eq!(s.hash(), lua_hash(b"hello"));
    }

    #[test]
    fn lua_string_empty() {
        let s = LuaString::new(b"", lua_hash(b""));
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
        assert_eq!(s.data(), b"");
    }

    #[test]
    fn lua_string_non_utf8() {
        let data = &[0xFF, 0xFE, 0x00, 0x80];
        let s = LuaString::new(data, lua_hash(data));
        assert_eq!(s.as_str(), None);
        assert_eq!(s.data(), data);
    }

    #[test]
    fn lua_string_equality() {
        let s1 = LuaString::new(b"hello", lua_hash(b"hello"));
        let s2 = LuaString::new(b"hello", lua_hash(b"hello"));
        let s3 = LuaString::new(b"world", lua_hash(b"world"));
        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn lua_string_debug() {
        let s = LuaString::new(b"test", lua_hash(b"test"));
        let debug = format!("{s:?}");
        assert!(debug.contains("test"));
    }

    #[test]
    fn lua_string_display() {
        let s = LuaString::new(b"hello world", lua_hash(b"hello world"));
        assert_eq!(format!("{s}"), "hello world");
    }

    #[test]
    fn lua_string_trace_is_noop() {
        let s = LuaString::new(b"test", lua_hash(b"test"));
        assert!(!s.needs_trace());
        s.trace(); // no-op, should not panic
    }

    // -- StringTable tests --

    #[test]
    fn intern_creates_string() {
        let mut arena = Arena::new();
        let mut table = StringTable::new();

        let r = table.intern(b"hello", &mut arena, Color::White0);
        let Some(s) = arena.get(r) else {
            unreachable!("arena lookup should succeed");
        };
        assert_eq!(s.data(), b"hello");
        assert_eq!(s.hash(), lua_hash(b"hello"));
        assert_eq!(table.count(), 1);
    }

    #[test]
    fn intern_deduplicates() {
        let mut arena = Arena::new();
        let mut table = StringTable::new();

        let r1 = table.intern(b"hello", &mut arena, Color::White0);
        let r2 = table.intern(b"hello", &mut arena, Color::White0);
        assert_eq!(r1, r2, "same content should return same GcRef");
        assert_eq!(table.count(), 1, "should not allocate twice");
        assert_eq!(arena.len(), 1);
    }

    #[test]
    fn intern_different_strings() {
        let mut arena = Arena::new();
        let mut table = StringTable::new();

        let r1 = table.intern(b"hello", &mut arena, Color::White0);
        let r2 = table.intern(b"world", &mut arena, Color::White0);
        assert_ne!(r1, r2);
        assert_eq!(table.count(), 2);
        assert_eq!(arena.len(), 2);
    }

    #[test]
    fn intern_empty_string() {
        let mut arena = Arena::new();
        let mut table = StringTable::new();

        let r = table.intern(b"", &mut arena, Color::White0);
        let Some(s) = arena.get(r) else {
            unreachable!("arena lookup should succeed");
        };
        assert!(s.is_empty());
        assert_eq!(table.count(), 1);
    }

    #[test]
    fn intern_with_embedded_null() {
        let mut arena = Arena::new();
        let mut table = StringTable::new();

        let data = b"hello\0world";
        let r = table.intern(data, &mut arena, Color::White0);
        let Some(s) = arena.get(r) else {
            unreachable!("arena lookup should succeed");
        };
        assert_eq!(s.data(), data);
        assert_eq!(s.len(), 11);
    }

    #[test]
    fn intern_triggers_resize() {
        let mut arena = Arena::new();
        let mut table = StringTable::new();
        let initial_buckets = table.bucket_count();
        assert_eq!(initial_buckets, MIN_STR_TAB_SIZE);

        // Intern enough strings to trigger resize (count > bucket_count).
        for i in 0..=initial_buckets {
            let data = format!("string_{i}");
            table.intern(data.as_bytes(), &mut arena, Color::White0);
        }

        assert_eq!(table.count(), initial_buckets + 1);
        assert_eq!(
            table.bucket_count(),
            initial_buckets * 2,
            "should have doubled"
        );

        // Verify all strings are still findable after resize.
        for i in 0..=initial_buckets {
            let data = format!("string_{i}");
            let r = table.intern(data.as_bytes(), &mut arena, Color::White0);
            let Some(s) = arena.get(r) else {
                unreachable!("arena lookup should succeed");
            };
            assert_eq!(s.data(), data.as_bytes());
        }
        // Count should not have increased (all were found).
        assert_eq!(table.count(), initial_buckets + 1);
    }

    #[test]
    fn remove_decrements_count() {
        let mut arena = Arena::new();
        let mut table = StringTable::new();

        let r1 = table.intern(b"hello", &mut arena, Color::White0);
        let r2 = table.intern(b"world", &mut arena, Color::White0);
        assert_eq!(table.count(), 2);

        table.remove(r1, &arena);
        assert_eq!(table.count(), 1);

        // r2 should still be findable.
        let r2_again = table.intern(b"world", &mut arena, Color::White0);
        assert_eq!(r2, r2_again);
        assert_eq!(table.count(), 1);
    }

    #[test]
    fn maybe_shrink_below_threshold() {
        let mut arena = Arena::new();
        let mut table = StringTable::new();

        // Grow the table first.
        let mut refs = Vec::new();
        for i in 0..65 {
            let data = format!("s{i}");
            refs.push(table.intern(data.as_bytes(), &mut arena, Color::White0));
        }
        // Should have grown past 64 buckets.
        assert!(table.bucket_count() > MIN_STR_TAB_SIZE * 2);
        let large_bucket_count = table.bucket_count();

        // Remove most strings to drop below 25% load.
        for &r in &refs[..60] {
            table.remove(r, &arena);
        }
        assert_eq!(table.count(), 5);

        table.maybe_shrink(&arena);
        assert!(
            table.bucket_count() < large_bucket_count,
            "should have shrunk"
        );

        // Remaining strings should still be findable.
        for &r in &refs[60..] {
            assert!(arena.get(r).is_some());
        }
    }

    #[test]
    fn no_shrink_below_minimum() {
        let mut arena = Arena::new();
        let mut table = StringTable::new();

        // Intern and remove a single string.
        let r = table.intern(b"x", &mut arena, Color::White0);
        table.remove(r, &arena);
        assert_eq!(table.count(), 0);

        // Should not shrink below MINSTRTABSIZE.
        table.maybe_shrink(&arena);
        assert_eq!(table.bucket_count(), MIN_STR_TAB_SIZE);
    }

    #[test]
    fn string_table_default() {
        let table = StringTable::default();
        assert_eq!(table.count(), 0);
        assert_eq!(table.bucket_count(), MIN_STR_TAB_SIZE);
    }

    #[test]
    fn interning_preserves_identity_across_types() {
        // Verify that interning the same content from different sources
        // always returns the same GcRef.
        let mut arena = Arena::new();
        let mut table = StringTable::new();

        let from_literal = table.intern(b"test", &mut arena, Color::White0);
        let from_string = table.intern(b"test", &mut arena, Color::White0);
        let from_vec = table.intern(b"test".as_ref(), &mut arena, Color::White0);

        assert_eq!(from_literal, from_string);
        assert_eq!(from_string, from_vec);
    }

    #[test]
    fn stress_many_strings() {
        let mut arena = Arena::new();
        let mut table = StringTable::new();

        // Intern 1000 unique strings.
        let mut refs = Vec::new();
        for i in 0..1000 {
            let data = format!("key_{i:04}");
            refs.push(table.intern(data.as_bytes(), &mut arena, Color::White0));
        }
        assert_eq!(table.count(), 1000);
        assert_eq!(arena.len(), 1000);

        // Verify all are still retrievable and deduplicated.
        for (i, &expected_ref) in refs.iter().enumerate() {
            let data = format!("key_{i:04}");
            let r = table.intern(data.as_bytes(), &mut arena, Color::White0);
            assert_eq!(r, expected_ref);
        }
        assert_eq!(table.count(), 1000);
    }
}
