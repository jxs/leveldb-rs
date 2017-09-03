use key_types::{LookupKey, UserKey};
use cmp::MemtableKeyCmp;
use error::{Status, StatusCode, Result};
use key_types::{parse_memtable_key, build_memtable_key};
use types::{current_key_val, LdbIterator, SequenceNumber, ValueType};
use skipmap::{SkipMap, SkipMapIter};
use options::Options;

use std::rc::Rc;

use integer_encoding::FixedInt;

/// Provides Insert/Get/Iterate, based on the SkipMap implementation.
/// MemTable uses MemtableKeys internally, that is, it stores key and value in the [Skipmap] key.
pub struct MemTable {
    map: SkipMap,
    opt: Options,
}

impl MemTable {
    /// Returns a new MemTable.
    /// This wraps opt.cmp inside a MemtableKey-specific comparator.
    pub fn new(mut opt: Options) -> MemTable {
        opt.cmp = Rc::new(Box::new(MemtableKeyCmp(opt.cmp.clone())));
        MemTable::new_raw(opt)
    }

    /// Doesn't wrap the comparator in a MemtableKeyCmp.
    fn new_raw(opt: Options) -> MemTable {
        // Not using SkipMap::new_memtable_map(), as opt.cmp will already be wrapped by
        // MemTable::new()
        MemTable {
            map: SkipMap::new(opt.clone()),
            opt: opt,
        }
    }
    pub fn approx_mem_usage(&self) -> usize {
        self.map.approx_memory()
    }

    pub fn add<'a>(&mut self, seq: SequenceNumber, t: ValueType, key: UserKey<'a>, value: &[u8]) {
        self.map.insert(build_memtable_key(key, value, t, seq), Vec::new())
    }

    #[allow(unused_variables)]
    pub fn get(&self, key: &LookupKey) -> Result<Vec<u8>> {
        let mut iter = self.map.iter();
        iter.seek(key.memtable_key());

        if let Some((foundkey, _)) = current_key_val(&iter) {
            // let (lkeylen, lkeyoff, _, _, _) = parse_memtable_key(key.memtable_key());
            let (fkeylen, fkeyoff, tag, vallen, valoff) = parse_memtable_key(&foundkey);

            // Compare user key -- if equal, proceed
            // We only care about user key equality here
            if key.user_key() == &foundkey[fkeyoff..fkeyoff + fkeylen] {
                if tag & 0xff == ValueType::TypeValue as u64 {
                    return Ok(foundkey[valoff..valoff + vallen].to_vec());
                } else {
                    return Err(Status::new(StatusCode::NotFound, ""));
                }
            }
        }
        Err(Status::new(StatusCode::NotFound, ""))
    }

    pub fn iter(&self) -> MemtableIterator {
        MemtableIterator { skipmapiter: self.map.iter() }
    }
}

pub struct MemtableIterator {
    skipmapiter: SkipMapIter,
}

