//! Table implementation: array + hash dual representation.
//!
//! Tables are the sole data structure in Lua, used for arrays,
//! dictionaries, objects, modules, and namespaces. The implementation
//! uses a dual representation: a dense array part for integer keys
//! `1..=n` and a hash part for all other keys.
//!
//! ## Hash Part
//!
//! The hash part uses open-addressing with Brent's collision resolution.
//! All nodes reside in a single flat `Vec<Node>` whose size is always a
//! power of 2. Free slots are found by scanning backward from `last_free`.
//!
//! ## Key Constraints
//!
//! - **Nil keys** are invalid (runtime error).
//! - **NaN keys** are invalid (runtime error).
//! - **Integer-float equivalence**: `t[1]` and `t[1.0]` access the same
//!   slot. A number qualifies as integer if `n as i64 as f64 == n`.

use crate::error::{LuaError, LuaResult, RuntimeError};

use super::gc::arena::{Arena, GcRef};
use super::gc::trace::Trace;
use super::string::LuaString;
use super::value::Val;

// ---------------------------------------------------------------------------
// Node (hash part entry)
// ---------------------------------------------------------------------------

/// A single node in the hash part of a table.
///
/// Each node holds a key-value pair and a chain pointer to the next
/// node in the collision chain. A node with `key == Val::Nil` is free.
struct Node {
    key: Val,
    value: Val,
    next: Option<u32>,
}

