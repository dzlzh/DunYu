//! A single polling round. Failed metadata reads are cached too.
use crate::SummonRule;
use std::{collections::HashMap, fs, path::Path};

/// Keep filesystem lookups local to a scan so retargeted junctions are seen next time.
#[derive(Default)]
pub struct ProcessPaths {
    resolved: HashMap<String, Option<String>>,
}

impl ProcessPaths {
    fn resolve(&mut self, path: &str) -> Option<&str> {
        if !self.resolved.contains_key(path) {
            let resolved = fs::canonicalize(path)
                .ok()
                .and_then(|p| p.to_str().map(str::to_lowercase));
            self.resolved.insert(path.to_owned(), resolved);
        }
        self.resolved[path].as_deref()
    }

    pub fn equivalent(&mut self, expected: &str, actual: &str) -> bool {
        if expected.to_lowercase() == actual.to_lowercase() {
            return true;
        }
        Path::new(expected).is_absolute()
            && Path::new(actual).is_absolute()
            && self.resolve(expected).is_some()
            && self.resolve(actual).is_some()
            && self.resolved[expected] == self.resolved[actual]
    }

    pub fn matches(&mut self, rule: &SummonRule, path: &str, class: &str, title: &str) -> bool {
        (rule.class.is_empty() || rule.class == class)
            && title.contains(&rule.title)
            && self.equivalent(&rule.process_path, path)
    }
}

pub struct Scan<T> {
    entries: Vec<(usize, Option<Option<T>>)>,
}
impl<T> Scan<T> {
    pub fn new(handles: impl IntoIterator<Item = usize>) -> Self {
        Self {
            entries: handles.into_iter().map(|h| (h, None)).collect(),
        }
    }
    pub fn find(
        &mut self,
        foreground: usize,
        mut read: impl FnMut(usize) -> Option<T>,
        mut accept: impl FnMut(usize, &T) -> bool,
    ) -> Option<usize> {
        let first = self.entries.iter().position(|e| e.0 == foreground);
        for i in first
            .into_iter()
            .chain((0..self.entries.len()).filter(|i| Some(*i) != first))
        {
            let (h, cached) = &mut self.entries[i];
            let value = cached.get_or_insert_with(|| read(*h));
            if value.as_ref().is_some_and(|v| accept(*h, v)) {
                return Some(*h);
            }
        }
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_once_per_round_including_failures() {
        let mut scan = Scan::new([1, 2, 3]);
        let mut reads = 0;
        for _ in 0..5 {
            assert_eq!(
                scan.find(
                    2,
                    |h| {
                        reads += 1;
                        (h != 2).then_some(h)
                    },
                    |_, _| false
                ),
                None
            );
        }
        assert_eq!(reads, 3);
        let mut next = Scan::new([1, 2, 3, 4]);
        assert_eq!(next.find(4, Some, |_, _| true), Some(4));
    }
    #[test]
    fn foreground_order_and_fresh_acceptance() {
        let mut scan = Scan::new([1, 2, 3]);
        assert_eq!(scan.find(3, Some, |_, _| true), Some(3));
        assert_eq!(scan.find(2, Some, |_, _| true), Some(2));
        // A closed or changed candidate is rejected even with cached metadata.
        assert_eq!(scan.find(3, Some, |h, _| h != 3), Some(1));
        assert_eq!(scan.find(9, Some, |h, _| h == 2), Some(2));
        assert_eq!(Scan::<usize>::new([]).find(1, Some, |_, _| true), None);
    }
}