impl LdbIterator for MemtableIterator {
    fn advance(&mut self) -> bool {
        // Make sure this is actually needed.
        let (mut key, mut val) = (vec![], vec![]);
        loop {
            if !self.skipmapiter.advance() {
                return false;
            }
            if self.skipmapiter.current(&mut key, &mut val) {
                let (_, _, tag, _, _) = parse_memtable_key(&key);

                if tag & 0xff == ValueType::TypeValue as u64 {
                    return true;
                } else {
                    continue;
                }
            } else {
                return false;
            }
        }
    }
    fn reset(&mut self) {
        self.skipmapiter.reset();
    }
    fn prev(&mut self) -> bool {
        // Make sure this is actually needed (skipping deleted values?).
        let (mut key, mut val) = (vec![], vec![]);
        loop {
            if !self.skipmapiter.prev() {
                return false;
            }
            if self.skipmapiter.current(&mut key, &mut val) {
                let (_, _, tag, _, _) = parse_memtable_key(&key);

                if tag & 0xff == ValueType::TypeValue as u64 {
                    return true;
                } else {
                    continue;
                }
            } else {
                return false;
            }
        }
    }
    fn valid(&self) -> bool {
        self.skipmapiter.valid()
    }
    fn current(&self, key: &mut Vec<u8>, val: &mut Vec<u8>) -> bool {
        if !self.valid() {
            return false;
        }

        if self.skipmapiter.current(key, val) {
            let (keylen, keyoff, tag, vallen, valoff) = parse_memtable_key(&key);

            if tag & 0xff == ValueType::TypeValue as u64 {
                val.clear();
                val.extend_from_slice(&key[valoff..valoff + vallen]);
                // zero-allocation truncation.
                shift_left(key, keyoff);
                // Truncate key to key+tag.
                key.truncate(keylen + u64::required_space());
                return true;
            } else {
                panic!("should not happen");
            }
        } else {
            panic!("should not happen");
        }
    }
    fn seek(&mut self, to: &[u8]) {
        self.skipmapiter.seek(LookupKey::new(to, 0).memtable_key());
    }
}

/// shift_left moves s[mid..] to s[0..s.len()-mid]. The new size is s.len()-mid.
fn shift_left(s: &mut Vec<u8>, mid: usize) {
    for i in mid..s.len() {
        s.swap(i, i - mid);
    }
    let newlen = s.len() - mid;
    s.truncate(newlen);
}

#[cfg(test)]
#[allow(unused_variables)]
mod tests {
    use super::*;
    use key_types::*;
    use test_util::{test_iterator_properties, LdbIteratorIter};
    use types::*;
    use options::Options;

    #[test]
    fn test_shift_left() {
        let mut v = vec![1, 2, 3, 4, 5];
        shift_left(&mut v, 1);
        assert_eq!(v, vec![2, 3, 4, 5]);

        let mut v = vec![1, 2, 3, 4, 5];
        shift_left(&mut v, 4);
        assert_eq!(v, vec![5]);
    }

    fn get_memtable() -> MemTable {
        let mut mt = MemTable::new(Options::default());
        let entries = vec![(115, "abc", "122"),
                           (120, "abc", "123"),
                           (121, "abd", "124"),
                           (122, "abe", "125"),
                           (123, "abf", "126")];

        for e in entries.iter() {
            mt.add(e.0, ValueType::TypeValue, e.1.as_bytes(), e.2.as_bytes());
        }
        mt
    }

    #[test]
    fn test_memtable_parse_tag() {
        let tag = (12345 << 8) | 1;
        assert_eq!(parse_tag(tag), (ValueType::TypeValue, 12345));
    }

    #[test]
    fn test_memtable_add() {
        let mut mt = MemTable::new_raw(Options::default());
        mt.add(123,
               ValueType::TypeValue,
               "abc".as_bytes(),
               "123".as_bytes());

        assert_eq!(mt.map.iter().next().unwrap().0,
                   vec![11, 97, 98, 99, 1, 123, 0, 0, 0, 0, 0, 0, 3, 49, 50, 51].as_slice());
    }

    #[test]
    fn test_memtable_add_get() {
        let mt = get_memtable();

        // Smaller sequence number doesn't find entry
        if let Ok(v) = mt.get(&LookupKey::new("abc".as_bytes(), 110)) {
            println!("{:?}", v);
            panic!("found");
        }

        if let Ok(v) = mt.get(&LookupKey::new("abf".as_bytes(), 110)) {
            println!("{:?}", v);
            panic!("found");
        }

        // Bigger sequence number falls back to next smaller
        if let Ok(v) = mt.get(&LookupKey::new("abc".as_bytes(), 116)) {
            assert_eq!(v, "122".as_bytes());
        } else {
            panic!("not found");
        }

        // Exact match works
        if let Ok(v) = mt.get(&LookupKey::new("abc".as_bytes(), 120)) {
            assert_eq!(v, "123".as_bytes());
        } else {
            panic!("not found");
        }

        if let Ok(v) = mt.get(&LookupKey::new("abe".as_bytes(), 122)) {
            assert_eq!(v, "125".as_bytes());
        } else {
            panic!("not found");
        }

        if let Ok(v) = mt.get(&LookupKey::new("abf".as_bytes(), 129)) {
            assert_eq!(v, "126".as_bytes());
        } else {
            panic!("not found");
        }
    }