impl Node {
    /// Creates an empty (free) node.
    const fn empty() -> Self {
        Self {
            key: Val::Nil,
            value: Val::Nil,
            next: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Hash helpers
// ---------------------------------------------------------------------------

/// Compute the PUC-Rio number hash: reinterpret `(n + 1.0)` bits as
/// two u32s and sum them. The `+1.0` normalizes `-0.0` to `+1.0`,
/// ensuring `-0.0` and `+0.0` hash identically.
fn hash_number(n: f64) -> u32 {
    let n = n + 1.0;
    let bits = n.to_bits();
    let lo = bits as u32;
    let hi = (bits >> 32) as u32;
    lo.wrapping_add(hi)
}

/// Power-of-2 modulus for string and boolean keys.
///
/// `hash & (size - 1)`. Requires `size` to be a power of 2.
#[inline]
fn hash_pow2(hash: u32, size: u32) -> u32 {
    hash & size.wrapping_sub(1)
}

/// Odd modulus for number and pointer-like keys.
///
/// `hash % ((size - 1) | 1)`. The `| 1` ensures the divisor is odd,
/// giving better distribution than pure power-of-2 modulus for values
/// that tend to be even. Matches PUC-Rio's `hashmod` macro.
#[inline]
fn hash_mod(hash: u32, size: u32) -> u32 {
    let divisor = size.wrapping_sub(1) | 1;
    hash % divisor
}

/// Convert an f64 to an integer key if it is exactly representable.
///
/// Returns `Some(k)` if `n as i64 as f64 == n` (exact round-trip).
/// Returns `None` for non-integers, NaN, infinity, and values outside
/// i64 range.
fn number_to_int_key(n: f64) -> Option<i64> {
    if !n.is_finite() {
        return None;
    }
    let k = n as i64;
    // Exact comparison is intentional: Lua requires the float to be
    // precisely representable as an integer (round-trip check).
    #[allow(clippy::float_cmp)]
    if (k as f64) == n { Some(k) } else { None }
}

/// Ceiling of log2 for hash table sizing.
///
/// Returns 0 for n <= 1. Result is the smallest `k` such that `2^k >= n`.
fn ceil_log2(n: usize) -> u8 {
    if n <= 1 {
        return 0;
    }
    (usize::BITS - (n - 1).leading_zeros()) as u8
}

/// Maximum number of bits for the array part.
///
/// Limits the array part to `2^MAXBITS` entries. Matches PUC-Rio's
/// `MAXBITS` which defaults to `LUAI_BITSINT - 2` (26 on 32-bit).
const MAXBITS: u8 = 26;

/// Maximum array part size: `2^MAXBITS`.
const MAXASIZE: usize = 1 << MAXBITS;

/// Create a runtime error with the given message.
fn runtime_error(msg: &str) -> LuaError {
    LuaError::Runtime(RuntimeError {
        message: msg.into(),
        level: 0,
        traceback: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Table
// ---------------------------------------------------------------------------

/// A Lua table with array + hash parts.
///
/// The array part stores values for integer keys `1..=n`. The hash part
/// uses open-addressing with Brent's collision resolution for all other
/// keys. Both parts may be empty.
pub struct Table {
    /// Array part: values for integer keys 1..=array.len().
    array: Vec<Val>,
    /// Hash part: open-addressing with collision chaining.
    nodes: Vec<Node>,
    /// Next free position scan point (scans backward from here).
    last_free: u32,
    /// log2 of nodes.len(). 0 when hash part is empty.
    log2_size: u8,
    /// Optional metatable (GC-managed).
    metatable: Option<GcRef<Self>>,
    /// Fast negative cache for metamethods.
    ///
    /// Bits 0-4 cache absence of events Index, NewIndex, GC, Mode, Eq.
    /// Set by gettm() when a metamethod is NOT found; cleared to 0 on
    /// any rawset where the key starts with `__`.
    flags: u8,
}

impl Table {
    /// Creates an empty table with no array or hash part.
    pub fn new() -> Self {
        Self {
            array: Vec::new(),
            nodes: Vec::new(),
            last_free: 0,
            log2_size: 0,
            metatable: None,
            flags: 0,
        }
    }

    /// Creates a table with pre-allocated array and hash parts.
    ///
    /// `hash_size` is rounded up to the next power of 2. If `hash_size`
    /// is 0, the hash part is empty. Array slots are initialized to nil.
    pub fn with_sizes(array_size: usize, hash_size: usize) -> Self {
        let array = vec![Val::Nil; array_size];

        if hash_size == 0 {
            Self {
                array,
                nodes: Vec::new(),
                last_free: 0,
                log2_size: 0,
                metatable: None,
                flags: 0,
            }
        } else {
            let log2 = ceil_log2(hash_size);
            let actual_size = 1u32 << log2;
            let mut nodes = Vec::with_capacity(actual_size as usize);
            for _ in 0..actual_size {
                nodes.push(Node::empty());
            }
            Self {
                array,
                nodes,
                last_free: actual_size,
                log2_size: log2,
                metatable: None,
                flags: 0,
            }
        }
    }

    /// Returns the size of the array part.
    #[inline]
    pub fn array_len(&self) -> usize {
        self.array.len()
    }

    /// Returns estimated memory usage of this table's array + hash parts.
    ///
    /// Used for memory tracking when tables resize after `raw_set`.
    pub fn estimated_memory(&self) -> usize {
        self.array.len() * 16 + self.hash_size() as usize * 32
    }

    /// Returns the size of the hash part (always a power of 2, or 0).
    #[inline]
    pub fn hash_size(&self) -> u32 {
        if self.log2_size == 0 && self.nodes.is_empty() {
            0
        } else {
            1u32 << self.log2_size
        }
    }

    /// Returns the metatable, if set.
    #[inline]
    pub fn metatable(&self) -> Option<GcRef<Self>> {
        self.metatable
    }

    /// Sets or clears the metatable. Invalidates the flags cache.
    #[inline]
    pub fn set_metatable(&mut self, mt: Option<GcRef<Self>>) {
        self.metatable = mt;
        self.flags = 0; // Invalidate metamethod cache
    }

    /// Returns a slice of the array part for GC traversal.
    ///
    /// The collector uses this to mark all values in the array part.
    #[inline]
    pub fn array_slice(&self) -> &[Val] {
        &self.array
    }

    /// Collects all occupied key-value pairs from the hash part.
    ///
    /// Used by the GC collector to mark hash entries. Returns only non-nil
    /// key entries (skips free nodes).
    pub fn hash_entries(&self) -> Vec<(Val, Val)> {
        let mut entries = Vec::new();
        for node in &self.nodes {
            if !node.key.is_nil() {
                entries.push((node.key, node.value));
            }
        }
        entries
    }

    /// Returns the `last_free` index (offset from start of nodes array).
    ///
    /// Used by the test library's `T.querytab` to match PUC-Rio's
    /// `t->lastfree - t->node` behavior.
    pub(crate) fn last_free_index(&self) -> u32 {
        self.last_free
    }

    /// Queries a hash node by index. Returns (key, value, next) if the
    /// index is valid.
    ///
    /// Used by the test library's `T.querytab` for hash part inspection.
    /// The `next` field is returned as `Option<u32>` matching the internal
    /// chain pointer (node index, not pointer offset).
    pub(crate) fn query_node(&self, idx: u32) -> Option<(Val, Val, Option<u32>)> {
        let node = self.nodes.get(idx as usize)?;
        Some((node.key, node.value, node.next))
    }

    /// Returns the value in the array part at the given 0-based index.
    ///
    /// Used by the test library's `T.querytab` for array part inspection.
    pub(crate) fn array_get(&self, idx: usize) -> Option<Val> {
        self.array.get(idx).copied()
    }

    /// Extends the array part to at least `min_size` entries, filling
    /// new slots with nil.
    ///
    /// Matches PUC-Rio's `luaH_resizearray` for the SETLIST case: the
    /// array is pre-allocated to the exact number of entries being
    /// written, avoiding a rehash that would round up to a power of 2.
    pub(crate) fn ensure_array_capacity(&mut self, min_size: usize) {
        if min_size > self.array.len() {
            self.array.resize(min_size, Val::Nil);
        }
    }

    /// Returns the number of hash nodes (including free slots).
    ///
    /// Used by GC traversal to iterate hash entries by index without
    /// allocating a Vec.
    #[inline]
    pub fn hash_node_count(&self) -> u32 {
        self.nodes.len() as u32
    }

    /// Returns the key-value pair at hash node index `idx`, if the node
    /// is occupied (key is not nil).
    ///
    /// Used by GC traversal for zero-allocation iteration.
    #[inline]
    pub fn hash_node_kv(&self, idx: u32) -> Option<(Val, Val)> {
        let node = self.nodes.get(idx as usize)?;
        if node.key.is_nil() {
            None
        } else {
            Some((node.key, node.value))
        }
    }

    /// Returns indices of hash nodes that contain dead references in weak tables.
    ///
    /// For `weak_keys`: a node is dead if its key is dead.
    /// For `weak_values`: a node is dead if its value is dead.
    /// The `is_dead` predicate checks whether a given Val is a dead collectable.
    pub fn find_dead_hash_entries<F>(
        &self,
        weak_keys: bool,
        weak_values: bool,
        is_dead: F,
    ) -> Vec<usize>
    where
        F: Fn(Val, bool) -> bool,
    {
        let mut dead = Vec::new();
        for (i, node) in self.nodes.iter().enumerate() {
            if node.key.is_nil() {
                continue;
            }
            if (weak_keys && is_dead(node.key, true)) || (weak_values && is_dead(node.value, false))
            {
                dead.push(i);
            }
        }
        dead
    }

    /// Sets the array element at `idx` to Nil (for weak-table clearing).
    pub fn nil_array_entry(&mut self, idx: usize) {
        if idx < self.array.len() {
            self.array[idx] = Val::Nil;
        }
    }

    /// Nils out hash entries at the given node indices (for weak-table clearing).
    ///
    /// Sets both key and value to Nil and clears the next pointer, effectively
    /// removing the entries from the hash chain.
    pub fn nil_hash_entries(&mut self, indices: &[usize]) {
        for &idx in indices {
            if idx < self.nodes.len() {
                // Only nil the value, keeping the key and chain intact.
                // This matches PUC-Rio's removeentry() which sets the key
                // type to LUA_TDEADKEY but preserves the GC pointer so
                // that next() can still find the entry position during
                // iteration. Our next() skips entries with nil values.
                self.nodes[idx].value = Val::Nil;
            }
        }
    }

    /// Returns the flags byte (metamethod absence cache).
    #[inline]
    pub fn flags(&self) -> u8 {
        self.flags
    }

    /// Sets a bit in the flags byte to cache the absence of a metamethod.
    #[inline]
    pub fn set_tm_flag(&mut self, event: u8) {
        if event <= 4 {
            self.flags |= 1u8 << event;
        }
    }

    /// Invalidates the metamethod cache. Called when a `__`-prefixed key
    /// is rawset into the table.
    #[inline]
    pub fn invalidate_tm_cache(&mut self) {
        self.flags = 0;
    }

    // -----------------------------------------------------------------------
    // Resize / rehash
    // -----------------------------------------------------------------------

    /// Count a candidate integer key into the `nums` histogram.
    ///
    /// If `key` is a positive integer in `[1, MAXASIZE]`, increment the
    /// corresponding bucket in `nums` and return 1. Otherwise return 0.
    /// Bucket `i` counts keys in the range `(2^(i-1), 2^i]`.
    fn count_int(key: &Val, nums: &mut [u32; MAXBITS as usize + 1]) -> u32 {
        if let Val::Num(n) = key
            && let Some(k) = number_to_int_key(*n)
            && k >= 1
            && (k as usize) <= MAXASIZE
        {
            nums[ceil_log2(k as usize) as usize] += 1;
            return 1;
        }
        0
    }

    /// Count integer keys currently in the array part.
    ///
    /// Populates `nums[i]` with the count of non-nil values in the range
    /// `(2^(i-1), 2^i]` of the array part. Returns the total count of
    /// non-nil array entries.
    fn num_use_array(&self, nums: &mut [u32; MAXBITS as usize + 1]) -> u32 {
        let mut ause: u32 = 0;
        let mut i: usize = 1; // 1-based array index
        for lg in 0..=MAXBITS {
            let ttlg = 1usize << lg;
            let lim = ttlg.min(self.array.len());
            if i > lim {
                break;
            }
            let mut lc: u32 = 0;
            while i <= lim {
                if !self.array[i - 1].is_nil() {
                    lc += 1;
                }
                i += 1;
            }
            nums[lg as usize] += lc;
            ause += lc;
        }
        ause
    }

    /// Count integer keys currently in the hash part.
    ///
    /// For each non-nil hash entry, checks if the key is a valid integer
    /// and updates `nums`. Returns the total number of non-nil entries in
    /// the hash part (both integer and non-integer keys). The count of
    /// integer keys found is added to `na_size`.
    fn num_use_hash(&self, nums: &mut [u32; MAXBITS as usize + 1], na_size: &mut u32) -> u32 {
        let mut total_use: u32 = 0;
        for node in self.nodes.iter().rev() {
            if !node.value.is_nil() {
                *na_size += Self::count_int(&node.key, nums);
                total_use += 1;
            }
        }
        total_use
    }

    /// Compute the optimal array size from the key distribution histogram.
    ///
    /// Finds the largest power of 2, `n`, such that more than half of the
    /// slots `[1, n]` are occupied. Returns the count of keys that will go
    /// into the array part.
    fn compute_sizes(nums: &[u32; MAXBITS as usize + 1], na_size: &mut u32) -> u32 {
        let mut a: u32 = 0; // cumulative count
        let mut na: u32 = 0; // count for best size
        let mut best_size: u32 = 0; // best array size found
        let mut twotoi: u32 = 1;
        for num in nums {
            if *num > 0 {
                a += *num;
                // More than half of [1, twotoi] occupied?
                if a > twotoi / 2 {
                    best_size = twotoi;
                    na = a;
                }
            }
            if a == *na_size {
                break; // All integer keys counted.
            }
            twotoi = twotoi.saturating_mul(2);
        }
        *na_size = best_size;
        na
    }

    /// Trigger a full rehash, recomputing the optimal array/hash split.
    ///
    /// Called from `new_key` when no free hash slots remain. The extra
    /// key (`ek`) is the key about to be inserted; it is counted in the
    /// histogram so the new table is sized to fit it.
    fn rehash(&mut self, ek: &Val, strings: &Arena<LuaString>) -> LuaResult<()> {
        let mut nums = [0u32; MAXBITS as usize + 1];

        // Phase 1: count integer keys in array part.
        let mut na_size = self.num_use_array(&mut nums);
        let mut total_use = na_size;

        // Phase 2: count all entries in hash, tagging integer keys.
        total_use += self.num_use_hash(&mut nums, &mut na_size);

        // Phase 3: count the new key being inserted.
        na_size += Self::count_int(ek, &mut nums);
        total_use += 1;

        // Phase 4: compute optimal array size.
        let na = Self::compute_sizes(&nums, &mut na_size);

        // Phase 5: resize with new configuration.
        // Hash needs to hold (total_use - na) entries.
        let nh_size = total_use - na;
        self.resize(na_size as usize, nh_size as usize, strings)
    }

    /// Resize the table to the given array and hash sizes.
    ///
    /// Rebuilds both parts, re-inserting all entries. The hash size is
    /// rounded up to the next power of 2 internally.
    fn resize(
        &mut self,
        new_array_size: usize,
        new_hash_size: usize,
        strings: &Arena<LuaString>,
    ) -> LuaResult<()> {
        let old_array_size = self.array.len();

        // Step 1: Grow array if needed (extend with nil).
        if new_array_size > old_array_size {
            self.array.resize(new_array_size, Val::Nil);
        }

        // Step 2: Create new hash part.
        let old_nodes = if new_hash_size == 0 {
            let old = core::mem::take(&mut self.nodes);
            self.log2_size = 0;
            self.last_free = 0;
            old
        } else {
            let log2 = ceil_log2(new_hash_size);
            if log2 > MAXBITS {
                return Err(runtime_error("table overflow"));
            }
            let actual_size = 1u32 << log2;
            let mut new_nodes = Vec::with_capacity(actual_size as usize);
            for _ in 0..actual_size {
                new_nodes.push(Node::empty());
            }
            let old = core::mem::replace(&mut self.nodes, new_nodes);
            self.log2_size = log2;
            self.last_free = actual_size;
            old
        };

        // Step 3: Shrink array if needed. Move displaced entries to hash.
        if new_array_size < old_array_size {
            for i in new_array_size..old_array_size {
                if !self.array[i].is_nil() {
                    let key = Val::Num((i + 1) as f64);
                    let val = self.array[i];
                    let node_idx = self.new_key(key, strings)?;
                    self.nodes[node_idx as usize].value = val;
                }
            }
            self.array.truncate(new_array_size);
        }

        // Step 4: Re-insert all old hash entries (backward for chain
        // ordering compatibility with PUC-Rio).
        for node in old_nodes.into_iter().rev() {
            if !node.value.is_nil() {
                // Try to place integer keys back into the (possibly larger)
                // array part first via raw_set.
                self.raw_set_impl(node.key, node.value, strings, false)?;
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Hash position computation
    // -----------------------------------------------------------------------

    /// Compute the main (home) position for a key in the hash part.
    ///
    /// Returns the bucket index where this key should ideally reside.
    /// Different key types use different hash methods matching PUC-Rio.
    pub(crate) fn main_position(&self, key: &Val, strings: &Arena<LuaString>) -> u32 {
        let size = self.hash_size();
        debug_assert!(size > 0, "main_position called on empty hash");
        match key {
            Val::Num(n) => hash_mod(hash_number(*n), size),
            Val::Str(r) => {
                let h = strings.get(*r).map_or(0, LuaString::hash);
                hash_pow2(h, size)
            }
            Val::Bool(b) => hash_pow2(u32::from(*b), size),
            Val::LightUserdata(p) => hash_mod(*p as u32, size),
            Val::Table(r) => hash_mod(r.index(), size),
            Val::Function(r) => hash_mod(r.index(), size),
            Val::Userdata(r) => hash_mod(r.index(), size),
            Val::Thread(r) => hash_mod(r.index(), size),
            Val::Nil => 0,
        }
    }

    // -----------------------------------------------------------------------
    // Free position scanning
    // -----------------------------------------------------------------------

    /// Scan backward from `last_free` to find a free node.
    ///
    /// A node is free if its key is nil. Returns `None` when no free
    /// positions remain (table is full, needs rehash).
    fn get_free_pos(&mut self) -> Option<u32> {
        while self.last_free > 0 {
            self.last_free -= 1;
            if self.nodes[self.last_free as usize].key.is_nil() {
                return Some(self.last_free);
            }
        }
        None
    }

    // -----------------------------------------------------------------------
    // Get operations
    // -----------------------------------------------------------------------

    /// Look up an integer key, checking array part first.
    ///
    /// This is the fast path for integer-keyed access. Uses the unsigned
    /// trick `(key - 1) as u64 < array_len` to simultaneously check
    /// `key >= 1` and `key <= array_len`.
    #[inline]
    pub fn get_int(&self, key: i64) -> Val {
        // Unsigned trick: negative keys and 0 wrap to large values.
        let idx = (key as u64).wrapping_sub(1);
        if idx < self.array.len() as u64 {
            return self.array[idx as usize];
        }

        // Hash part lookup for out-of-range integer keys.
        if self.nodes.is_empty() {
            return Val::Nil;
        }

        let nk = key as f64;
        let mp = hash_mod(hash_number(nk), self.hash_size());
        let mut cur = Some(mp);
        while let Some(i) = cur {
            let node = &self.nodes[i as usize];
            // Exact float comparison: Lua requires integer keys stored as
            // f64 to match precisely (same bit pattern via hash_number).
            #[allow(clippy::float_cmp)]
            if let Val::Num(n) = node.key
                && n == nk
            {
                return node.value;
            }
            cur = node.next;
        }

        Val::Nil
    }

    /// Look up a string key by interned reference.
    ///
    /// Uses the string's cached hash for bucket lookup, then compares
    /// by `GcRef` identity (interning guarantees unique refs).
    #[inline]
    pub fn get_str(&self, key: GcRef<LuaString>, strings: &Arena<LuaString>) -> Val {
        if self.nodes.is_empty() {
            return Val::Nil;
        }

        let Some(s) = strings.get(key) else {
            return Val::Nil;
        };

        let mp = hash_pow2(s.hash(), self.hash_size());
        let mut cur = Some(mp);
        while let Some(i) = cur {
            let node = &self.nodes[i as usize];
            if node.key == Val::Str(key) {
                return node.value;
            }
            cur = node.next;
        }

        Val::Nil
    }

    /// Generic key lookup dispatching by type.
    ///
    /// For number keys, tries integer fast path first. For string keys,
    /// uses pointer-identity comparison. For nil, returns nil immediately.
    #[inline]
    pub fn get(&self, key: Val, strings: &Arena<LuaString>) -> Val {
        match key {
            Val::Nil => Val::Nil,
            Val::Str(r) => self.get_str(r, strings),
            Val::Num(n) => {
                if let Some(k) = number_to_int_key(n) {
                    self.get_int(k)
                } else {
                    self.get_hash(key, strings)
                }
            }
            _ => self.get_hash(key, strings),
        }
    }

    /// Walk the hash chain for a generic key.
    #[inline]
    fn get_hash(&self, key: Val, strings: &Arena<LuaString>) -> Val {
        if self.nodes.is_empty() {
            return Val::Nil;
        }

        let mp = self.main_position(&key, strings);
        let mut cur = Some(mp);
        while let Some(i) = cur {
            let node = &self.nodes[i as usize];
            if node.key == key {
                return node.value;
            }
            cur = node.next;
        }

        Val::Nil
    }

    // -----------------------------------------------------------------------
    // Length operator (#)
    // -----------------------------------------------------------------------

    /// Compute the table length (the `#` operator).
    ///
    /// Finds a boundary: an integer index `n` such that `t[n] ~= nil`
    /// and `t[n+1] == nil`, or 0 if `t[1] == nil`.
    ///
    /// For contiguous arrays starting at 1, this returns the element
    /// count. For tables with holes, the result is unspecified (any
    /// boundary is valid per the Lua 5.1 spec).
    pub fn len(&self, strings: &Arena<LuaString>) -> usize {
        let j = self.array.len();

        // Case 1: Boundary exists in array part (last slot is nil).
        if j > 0 && self.array[j - 1].is_nil() {
            // Binary search: invariant is array[i] is non-nil (or i=0),
            // array[j-1] is nil.
            let mut lo: usize = 0;
            let mut hi: usize = j;
            while hi - lo > 1 {
                let m = usize::midpoint(lo, hi);
                if self.array[m - 1].is_nil() {
                    hi = m;
                } else {
                    lo = m;
                }
            }
            return lo;
        }

        // Case 2: Array is fully populated (or empty).
        if self.nodes.is_empty() {
            // No hash part — return array size.
            return j;
        }

        // Case 3: Search beyond array into hash part.
        self.unbound_search(j, strings)
    }

    /// Exponential + binary search for a boundary beyond the array part.
    ///
    /// Called when the array part is fully occupied and the hash part
    /// exists. Finds the boundary among integer keys stored in the hash.
    fn unbound_search(&self, array_size: usize, strings: &Arena<LuaString>) -> usize {
        let mut i = array_size; // Last known present.
        let mut j = array_size + 1; // First candidate.

        // Phase 1: Exponential probe — double j until t[j] is nil.
        while self.get(Val::Num(j as f64), strings) != Val::Nil {
            i = j;
            j = match j.checked_mul(2) {
                Some(doubled) => doubled,
                None => {
                    // Overflow guard: fall back to linear scan.
                    return self.linear_boundary_search(strings);
                }
            };
        }

        // Phase 2: Binary search between i (present) and j (absent).
        while j - i > 1 {
            let m = usize::midpoint(i, j);
            if self.get(Val::Num(m as f64), strings) == Val::Nil {
                j = m;
            } else {
                i = m;
            }
        }
        i
    }

    /// Linear fallback for boundary search when exponential probe overflows.
    fn linear_boundary_search(&self, strings: &Arena<LuaString>) -> usize {
        let mut i: usize = 1;
        while self.get(Val::Num(i as f64), strings) != Val::Nil {
            i += 1;
        }
        i - 1
    }

    // -----------------------------------------------------------------------
    // Table traversal (next)
    // -----------------------------------------------------------------------

    /// Find the next key-value pair after `key` in the table.
    ///
    /// Returns `Some((next_key, next_value))` or `None` if no more entries.
    /// Pass `Val::Nil` as the key to start iteration from the beginning.
    ///
    /// Iteration order: array entries first (indices 1, 2, ...), then
    /// hash entries in hash-internal order.
    ///
    /// Returns `Err` if the key is not found in the table (the table was
    /// modified during iteration in a way that invalidated the key).
    #[inline]
    pub fn next(&self, key: Val, strings: &Arena<LuaString>) -> LuaResult<Option<(Val, Val)>> {
        let idx = self.find_index(key, strings)?;

        // Advance to next position. Wrapping handles the nil-start
        // sentinel (usize::MAX wraps to 0).
        let start = idx.wrapping_add(1);

        // Scan array part from start onward.
        if start < self.array.len() {
            for i in start..self.array.len() {
                if !self.array[i].is_nil() {
                    return Ok(Some((Val::Num((i + 1) as f64), self.array[i])));
                }
            }
        }

        // Scan hash part. Compute starting hash index.
        let hash_start = if start >= self.array.len() {
            start.wrapping_sub(self.array.len())
        } else {
            0
        };

        for i in hash_start..self.nodes.len() {
            let node = &self.nodes[i];
            if !node.value.is_nil() {
                return Ok(Some((node.key, node.value)));
            }
        }

        Ok(None) // No more entries.
    }

    /// Convert a key to a unified iteration index.
    ///
    /// Unified index space:
    /// - Array: 0 .. array_len - 1
    /// - Hash:  array_len .. array_len + hash_size - 1
    ///
    /// Nil key returns `usize::MAX` (will wrap to 0 when +1 is applied,
    /// starting iteration at the beginning).
    fn find_index(&self, key: Val, strings: &Arena<LuaString>) -> LuaResult<usize> {
        // Nil = start of iteration.
        if key.is_nil() {
            // Return a value that, when +1 is computed, gives 0.
            // We use wrapping arithmetic: usize::MAX + 1 = 0.
            return Ok(usize::MAX);
        }

        // Try array index.
        if let Val::Num(n) = key
            && let Some(k) = number_to_int_key(n)
        {
            let idx = (k as u64).wrapping_sub(1);
            if idx < self.array.len() as u64 {
                return Ok(idx as usize);
            }
        }

        // Search hash part.
        if !self.nodes.is_empty() {
            let mp = self.main_position(&key, strings);
            let mut cur = Some(mp);
            while let Some(i) = cur {
                if self.nodes[i as usize].key == key {
                    return Ok(i as usize + self.array.len());
                }
                cur = self.nodes[i as usize].next;
            }
        }

        Err(runtime_error("invalid key to 'next'"))
    }

    // -----------------------------------------------------------------------
    // Set operations
    // -----------------------------------------------------------------------

    /// Set a key-value pair in the table (raw, no metamethods).
    ///
    /// Validates the key: nil and NaN keys produce runtime errors.
    /// Setting a value to nil for an existing key leaves the slot
    /// (value becomes nil but key remains). Setting nil for a
    /// non-existent key is a no-op.
    ///
    /// Invalidates the metamethod flags cache if the key is a string
    /// starting with `__`.
    pub fn raw_set(&mut self, key: Val, value: Val, strings: &Arena<LuaString>) -> LuaResult<()> {
        // Invalidate metamethod cache for `__` keys.
        if let Val::Str(r) = key
            && let Some(s) = strings.get(r)
            && s.data().starts_with(b"__")
        {
            self.flags = 0;
        }
        self.raw_set_impl(key, value, strings, true)
    }

    /// Inner implementation of `raw_set`. The `allow_rehash` flag
    /// prevents infinite recursion: the first call allows rehash, the
    /// retry after rehash does not.
    fn raw_set_impl(
        &mut self,
        key: Val,
        value: Val,
        strings: &Arena<LuaString>,
        allow_rehash: bool,
    ) -> LuaResult<()> {
        // Validate key.
        match key {
            Val::Nil => return Err(runtime_error("table index is nil")),
            Val::Num(n) if n.is_nan() => return Err(runtime_error("table index is NaN")),
            _ => {}
        }

        // Try array part for integer keys.
        if let Val::Num(n) = key
            && let Some(k) = number_to_int_key(n)
        {
            let idx = (k as u64).wrapping_sub(1);
            if idx < self.array.len() as u64 {
                self.array[idx as usize] = value;
                return Ok(());
            }
        }

        // Try existing slot in hash part.
        if !self.nodes.is_empty() {
            let mp = self.main_position(&key, strings);
            let mut cur = Some(mp);
            while let Some(i) = cur {
                if self.nodes[i as usize].key == key {
                    self.nodes[i as usize].value = value;
                    return Ok(());
                }
                cur = self.nodes[i as usize].next;
            }
        }

        // Insert new key via Brent's algorithm.
        // Note: PUC-Rio's luaH_set allocates a hash entry even when
        // the value is nil. This is necessary so nil-assignments to
        // non-existing keys can fill the hash and trigger rehash,
        // compacting the table. Skipping would prevent rehash.
        match self.new_key(key, strings) {
            Ok(node_idx) => {
                self.nodes[node_idx as usize].value = value;
                Ok(())
            }
            Err(_) if allow_rehash => {
                // Hash is empty or full. Rehash and retry through the
                // full raw_set path (key may land in array after rehash).
                self.rehash(&key, strings)?;
                self.raw_set_impl(key, value, strings, false)
            }
            Err(e) => Err(e),
        }
    }

    /// Insert a new key into the hash part using Brent's algorithm.
    ///
    /// Returns the node index where the key was placed. The value slot
    /// is left as nil; the caller writes the value.
    ///
    /// Returns an error if the hash part is empty or full. The caller
    /// is responsible for triggering rehash and retrying.
    fn new_key(&mut self, key: Val, strings: &Arena<LuaString>) -> LuaResult<u32> {
        if self.nodes.is_empty() {
            return Err(runtime_error("table overflow"));
        }

        let mp = self.main_position(&key, strings);

        // If the main position is free, place the key directly.
        if self.nodes[mp as usize].key.is_nil() {
            self.nodes[mp as usize].key = key;
            return Ok(mp);
        }

        // Check if the occupant is a dead entry: a stale string key
        // (GC-swept) with nil value. Dead entries can be safely reused
        // because their key string no longer exists. We keep the node's
        // `next` pointer intact so hash chain integrity is preserved.
        //
        // This handles the case PUC-Rio solves with LUA_TDEADKEY: after
        // GC sweeps key strings of nil-valued entries, main_position()
        // on the stale GcRef returns an incorrect bucket (hash defaults
        // to 0). Without this check, Brent's Case A would use the wrong
        // chain origin, corrupting hash chains of unrelated entries.
        if self.nodes[mp as usize].value.is_nil()
            && let Val::Str(r) = self.nodes[mp as usize].key
            && strings.get(r).is_none()
        {
            self.nodes[mp as usize].key = key;
            return Ok(mp);
        }

        // Main position is occupied. Find a free slot.
        let Some(free) = self.get_free_pos() else {
            return Err(runtime_error("table overflow"));
        };

        // Check if the occupant at mp is in its own main position.
        let occupant_key = self.nodes[mp as usize].key;
        let other_mp = self.main_position(&occupant_key, strings);

        if other_mp == mp {
            // Case B: The occupant is in its home bucket.
            // Place the new key in the free slot, chained after mp.
            self.nodes[free as usize].next = self.nodes[mp as usize].next;
            self.nodes[mp as usize].next = Some(free);
            self.nodes[free as usize].key = key;
            Ok(free)
        } else {
            // Case A: The occupant is displaced (not in its home bucket).
            // Relocate it to the free slot and give mp to the new key.

            // Walk the chain from other_mp to find the predecessor of mp.
            let mut prev = other_mp;
            while self.nodes[prev as usize].next != Some(mp) {
                if let Some(next) = self.nodes[prev as usize].next {
                    prev = next;
                } else {
                    break;
                }
            }

            // Repoint the chain: predecessor -> free (instead of -> mp).
            self.nodes[prev as usize].next = Some(free);

            // Copy the displaced occupant to the free slot.
            let occ_key = self.nodes[mp as usize].key;
            let occ_val = self.nodes[mp as usize].value;
            let occ_next = self.nodes[mp as usize].next;
            self.nodes[free as usize].key = occ_key;
            self.nodes[free as usize].value = occ_val;
            self.nodes[free as usize].next = occ_next;

            // Clear mp and place the new key there.
            self.nodes[mp as usize] = Node::empty();
            self.nodes[mp as usize].key = key;
            Ok(mp)
        }
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

impl Trace for Table {
    /// Mark all reachable GC objects in this table.
    ///
    /// Full implementation in Phase 7 (GC collector). For now, this is
    /// a stub that satisfies the trait contract.
    fn trace(&self) {
        // Phase 7: trace metatable, all array values, all hash keys + values.
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::gc::Color;
    use crate::vm::gc::arena::Arena;
    use crate::vm::string::StringTable;

    // Helper: create a string arena + table and intern a string.
    fn intern_str(
        arena: &mut Arena<LuaString>,
        table: &mut StringTable,
        s: &[u8],
    ) -> GcRef<LuaString> {
        table.intern(s, arena, Color::White0)
    }

    // -- Hash helper tests --

    #[test]
    fn ceil_log2_values() {
        assert_eq!(ceil_log2(0), 0);
        assert_eq!(ceil_log2(1), 0);
        assert_eq!(ceil_log2(2), 1);
        assert_eq!(ceil_log2(3), 2);
        assert_eq!(ceil_log2(4), 2);
        assert_eq!(ceil_log2(5), 3);
        assert_eq!(ceil_log2(8), 3);
        assert_eq!(ceil_log2(9), 4);
        assert_eq!(ceil_log2(32), 5);
        assert_eq!(ceil_log2(33), 6);
    }

    #[test]
    fn number_to_int_key_integers() {
        assert_eq!(number_to_int_key(1.0), Some(1));
        assert_eq!(number_to_int_key(0.0), Some(0));
        assert_eq!(number_to_int_key(-1.0), Some(-1));
        assert_eq!(number_to_int_key(100.0), Some(100));
    }

    #[test]
    fn number_to_int_key_non_integers() {
        assert_eq!(number_to_int_key(1.5), None);
        assert_eq!(number_to_int_key(0.1), None);
        assert_eq!(number_to_int_key(f64::NAN), None);
        assert_eq!(number_to_int_key(f64::INFINITY), None);
    }

    #[test]
    fn number_to_int_key_negative_zero() {
        // -0.0 should convert to integer key 0.
        assert_eq!(number_to_int_key(-0.0), Some(0));
    }

    #[test]
    fn hash_number_negative_zero_equals_positive_zero() {
        // The +1.0 normalization makes -0.0 and +0.0 hash identically.
        assert_eq!(hash_number(-0.0), hash_number(0.0));
    }

    // -- Table construction --

    #[test]
    fn empty_table() {
        let t = Table::new();
        assert_eq!(t.array_len(), 0);
        assert_eq!(t.hash_size(), 0);
        assert!(t.metatable().is_none());
    }

    #[test]
    fn table_with_sizes() {
        let t = Table::with_sizes(10, 8);
        assert_eq!(t.array_len(), 10);
        assert_eq!(t.hash_size(), 8);
    }

    #[test]
    fn table_hash_rounds_up_to_power_of_2() {
        let t = Table::with_sizes(0, 5);
        assert_eq!(t.hash_size(), 8); // next power of 2 >= 5
    }

    #[test]
    fn table_default() {
        let t = Table::default();
        assert_eq!(t.array_len(), 0);
        assert_eq!(t.hash_size(), 0);
    }

    // -- Array get/set --

    #[test]
    fn get_set_array_integers() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(10, 0);

        // Set integer keys in array range.
        t.raw_set(Val::Num(1.0), Val::Num(100.0), &strings_arena)
            .ok();
        t.raw_set(Val::Num(5.0), Val::Num(500.0), &strings_arena)
            .ok();
        t.raw_set(Val::Num(10.0), Val::Num(1000.0), &strings_arena)
            .ok();

        // Get via integer path.
        assert_eq!(t.get_int(1), Val::Num(100.0));
        assert_eq!(t.get_int(5), Val::Num(500.0));
        assert_eq!(t.get_int(10), Val::Num(1000.0));

        // Unset keys return nil.
        assert_eq!(t.get_int(2), Val::Nil);
        assert_eq!(t.get_int(0), Val::Nil);
        assert_eq!(t.get_int(-1), Val::Nil);
    }

    #[test]
    fn integer_float_equivalence() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(10, 0);

        // Set via float 3.0, get via integer 3.
        t.raw_set(Val::Num(3.0), Val::Num(42.0), &strings_arena)
            .ok();
        assert_eq!(t.get_int(3), Val::Num(42.0));
        assert_eq!(t.get(Val::Num(3.0), &strings_arena), Val::Num(42.0));
    }

    #[test]
    fn overwrite_array_value() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(5, 0);

        t.raw_set(Val::Num(1.0), Val::Num(10.0), &strings_arena)
            .ok();
        assert_eq!(t.get_int(1), Val::Num(10.0));

        t.raw_set(Val::Num(1.0), Val::Num(20.0), &strings_arena)
            .ok();
        assert_eq!(t.get_int(1), Val::Num(20.0));
    }

    #[test]
    fn set_nil_removes_array_value() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(5, 0);

        t.raw_set(Val::Num(1.0), Val::Num(10.0), &strings_arena)
            .ok();
        t.raw_set(Val::Num(1.0), Val::Nil, &strings_arena).ok();
        assert_eq!(t.get_int(1), Val::Nil);
    }

    // -- Hash part get/set --

    #[test]
    fn get_set_string_keys() {
        let mut strings_arena = Arena::new();
        let mut str_table = StringTable::new();

        let key_a = intern_str(&mut strings_arena, &mut str_table, b"hello");
        let key_b = intern_str(&mut strings_arena, &mut str_table, b"world");

        let mut t = Table::with_sizes(0, 4);
        t.raw_set(Val::Str(key_a), Val::Num(1.0), &strings_arena)
            .ok();
        t.raw_set(Val::Str(key_b), Val::Num(2.0), &strings_arena)
            .ok();

        assert_eq!(t.get_str(key_a, &strings_arena), Val::Num(1.0));
        assert_eq!(t.get_str(key_b, &strings_arena), Val::Num(2.0));
    }

    #[test]
    fn get_set_boolean_keys() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(0, 4);

        t.raw_set(Val::Bool(true), Val::Num(1.0), &strings_arena)
            .ok();
        t.raw_set(Val::Bool(false), Val::Num(0.0), &strings_arena)
            .ok();

        assert_eq!(t.get(Val::Bool(true), &strings_arena), Val::Num(1.0));
        assert_eq!(t.get(Val::Bool(false), &strings_arena), Val::Num(0.0));
    }

    #[test]
    fn get_nonexistent_key_returns_nil() {
        let mut strings_arena = Arena::new();
        let mut str_table = StringTable::new();

        let key = intern_str(&mut strings_arena, &mut str_table, b"missing");
        let t = Table::with_sizes(5, 4);

        assert_eq!(t.get_int(1), Val::Nil);
        assert_eq!(t.get_str(key, &strings_arena), Val::Nil);
        assert_eq!(t.get(Val::Num(99.0), &strings_arena), Val::Nil);
    }

    #[test]
    fn get_from_empty_table() {
        let strings_arena = Arena::new();
        let t = Table::new();

        assert_eq!(t.get_int(1), Val::Nil);
        assert_eq!(t.get(Val::Num(1.0), &strings_arena), Val::Nil);
        assert_eq!(t.get(Val::Bool(true), &strings_arena), Val::Nil);
    }

    #[test]
    fn integer_out_of_array_goes_to_hash() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(5, 4);

        // Key 10 is beyond array (size 5), goes to hash.
        t.raw_set(Val::Num(10.0), Val::Num(100.0), &strings_arena)
            .ok();
        assert_eq!(t.get_int(10), Val::Num(100.0));
        assert_eq!(t.get(Val::Num(10.0), &strings_arena), Val::Num(100.0));
    }

    #[test]
    fn zero_key_goes_to_hash() {
        // Key 0 is not in the 1-based array part.
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(5, 4);

        t.raw_set(Val::Num(0.0), Val::Bool(true), &strings_arena)
            .ok();
        assert_eq!(t.get_int(0), Val::Bool(true));
    }

    #[test]
    fn negative_key_goes_to_hash() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(5, 4);

        t.raw_set(Val::Num(-1.0), Val::Bool(true), &strings_arena)
            .ok();
        assert_eq!(t.get_int(-1), Val::Bool(true));
    }

    #[test]
    fn float_key_not_integer_in_hash() {
        // 1.5 is not an integer, goes to hash even though array exists.
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(5, 4);

        t.raw_set(Val::Num(1.5), Val::Num(99.0), &strings_arena)
            .ok();
        assert_eq!(t.get(Val::Num(1.5), &strings_arena), Val::Num(99.0));
        // Should not affect array[0] (key 1).
        assert_eq!(t.get_int(1), Val::Nil);
    }

    // -- Key validation --

    #[test]
    fn nil_key_is_error() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(5, 4);

        let result = t.raw_set(Val::Nil, Val::Num(1.0), &strings_arena);
        let Err(err) = result else {
            unreachable!("nil key should produce an error");
        };
        assert_eq!(err.to_string(), "table index is nil");
    }

    #[test]
    fn nan_key_is_error() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(5, 4);

        let result = t.raw_set(Val::Num(f64::NAN), Val::Num(1.0), &strings_arena);
        let Err(err) = result else {
            unreachable!("NaN key should produce an error");
        };
        assert_eq!(err.to_string(), "table index is NaN");
    }

    #[test]
    fn nil_get_returns_nil() {
        // Getting with nil key is not an error -- just returns nil.
        let strings_arena = Arena::new();
        let t = Table::with_sizes(5, 4);

        assert_eq!(t.get(Val::Nil, &strings_arena), Val::Nil);
    }

    #[test]
    fn set_nil_to_nonexistent_is_noop() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(0, 4);

        // Setting nil to a key that doesn't exist should not create a slot.
        t.raw_set(Val::Num(42.0), Val::Nil, &strings_arena).ok();
        assert_eq!(t.get(Val::Num(42.0), &strings_arena), Val::Nil);
    }

    // -- Brent's collision resolution --

    #[test]
    fn hash_collision_resolved() {
        // Create a small hash (size 2) so collisions are guaranteed.
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(0, 2);

        // Insert two keys. With size-2 hash, collisions are likely.
        t.raw_set(Val::Num(1.0), Val::Num(10.0), &strings_arena)
            .ok();
        t.raw_set(Val::Num(2.0), Val::Num(20.0), &strings_arena)
            .ok();

        // Both should be retrievable.
        assert_eq!(t.get(Val::Num(1.0), &strings_arena), Val::Num(10.0));
        assert_eq!(t.get(Val::Num(2.0), &strings_arena), Val::Num(20.0));
    }

    #[test]
    fn multiple_keys_in_hash() {
        let mut strings_arena = Arena::new();
        let mut str_table = StringTable::new();

        let mut t = Table::with_sizes(0, 8);

        // Insert several different key types.
        let key_s = intern_str(&mut strings_arena, &mut str_table, b"name");
        t.raw_set(Val::Str(key_s), Val::Num(1.0), &strings_arena)
            .ok();
        t.raw_set(Val::Num(42.0), Val::Num(2.0), &strings_arena)
            .ok();
        t.raw_set(Val::Bool(true), Val::Num(3.0), &strings_arena)
            .ok();
        t.raw_set(Val::Bool(false), Val::Num(4.0), &strings_arena)
            .ok();

        assert_eq!(t.get_str(key_s, &strings_arena), Val::Num(1.0));
        assert_eq!(t.get(Val::Num(42.0), &strings_arena), Val::Num(2.0));
        assert_eq!(t.get(Val::Bool(true), &strings_arena), Val::Num(3.0));
        assert_eq!(t.get(Val::Bool(false), &strings_arena), Val::Num(4.0));
    }

    #[test]
    fn fill_hash_part_completely() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(0, 4);

        // Fill all 4 slots.
        for i in 0..4 {
            let key = Val::Num(f64::from(i));
            t.raw_set(key, Val::Num(f64::from(i * 10)), &strings_arena)
                .ok();
        }

        // All should be retrievable.
        for i in 0..4 {
            let key = Val::Num(f64::from(i));
            assert_eq!(t.get(key, &strings_arena), Val::Num(f64::from(i * 10)));
        }
    }

    #[test]
    fn overflow_triggers_rehash() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(0, 2);

        // Fill both slots.
        t.raw_set(Val::Num(1.0), Val::Num(10.0), &strings_arena)
            .ok();
        t.raw_set(Val::Num(2.0), Val::Num(20.0), &strings_arena)
            .ok();

        // Third insert triggers rehash (grows hash or moves to array).
        t.raw_set(Val::Num(3.0), Val::Num(30.0), &strings_arena)
            .ok();
        assert_eq!(t.get(Val::Num(1.0), &strings_arena), Val::Num(10.0));
        assert_eq!(t.get(Val::Num(2.0), &strings_arena), Val::Num(20.0));
        assert_eq!(t.get(Val::Num(3.0), &strings_arena), Val::Num(30.0));
    }

