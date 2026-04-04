use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::Path;

use crate::types::FileEntry;

/// Bytes to read for prefix hashing. 8 KiB is enough to distinguish
/// the vast majority of non-identical files that happen to share a size.
const PREFIX_BYTES: usize = 8192;

/// Buffer size for full-file hashing (64 KiB).
const READ_BUF: usize = 65_536;

fn hash_prefix(path: &Path) -> Option<u64> {
    let mut file = File::open(path).ok()?;
    let mut buf = vec![0_u8; PREFIX_BYTES];
    let n = file.read(&mut buf).ok()?;
    buf.truncate(n);
    let mut hasher = DefaultHasher::new();
    buf.hash(&mut hasher);
    Some(hasher.finish())
}

fn hash_full(path: &Path) -> Option<u64> {
    let mut file = File::open(path).ok()?;
    let mut hasher = DefaultHasher::new();
    let mut buf = vec![0_u8; READ_BUF];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        let chunk = buf.get(..n)?;
        chunk.hash(&mut hasher);
        if n < READ_BUF {
            break;
        }
    }
    Some(hasher.finish())
}

/// Remove duplicate files from the entry list, keeping one copy of each.
///
/// Uses a three-tier approach for speed:
/// 1. Group by file size (O(1) per file, no I/O).
/// 2. For same-size groups, hash first 8 KiB.
/// 3. For same-prefix groups, hash the full file.
///
/// Files that cannot be read are kept (never marked as duplicates).
pub fn deduplicate(entries: Vec<FileEntry>) -> Vec<FileEntry> {
    let len = entries.len();

    // Tier 1: group indices by file size
    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        by_size.entry(entry.size.as_u64()).or_default().push(i);
    }

    let mut remove = vec![false; len];

    for size_group in by_size.values() {
        if size_group.len() < 2 {
            continue;
        }

        // Tier 2: hash prefix for files with the same size
        let mut by_prefix: HashMap<u64, Vec<usize>> = HashMap::new();
        for &idx in size_group {
            let Some(entry) = entries.get(idx) else {
                continue;
            };
            if let Some(h) = hash_prefix(&entry.path) {
                by_prefix.entry(h).or_default().push(idx);
            }
        }

        for prefix_group in by_prefix.values() {
            if prefix_group.len() < 2 {
                continue;
            }

            // Tier 3: hash full file for files with same size + same prefix
            let mut by_full: HashMap<u64, Vec<usize>> = HashMap::new();
            for &idx in prefix_group {
                let Some(entry) = entries.get(idx) else {
                    continue;
                };
                if let Some(h) = hash_full(&entry.path) {
                    by_full.entry(h).or_default().push(idx);
                }
            }

            for full_group in by_full.values() {
                // Keep the first, mark the rest as duplicates
                for &idx in full_group.iter().skip(1) {
                    if let Some(flag) = remove.get_mut(idx) {
                        *flag = true;
                    }
                }
            }
        }
    }

    entries
        .into_iter()
        .zip(remove)
        .filter(|(_, is_dup)| !is_dup)
        .map(|(entry, _)| entry)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ByteSize;
    use std::fs;
    use std::path::PathBuf;

    fn entry(path: PathBuf, size: u64) -> FileEntry {
        FileEntry {
            path,
            size: ByteSize(size),
            bitrate_kbps: None,
        }
    }

    #[test]
    fn no_duplicates_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mp3");
        let b = dir.path().join("b.mp3");
        fs::write(&a, vec![1_u8; 100]).unwrap();
        fs::write(&b, vec![2_u8; 100]).unwrap();

        let entries = vec![entry(a, 100), entry(b, 100)];
        let result = deduplicate(entries);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn identical_files_deduplicated() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mp3");
        let b = dir.path().join("b.mp3");
        let c = dir.path().join("c.mp3");
        let content = vec![42_u8; 500];
        fs::write(&a, &content).unwrap();
        fs::write(&b, &content).unwrap();
        fs::write(&c, &content).unwrap();

        let entries = vec![entry(a, 500), entry(b, 500), entry(c, 500)];
        let result = deduplicate(entries);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn different_sizes_never_compared() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mp3");
        let b = dir.path().join("b.mp3");
        fs::write(&a, vec![1_u8; 100]).unwrap();
        fs::write(&b, vec![1_u8; 200]).unwrap();

        let entries = vec![entry(a, 100), entry(b, 200)];
        let result = deduplicate(entries);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn same_prefix_different_tail() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mp3");
        let b = dir.path().join("b.mp3");
        // Files larger than PREFIX_BYTES with same prefix but different tail
        let size = PREFIX_BYTES.saturating_add(1000);
        let mut content_a = vec![0_u8; size];
        let mut content_b = vec![0_u8; size];
        // Differ only in the tail
        if let Some(byte) = content_a.get_mut(size.saturating_sub(1)) {
            *byte = 1;
        }
        if let Some(byte) = content_b.get_mut(size.saturating_sub(1)) {
            *byte = 2;
        }
        fs::write(&a, &content_a).unwrap();
        fs::write(&b, &content_b).unwrap();

        #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
        let file_size = size as u64;
        let entries = vec![entry(a, file_size), entry(b, file_size)];
        let result = deduplicate(entries);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn large_identical_files_deduplicated() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mp3");
        let b = dir.path().join("b.mp3");
        let size = PREFIX_BYTES.saturating_add(5000);
        let content = vec![7_u8; size];
        fs::write(&a, &content).unwrap();
        fs::write(&b, &content).unwrap();

        #[allow(clippy::cast_possible_truncation, clippy::as_conversions)]
        let file_size = size as u64;
        let entries = vec![entry(a, file_size), entry(b, file_size)];
        let result = deduplicate(entries);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn missing_file_kept() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mp3");
        let b = dir.path().join("missing.mp3");
        fs::write(&a, vec![1_u8; 100]).unwrap();
        // b does not exist on disk

        let entries = vec![entry(a, 100), entry(b, 100)];
        let result = deduplicate(entries);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn empty_input() {
        let result = deduplicate(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn single_entry() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mp3");
        fs::write(&a, vec![1_u8; 100]).unwrap();

        let entries = vec![entry(a.clone(), 100)];
        let result = deduplicate(entries);
        assert_eq!(result.len(), 1);
        assert_eq!(result.first().map(|e| &e.path), Some(&a));
    }

    #[test]
    fn mixed_duplicates_and_uniques() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.mp3");
        let b = dir.path().join("b.mp3");
        let c = dir.path().join("c.mp3");
        let d = dir.path().join("d.mp3");

        fs::write(&a, vec![1_u8; 100]).unwrap();
        fs::write(&b, vec![1_u8; 100]).unwrap(); // dup of a
        fs::write(&c, vec![2_u8; 100]).unwrap(); // unique (same size, different content)
        fs::write(&d, vec![3_u8; 200]).unwrap(); // unique (different size)

        let entries = vec![
            entry(a, 100),
            entry(b, 100),
            entry(c, 100),
            entry(d, 200),
        ];
        let result = deduplicate(entries);
        assert_eq!(result.len(), 3);
    }
}
