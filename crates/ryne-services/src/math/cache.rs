//! Retain a bounded warm set; resources owned by displays remain reusable via Weak.
use super::{FontChoice, Formula};
use std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

type Key = (FontChoice, String, bool);
struct Entry {
    value: Option<Arc<Formula>>,
    bytes: usize,
    used: u64,
}
pub(super) struct Cache {
    entries: HashMap<Key, Entry>,
    live: HashMap<Key, Weak<Formula>>,
    bytes: usize,
    clock: u64,
    budget: usize,
    capacity: usize,
}
impl Default for Cache {
    fn default() -> Self {
        Self::new(16 * 1024 * 1024, 256)
    }
}
impl Cache {
    fn new(budget: usize, capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            live: HashMap::new(),
            bytes: 0,
            clock: 0,
            budget,
            capacity,
        }
    }
    pub(super) fn get(&mut self, key: &Key) -> Option<Option<Arc<Formula>>> {
        self.clock = self.clock.wrapping_add(1);
        if let Some(entry) = self.entries.get_mut(key) {
            entry.used = self.clock;
            return Some(entry.value.clone());
        }
        // The previous display stays alive while the next projection is built.
        // This avoids sequential-scan thrashing even beyond the retained capacity.
        self.live.get(key).and_then(Weak::upgrade).map(Some)
    }
    pub(super) fn insert(&mut self, key: Key, value: Option<Arc<Formula>>) {
        self.clock = self.clock.wrapping_add(1);
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.bytes;
        }
        let bytes = key.1.len() + value.as_ref().map_or(0, |f| f.svg.len() + f.path.len());
        // Bound weak-key metadata as well as retained SVGs. Dead displays do not
        // stay alive just because their content was once rendered.
        if self.live.len() >= 4096 {
            self.live.retain(|_, resource| resource.strong_count() > 0);
        }
        if self.live.len() < 4096
            && let Some(resource) = &value
        {
            self.live.insert(key.clone(), Arc::downgrade(resource));
        }
        if bytes > self.budget {
            return;
        }
        while self.bytes + bytes > self.budget || self.entries.len() >= self.capacity {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone());
            let Some(oldest) = oldest else {
                break;
            };
            if let Some(old) = self.entries.remove(&oldest) {
                self.bytes -= old.bytes;
            }
        }
        self.bytes += bytes;
        self.entries.insert(
            key,
            Entry {
                value,
                bytes,
                used: self.clock,
            },
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn resource(name: &str) -> Arc<Formula> {
        Arc::new(Formula {
            path: name.into(),
            svg: vec![0; 100],
            width: 1.,
            ascent: 1.,
            descent: 0.,
        })
    }
    fn key(s: &str) -> Key {
        (FontChoice::LeteSans, s.into(), false)
    }
    #[test]
    fn evicted_live_formulas_remain_reusable_but_are_not_kept_alive() {
        let mut cache = Cache::new(256, 2);
        let resources: Vec<_> = (0..20).map(|i| resource(&i.to_string())).collect();
        for resource in &resources {
            cache.insert(key(&resource.path), Some(resource.clone()));
        }
        assert!(cache.bytes <= 256);
        assert!(cache.entries.len() <= 2);
        for resource in &resources {
            assert!(Arc::ptr_eq(
                &cache.get(&key(&resource.path)).unwrap().unwrap(),
                resource
            ));
        }
        drop(resources);
        assert!(cache.get(&key("0")).is_none());
        assert!(cache.get(&key("19")).unwrap().is_some());
        cache.insert(key("invalid"), None);
        assert!(cache.get(&key("invalid")).unwrap().is_none());
        assert!(cache.bytes <= 256);
    }
}