    #[test]
    fn test_memtable_iterator_init() {
        let mt = get_memtable();
        let mut iter = mt.iter();

        assert!(!iter.valid());
        iter.next();
        assert!(iter.valid());
        assert_eq!(current_key_val(&iter).unwrap().0,
                   vec![97, 98, 99, 1, 120, 0, 0, 0, 0, 0, 0].as_slice());
        iter.reset();
        assert!(!iter.valid());
    }

    #[test]
    fn test_memtable_iterator_fwd_seek() {
        let mt = get_memtable();
        let mut iter = mt.iter();

        let expected = vec!["123".as_bytes(), /* i.e., the abc entry with
                                               * higher sequence number comes first */
                            "122".as_bytes(),
                            "124".as_bytes(),
                            "125".as_bytes(),
                            "126".as_bytes()];
        let mut i = 0;

        for (k, v) in LdbIteratorIter::wrap(&mut iter) {
            assert_eq!(v, expected[i]);
            i += 1;
        }
    }

    #[test]
    fn test_memtable_iterator_reverse() {
        let mt = get_memtable();
        let mut iter = mt.iter();

        // Bigger sequence number comes first
        iter.next();
        assert!(iter.valid());
        assert_eq!(current_key_val(&iter).unwrap().0,
                   vec![97, 98, 99, 1, 120, 0, 0, 0, 0, 0, 0].as_slice());

        iter.next();
        assert!(iter.valid());
        assert_eq!(current_key_val(&iter).unwrap().0,
                   vec![97, 98, 99, 1, 115, 0, 0, 0, 0, 0, 0].as_slice());

        iter.next();
        assert!(iter.valid());
        assert_eq!(current_key_val(&iter).unwrap().0,
                   vec![97, 98, 100, 1, 121, 0, 0, 0, 0, 0, 0].as_slice());

        iter.prev();
        assert!(iter.valid());
        assert_eq!(current_key_val(&iter).unwrap().0,
                   vec![97, 98, 99, 1, 115, 0, 0, 0, 0, 0, 0].as_slice());

        iter.prev();
        assert!(iter.valid());
        assert_eq!(current_key_val(&iter).unwrap().0,
                   vec![97, 98, 99, 1, 120, 0, 0, 0, 0, 0, 0].as_slice());

        iter.prev();
        assert!(!iter.valid());
    }

    #[test]
    fn test_memtable_parse_key() {
        let key = vec![11, 1, 2, 3, 1, 123, 0, 0, 0, 0, 0, 0, 3, 4, 5, 6];
        let (keylen, keyoff, tag, vallen, valoff) = parse_memtable_key(&key);
        assert_eq!(keylen, 3);
        assert_eq!(&key[keyoff..keyoff + keylen], vec![1, 2, 3].as_slice());
        assert_eq!(tag, 123 << 8 | 1);
        assert_eq!(vallen, 3);
        assert_eq!(&key[valoff..valoff + vallen], vec![4, 5, 6].as_slice());
    }

    #[test]
    fn test_memtable_iterator_behavior() {
        let mut mt = MemTable::new(Options::default());
        let entries = vec![(115, "abc", "122"),
                           (120, "abc", "123"),
                           (121, "abd", "124"),
                           (123, "abf", "126")];

        for e in entries.iter() {
            mt.add(e.0, ValueType::TypeValue, e.1.as_bytes(), e.2.as_bytes());
        }

        test_iterator_properties(mt.iter());
    }
}
