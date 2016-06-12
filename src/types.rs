use std::default::Default;
use std::collections::HashMap;
use std::cmp::Ordering;

pub enum ValueType {
    TypeDeletion = 0,
    TypeValue = 1,
}

/// Represents a sequence number of a single entry.
pub type SequenceNumber = u64;

#[allow(dead_code)]
pub enum Status {
    OK,
    NotFound(String),
    Corruption(String),
    NotSupported(String),
    InvalidArgument(String),
    IOError(String),
}

/// Trait used to influence how SkipMap determines the order of elements. Use StandardComparator
/// for the normal implementation using numerical comparison.
pub trait Comparator {
    fn cmp(&[u8], &[u8]) -> Ordering;
}

pub struct StandardComparator;

impl Comparator for StandardComparator {
    fn cmp(a: &[u8], b: &[u8]) -> Ordering {
        a.cmp(b)
    }
}

/// [not all member types implemented yet]
///
pub struct Options<C: Comparator> {
    pub cmp: C,
    pub create_if_missing: bool,
    pub error_if_exists: bool,
    pub paranoid_checks: bool,
    // pub logger: Logger,
    pub write_buffer_size: usize,
    pub max_open_files: usize,
    // pub block_cache: Cache,
    pub block_size: usize,
    pub block_restart_interval: usize,
    // pub compression_type: CompressionType,
    pub reuse_logs: bool, // pub filter_policy: FilterPolicy,
}

impl Default for Options<StandardComparator> {
    fn default() -> Options<StandardComparator> {
        Options {
            cmp: StandardComparator,
            create_if_missing: true,
            error_if_exists: false,
            paranoid_checks: false,
            write_buffer_size: 4 << 20,
            max_open_files: 1 << 10,
            block_size: 4 << 10,
            block_restart_interval: 16,
            reuse_logs: false,
        }
    }
}

/// An extension of the standard `Iterator` trait that supports some methods necessary for LevelDB.
/// This works because the iterators used are stateful and keep the last returned element.
pub trait LdbIterator<'a>: Iterator {
    // We're emulating LevelDB's Slice type here using actual slices with the lifetime of the
    // iterator. The lifetime of the iterator is usually the one of the backing storage (Block,
    // MemTable, SkipMap...)
    // type Item = (&'a [u8], &'a [u8]);
    fn seek(&mut self, key: &[u8]);
    fn valid(&self) -> bool;
    fn current(&'a self) -> Self::Item;
}

/// Supplied to DB read operations.
pub struct ReadOptions {
    pub verify_checksums: bool,
    pub fill_cache: bool,
    pub snapshot: Option<SequenceNumber>,
}

impl Default for ReadOptions {
    fn default() -> Self {
        ReadOptions {
            verify_checksums: false,
            fill_cache: true,
            snapshot: None,
        }
    }
}

// Opaque snapshot handle; Represents index to SnapshotList.map
pub type Snapshot = u64;

/// A list of all snapshots is kept in the DB.
pub struct SnapshotList {
    map: HashMap<Snapshot, SequenceNumber>,
    newest: Snapshot,
    oldest: Snapshot,
}

impl SnapshotList {
    pub fn new() -> SnapshotList {
        SnapshotList {
            map: HashMap::new(),
            newest: 0,
            oldest: 0,
        }
    }

    pub fn new_snapshot(&mut self, seq: SequenceNumber) -> Snapshot {
        self.newest += 1;
        self.map.insert(self.newest, seq);

        if self.oldest == 0 {
            self.oldest = self.newest;
        }

        self.newest
    }

    pub fn oldest(&self) -> SequenceNumber {
        self.map.get(&self.oldest).unwrap().clone()
    }

    pub fn newest(&self) -> SequenceNumber {
        self.map.get(&self.newest).unwrap().clone()
    }

    pub fn delete(&mut self, ss: Snapshot) {
        if self.oldest == ss {
            self.oldest += 1;
        }
        if self.newest == ss {
            self.newest -= 1;
        }
        self.map.remove(&ss);
    }

    pub fn empty(&self) -> bool {
        self.oldest == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_list() {
        let mut l = SnapshotList::new();

        assert!(l.empty());

        let oldest = l.new_snapshot(1);
        l.new_snapshot(2);
        let newest = l.new_snapshot(0);

        assert!(!l.empty());

        assert_eq!(l.oldest(), 1);
        assert_eq!(l.newest(), 0);

        l.delete(newest);

        assert_eq!(l.newest(), 2);
        assert_eq!(l.oldest(), 1);

        l.delete(oldest);

        assert_eq!(l.oldest(), 2);
    }

}