    #[test]
    fn set_on_empty_hash_triggers_rehash() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(5, 0);

        // Array-range key works fine.
        t.raw_set(Val::Num(1.0), Val::Num(10.0), &strings_arena)
            .ok();

        // String key on empty hash triggers auto-rehash.
        let mut str_arena = Arena::new();
        let mut str_tbl = StringTable::new();
        let key = intern_str(&mut str_arena, &mut str_tbl, b"x");
        t.raw_set(Val::Str(key), Val::Num(1.0), &str_arena).ok();
        assert_eq!(t.get_str(key, &str_arena), Val::Num(1.0));
        assert!(t.hash_size() > 0);
    }

    // -- Metatable --

    #[test]
    fn metatable_get_set() {
        let mut table_arena: Arena<Table> = Arena::new();
        let mt_ref = table_arena.alloc(Table::new(), Color::White0);

        let mut t = Table::new();
        assert!(t.metatable().is_none());

        t.set_metatable(Some(mt_ref));
        assert_eq!(t.metatable(), Some(mt_ref));

        t.set_metatable(None);
        assert!(t.metatable().is_none());
    }

    // -- Mixed array + hash --

    #[test]
    fn mixed_array_and_hash() {
        let mut strings_arena = Arena::new();
        let mut str_table = StringTable::new();

        let mut t = Table::with_sizes(3, 4);
        let key_name = intern_str(&mut strings_arena, &mut str_table, b"name");

        // Array keys.
        t.raw_set(Val::Num(1.0), Val::Num(100.0), &strings_arena)
            .ok();
        t.raw_set(Val::Num(2.0), Val::Num(200.0), &strings_arena)
            .ok();

        // Hash key (string).
        t.raw_set(Val::Str(key_name), Val::Num(999.0), &strings_arena)
            .ok();

        // Hash key (out-of-range integer).
        t.raw_set(Val::Num(10.0), Val::Num(1000.0), &strings_arena)
            .ok();

        assert_eq!(t.get_int(1), Val::Num(100.0));
        assert_eq!(t.get_int(2), Val::Num(200.0));
        assert_eq!(t.get_str(key_name, &strings_arena), Val::Num(999.0));
        assert_eq!(t.get_int(10), Val::Num(1000.0));
    }

    // -- Negative zero --

    #[test]
    fn negative_zero_and_positive_zero_same_key() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(0, 4);

        t.raw_set(Val::Num(0.0), Val::Num(42.0), &strings_arena)
            .ok();
        // -0.0 should find the same entry.
        assert_eq!(t.get(Val::Num(-0.0), &strings_arena), Val::Num(42.0));

        // Overwrite via -0.0 should affect the same slot.
        t.raw_set(Val::Num(-0.0), Val::Num(99.0), &strings_arena)
            .ok();
        assert_eq!(t.get(Val::Num(0.0), &strings_arena), Val::Num(99.0));
    }

    // -- Resize / rehash --

    #[test]
    fn rehash_empty_table_grows() {
        // Starting from a totally empty table, inserting should auto-grow.
        let strings_arena = Arena::new();
        let mut t = Table::new();

        t.raw_set(Val::Num(1.0), Val::Num(10.0), &strings_arena)
            .ok();
        assert_eq!(t.get(Val::Num(1.0), &strings_arena), Val::Num(10.0));
    }

    #[test]
    fn rehash_moves_integers_to_array() {
        // Start with all integers in hash, rehash should move them to array.
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(0, 4);

        for i in 1..=4 {
            t.raw_set(
                Val::Num(f64::from(i)),
                Val::Num(f64::from(i * 10)),
                &strings_arena,
            )
            .ok();
        }

        // All values stored in hash. Trigger rehash by filling it.
        // After rehash, integer keys should move to array part.
        t.raw_set(Val::Num(5.0), Val::Num(50.0), &strings_arena)
            .ok();

        // All values should be retrievable.
        for i in 1..=5 {
            assert_eq!(
                t.get(Val::Num(f64::from(i)), &strings_arena),
                Val::Num(f64::from(i * 10))
            );
        }
        // Array part should have grown (integers moved from hash).
        assert!(t.array_len() > 0);
    }

    #[test]
    fn rehash_mixed_keys_splits_correctly() {
        let mut strings_arena = Arena::new();
        let mut str_table = StringTable::new();
        let mut t = Table::new();

        // Insert some integer keys and some string keys.
        for i in 1..=8 {
            t.raw_set(
                Val::Num(f64::from(i)),
                Val::Num(f64::from(i)),
                &strings_arena,
            )
            .ok();
        }
        let key_a = intern_str(&mut strings_arena, &mut str_table, b"a");
        let key_b = intern_str(&mut strings_arena, &mut str_table, b"b");
        t.raw_set(Val::Str(key_a), Val::Bool(true), &strings_arena)
            .ok();
        t.raw_set(Val::Str(key_b), Val::Bool(false), &strings_arena)
            .ok();

        // Verify all values.
        for i in 1..=8 {
            assert_eq!(
                t.get(Val::Num(f64::from(i)), &strings_arena),
                Val::Num(f64::from(i))
            );
        }
        assert_eq!(t.get_str(key_a, &strings_arena), Val::Bool(true));
        assert_eq!(t.get_str(key_b, &strings_arena), Val::Bool(false));

        // Integer keys should have moved to array.
        assert!(t.array_len() >= 8);
    }

    #[test]
    fn rehash_sparse_keys_stay_in_hash() {
        // Sparse integer keys should NOT expand the array.
        let strings_arena = Arena::new();
        let mut t = Table::new();

        t.raw_set(Val::Num(1.0), Val::Num(10.0), &strings_arena)
            .ok();
        t.raw_set(Val::Num(1000.0), Val::Num(20.0), &strings_arena)
            .ok();

        // Key 1 can go to array; key 1000 should go to hash.
        assert_eq!(t.get(Val::Num(1.0), &strings_arena), Val::Num(10.0));
        assert_eq!(t.get(Val::Num(1000.0), &strings_arena), Val::Num(20.0));
        // Array should NOT be 1000 entries.
        assert!(t.array_len() < 100);
    }

    #[test]
    fn rehash_preserves_string_keys() {
        let mut strings_arena = Arena::new();
        let mut str_table = StringTable::new();
        let mut t = Table::with_sizes(0, 2);

        let key_x = intern_str(&mut strings_arena, &mut str_table, b"x");
        let key_y = intern_str(&mut strings_arena, &mut str_table, b"y");

        t.raw_set(Val::Str(key_x), Val::Num(1.0), &strings_arena)
            .ok();
        t.raw_set(Val::Str(key_y), Val::Num(2.0), &strings_arena)
            .ok();

        // Insert more to trigger rehash.
        let key_z = intern_str(&mut strings_arena, &mut str_table, b"z");
        t.raw_set(Val::Str(key_z), Val::Num(3.0), &strings_arena)
            .ok();

        assert_eq!(t.get_str(key_x, &strings_arena), Val::Num(1.0));
        assert_eq!(t.get_str(key_y, &strings_arena), Val::Num(2.0));
        assert_eq!(t.get_str(key_z, &strings_arena), Val::Num(3.0));
    }

    #[test]
    fn many_inserts_grow_correctly() {
        // Stress test: insert 100 entries into an empty table.
        let strings_arena = Arena::new();
        let mut t = Table::new();

        for i in 1..=100 {
            t.raw_set(
                Val::Num(f64::from(i)),
                Val::Num(f64::from(i * 3)),
                &strings_arena,
            )
            .ok();
        }

        for i in 1..=100 {
            assert_eq!(
                t.get(Val::Num(f64::from(i)), &strings_arena),
                Val::Num(f64::from(i * 3))
            );
        }
    }

    #[test]
    fn compute_sizes_fifty_percent_threshold() {
        // Verify the >50% occupancy rule.
        // keys 1,2,3 in size 4 -> 3/4 = 75% > 50%, so array = 4.
        let mut nums = [0u32; MAXBITS as usize + 1];
        nums[0] = 1; // key 1
        nums[1] = 1; // key 2
        nums[2] = 1; // key 3
        let mut na_size: u32 = 3;
        let na = Table::compute_sizes(&nums, &mut na_size);
        assert_eq!(na_size, 4); // array size 4
        assert_eq!(na, 3); // 3 keys fit

        // keys 1,1000000 in a large range -> only size 1 qualifies.
        let mut nums2 = [0u32; MAXBITS as usize + 1];
        nums2[0] = 1; // key 1
        nums2[ceil_log2(1_000_000) as usize] = 1; // key 1000000
        let mut na_size2: u32 = 2;
        let na2 = Table::compute_sizes(&nums2, &mut na_size2);
        assert!(na_size2 <= 2); // array stays small
        assert!(na2 <= 2);
    }

    // -- Length operator --

    #[test]
    fn len_empty_table() {
        let strings_arena = Arena::new();
        let t = Table::new();
        assert_eq!(t.len(&strings_arena), 0);
    }

    #[test]
    fn len_contiguous_array() {
        let strings_arena = Arena::new();
        let mut t = Table::new();
        for i in 1..=5 {
            t.raw_set(
                Val::Num(f64::from(i)),
                Val::Num(f64::from(i)),
                &strings_arena,
            )
            .ok();
        }
        assert_eq!(t.len(&strings_arena), 5);
    }

    #[test]
    fn len_array_with_nil_at_end() {
        let strings_arena = Arena::new();
        let mut t = Table::with_sizes(5, 0);
        t.raw_set(Val::Num(1.0), Val::Bool(true), &strings_arena)
            .ok();
        t.raw_set(Val::Num(2.0), Val::Bool(true), &strings_arena)
            .ok();
        t.raw_set(Val::Num(3.0), Val::Bool(true), &strings_arena)
            .ok();
        // Slots 4 and 5 are nil.
        assert_eq!(t.len(&strings_arena), 3);
    }

    #[test]
    fn len_single_element() {
        let strings_arena = Arena::new();
        let mut t = Table::new();
        t.raw_set(Val::Num(1.0), Val::Bool(true), &strings_arena)
            .ok();
        assert_eq!(t.len(&strings_arena), 1);
    }

    #[test]
    fn len_only_hash_keys() {
        // Table with only string keys: length should be 0.
        let mut strings_arena = Arena::new();
        let mut str_table = StringTable::new();
        let mut t = Table::new();
        let key = intern_str(&mut strings_arena, &mut str_table, b"hello");
        t.raw_set(Val::Str(key), Val::Bool(true), &strings_arena)
            .ok();
        assert_eq!(t.len(&strings_arena), 0);
    }

    #[test]
    fn len_pre_allocated_array() {
        // With pre-allocated slots, all nil → length 0.
        let strings_arena = Arena::new();
        let t = Table::with_sizes(10, 0);
        assert_eq!(t.len(&strings_arena), 0);
    }

    // -- next() traversal --

    #[test]
    fn next_empty_table() {
        let strings_arena = Arena::new();
        let t = Table::new();
        let result = t.next(Val::Nil, &strings_arena);
        let Ok(None) = result else {
            unreachable!("empty table should return None");
        };
    }

    #[test]
    fn next_array_traversal() {
        let strings_arena = Arena::new();
        let mut t = Table::new();
        t.raw_set(Val::Num(1.0), Val::Num(10.0), &strings_arena)
            .ok();
        t.raw_set(Val::Num(2.0), Val::Num(20.0), &strings_arena)
            .ok();
        t.raw_set(Val::Num(3.0), Val::Num(30.0), &strings_arena)
            .ok();

        // Start iteration.
        let Ok(Some((k1, v1))) = t.next(Val::Nil, &strings_arena) else {
            unreachable!("should have first entry");
        };
        assert_eq!(k1, Val::Num(1.0));
        assert_eq!(v1, Val::Num(10.0));

        let Ok(Some((k2, v2))) = t.next(k1, &strings_arena) else {
            unreachable!("should have second entry");
        };
        assert_eq!(k2, Val::Num(2.0));
        assert_eq!(v2, Val::Num(20.0));

        let Ok(Some((k3, v3))) = t.next(k2, &strings_arena) else {
            unreachable!("should have third entry");
        };
        assert_eq!(k3, Val::Num(3.0));
        assert_eq!(v3, Val::Num(30.0));

        // No more entries.
        let Ok(None) = t.next(k3, &strings_arena) else {
            unreachable!("should be end of iteration");
        };
    }

    #[test]
    fn next_collects_all_entries() {
        // Verify next() visits all entries in a mixed table.
        let mut strings_arena = Arena::new();
        let mut str_table = StringTable::new();
        let mut t = Table::new();

        t.raw_set(Val::Num(1.0), Val::Num(100.0), &strings_arena)
            .ok();
        t.raw_set(Val::Num(2.0), Val::Num(200.0), &strings_arena)
            .ok();
        let key_a = intern_str(&mut strings_arena, &mut str_table, b"a");
        t.raw_set(Val::Str(key_a), Val::Bool(true), &strings_arena)
            .ok();

        // Collect all entries via next().
        let mut entries = Vec::new();
        let mut key = Val::Nil;
        loop {
            let Ok(result) = t.next(key, &strings_arena) else {
                unreachable!("next should not error");
            };
            let Some((k, v)) = result else {
                break;
            };
            entries.push((k, v));
            key = k;
        }

        // Should have exactly 3 entries.
        assert_eq!(entries.len(), 3);

        // Array entries come first.
        assert_eq!(entries[0], (Val::Num(1.0), Val::Num(100.0)));
        assert_eq!(entries[1], (Val::Num(2.0), Val::Num(200.0)));
        // String entry last (in hash).
        assert_eq!(entries[2], (Val::Str(key_a), Val::Bool(true)));
    }

    #[test]
    fn next_invalid_key_errors() {
        let strings_arena = Arena::new();
        let mut t = Table::new();
        t.raw_set(Val::Num(1.0), Val::Num(10.0), &strings_arena)
            .ok();

        // Key 999.0 is not in the table.
        let result = t.next(Val::Num(999.0), &strings_arena);
        assert!(result.is_err());
    }

    // -- Trace --

    #[test]
    fn table_trace_is_stub() {
        let t = Table::new();
        t.trace(); // should not panic
        assert!(t.needs_trace()); // tables DO need tracing (have GC refs)
    }

    /// Regression test: inserting a new key into a table with many
    /// nil-valued entries must not lose existing live entries after rehash.
    #[test]
    fn rehash_preserves_live_entries_after_nil_clearing() {
        let mut arena: Arena<LuaString> = Arena::new();
        let mut strtable = StringTable::new();
        let mut table = Table::new();

        let mut keys = Vec::new();
        for i in 0..60 {
            let name = format!("global_{i}");
            let r = intern_str(&mut arena, &mut strtable, name.as_bytes());
            table.raw_set(Val::Str(r), Val::Bool(true), &arena).ok();
            keys.push((name, r));
        }

        let assert_ref = intern_str(&mut arena, &mut strtable, b"assert");
        table
            .raw_set(Val::Str(assert_ref), Val::Bool(true), &arena)
            .ok();

        for (_, r) in &keys {
            table.raw_set(Val::Str(*r), Val::Nil, &arena).ok();
        }

        let new_key = intern_str(&mut arena, &mut strtable, b"t");
        table
            .raw_set(Val::Str(new_key), Val::Bool(true), &arena)
            .ok();

        let val = table.get_str(assert_ref, &arena);
        assert!(!val.is_nil(), "assert lost after rehash");
    }

    /// Regression test: after GC sweeps key strings of nil-valued entries,
    /// inserting a new key must not corrupt hash chains. This reproduces
    /// the bug where `new_key` calls `main_position` on a stale GcRef
    /// (swept string key), gets hash 0, and Brent's Case A chain
    /// relocation corrupts unrelated entries.
    #[test]
    fn new_key_reuses_dead_occupant_slot() {
        let mut arena: Arena<LuaString> = Arena::new();
        let mut strtable = StringTable::new();
        let mut table = Table::new();

        let assert_ref = intern_str(&mut arena, &mut strtable, b"assert");
        table
            .raw_set(Val::Str(assert_ref), Val::Bool(true), &arena)
            .ok();

        let mut live_refs = Vec::new();
        for i in 0..10 {
            let name = format!("live_{i}");
            let r = intern_str(&mut arena, &mut strtable, name.as_bytes());
            table
                .raw_set(Val::Str(r), Val::Num(f64::from(i)), &arena)
                .ok();
            live_refs.push((name, r));
        }

        let mut dead_refs = Vec::new();
        for i in 0..50 {
            let name = format!("dead_{i}");
            let r = intern_str(&mut arena, &mut strtable, name.as_bytes());
            table.raw_set(Val::Str(r), Val::Bool(true), &arena).ok();
            dead_refs.push(r);
        }

        // Set dead entries to nil, then sweep their strings.
        for &r in &dead_refs {
            table.raw_set(Val::Str(r), Val::Nil, &arena).ok();
        }
        for &r in &dead_refs {
            arena.free(r);
        }
        strtable.sweep_dead(&arena);

        // Insert new keys - exercises new_key with stale occupants.
        for i in 0..5 {
            let name = format!("new_{i}");
            let r = intern_str(&mut arena, &mut strtable, name.as_bytes());
            table.raw_set(Val::Str(r), Val::Bool(true), &arena).ok();
        }

        // Verify all live entries are still accessible.
        let val = table.get_str(assert_ref, &arena);
        assert!(!val.is_nil(), "assert lost after new_key with stale keys");
        for (name, r) in &live_refs {
            let val = table.get_str(*r, &arena);
            assert!(!val.is_nil(), "live entry '{name}' lost");
        }
    }
}
