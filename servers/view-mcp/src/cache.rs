use std::{collections::HashMap, hash::Hash, sync::Arc};

#[derive(Debug)]
struct Entry<V> {
    value: Arc<V>,
    bytes: u64,
    last_used: u64,
    pins: u32,
}

#[derive(Debug)]
pub struct WeightedLru<K, V> {
    entries: HashMap<K, Entry<V>>,
    capacity_bytes: u64,
    resident_bytes: u64,
    clock: u64,
}

impl<K, V> WeightedLru<K, V>
where
    K: Clone + Eq + Hash,
{
    pub fn new(capacity_bytes: u64) -> Self {
        Self {
            entries: HashMap::new(),
            capacity_bytes,
            resident_bytes: 0,
            clock: 0,
        }
    }

    pub fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    pub fn get(&mut self, key: &K) -> Option<Arc<V>> {
        self.clock = self.clock.wrapping_add(1);
        let entry = self.entries.get_mut(key)?;
        entry.last_used = self.clock;
        Some(entry.value.clone())
    }

    pub fn insert(&mut self, key: K, value: Arc<V>, bytes: u64) -> Vec<(K, Arc<V>)> {
        self.clock = self.clock.wrapping_add(1);
        if let Some(old) = self.entries.remove(&key) {
            self.resident_bytes = self.resident_bytes.saturating_sub(old.bytes);
        }
        self.resident_bytes = self.resident_bytes.saturating_add(bytes);
        self.entries.insert(
            key.clone(),
            Entry {
                value,
                bytes,
                last_used: self.clock,
                // Retain the entry being inserted even when one item is larger than
                // the configured budget. Its caller already bounded the item, and
                // dropping it here would turn a successful load into an immediate
                // cache miss loop.
                pins: 1,
            },
        );
        let evicted = self.evict_to_budget();
        self.entries.get_mut(&key).expect("inserted entry").pins = 0;
        evicted
    }

    pub fn pin(&mut self, key: &K) -> bool {
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        entry.pins = entry.pins.saturating_add(1);
        true
    }

    pub fn unpin(&mut self, key: &K) -> bool {
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        entry.pins = entry.pins.saturating_sub(1);
        true
    }

    pub fn remove(&mut self, key: &K) -> Option<Arc<V>> {
        let entry = self.entries.remove(key)?;
        self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
        Some(entry.value)
    }

    pub fn evict_to_budget(&mut self) -> Vec<(K, Arc<V>)> {
        let mut evicted = Vec::new();
        while self.resident_bytes > self.capacity_bytes {
            let victim = self
                .entries
                .iter()
                .filter(|(_, entry)| entry.pins == 0)
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone());
            let Some(victim) = victim else {
                break;
            };
            if let Some(entry) = self.entries.remove(&victim) {
                self.resident_bytes = self.resident_bytes.saturating_sub(entry.bytes);
                evicted.push((victim, entry.value));
            }
        }
        evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_oldest_unpinned_entry_by_weight() {
        let mut cache = WeightedLru::new(10);
        cache.insert("a", Arc::new(1), 6);
        cache.insert("b", Arc::new(2), 4);
        cache.pin(&"a");
        let evicted = cache.insert("c", Arc::new(3), 5);
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0].0, "b");
        assert!(cache.contains_key(&"a"));
        assert!(cache.contains_key(&"c"));
        assert_eq!(cache.resident_bytes(), 11);
        cache.unpin(&"a");
        cache.evict_to_budget();
        assert!(cache.resident_bytes() <= 10);
    }

    #[test]
    fn access_refreshes_recency() {
        let mut cache = WeightedLru::new(2);
        cache.insert("a", Arc::new(1), 1);
        cache.insert("b", Arc::new(2), 1);
        cache.get(&"a");
        let evicted = cache.insert("c", Arc::new(3), 1);
        assert_eq!(evicted[0].0, "b");
    }

    #[test]
    fn retains_one_bounded_item_larger_than_the_budget() {
        let mut cache = WeightedLru::new(4);
        let evicted = cache.insert("large", Arc::new(1), 8);
        assert!(evicted.is_empty());
        assert!(cache.contains_key(&"large"));
        assert_eq!(cache.resident_bytes(), 8);
    }
}
