//! Shared browser-stream capacity, with independent per-principal reservations.
use parking_lot::Mutex;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub(super) struct Limits {
    total: Arc<Semaphore>,
    people: Mutex<BTreeMap<String, usize>>,
}
pub(super) struct Slot {
    _permit: OwnedSemaphorePermit,
    limits: Arc<Limits>,
    person: String,
}
impl Limits {
    pub(super) fn new(total: usize) -> Arc<Self> {
        Arc::new(Self {
            total: Arc::new(Semaphore::new(total)),
            people: Mutex::new(BTreeMap::new()),
        })
    }
    pub(super) fn acquire(self: &Arc<Self>, person: &str) -> Option<Slot> {
        let permit = self.total.clone().try_acquire_owned().ok()?;
        let mut people = self.people.lock();
        let count = people.entry(person.to_owned()).or_default();
        if *count >= 4 {
            return None;
        }
        *count += 1;
        Some(Slot {
            _permit: permit,
            limits: self.clone(),
            person: person.to_owned(),
        })
    }
}
impl Drop for Slot {
    fn drop(&mut self) {
        let mut people = self.limits.people.lock();
        if let Some(count) = people.get_mut(&self.person) {
            *count -= 1;
            if *count == 0 {
                people.remove(&self.person);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slow_consumers_and_many_tabs_have_bounded_capacity() {
        let limits = Arc::new(Limits {
            total: Arc::new(Semaphore::new(5)),
            people: Mutex::new(BTreeMap::new()),
        });
        let tabs: Vec<_> = (0..4).map(|_| limits.acquire("Alice").unwrap()).collect();
        assert!(limits.acquire("Alice").is_none());
        let bob = limits.acquire("Bob").unwrap();
        assert!(limits.acquire("Eve").is_none());
        drop(tabs);
        drop(bob);
        assert!(limits.people.lock().is_empty());
        assert_eq!(limits.total.available_permits(), 5);
    }
}
