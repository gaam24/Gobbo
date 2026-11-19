//! A value derived from two shared inputs, kept while the inputs stay the same.

use std::sync::{Arc, Mutex, PoisonError, Weak};

/// The kept value and the inputs it was computed from.
type Entry<A, B, V> = (Weak<A>, Weak<B>, Arc<V>);

/// The value computed from two `Arc` inputs, kept while both are the very same allocations. The file caches hand out a
/// new `Arc` whenever the file behind it changed, so identity stands for content. The entry holds weak references: it
/// never keeps an old input alive, and while a weak reference lives its allocation is not reused, so a new input can
/// never be mistaken for an old one at the same address.
pub struct Memo<A, B, V> {
    entry: Mutex<Option<Entry<A, B, V>>>,
}

impl<A, B, V> Default for Memo<A, B, V> {
    fn default() -> Self {
        Self {
            entry: Mutex::new(None),
        }
    }
}

impl<A, B, V> Memo<A, B, V> {
    /// The kept value when `a` and `b` are the inputs it was computed from; otherwise `compute` makes it and it is kept.
    /// `compute` runs outside the lock: a caller that loses the race keeps the value the first caller stored.
    pub fn get(&self, a: &Arc<A>, b: &Arc<B>, compute: impl FnOnce() -> V) -> Arc<V> {
        if let Some(value) = self.kept(a, b) {
            return value;
        }
        let value = Arc::new(compute());
        let mut entry = self.entry.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((kept_a, kept_b, kept)) = entry.as_ref()
            && kept_a.as_ptr() == Arc::as_ptr(a)
            && kept_b.as_ptr() == Arc::as_ptr(b)
        {
            // Another caller computed the same inputs while this one was unlocked. Its value is the first one.
            return Arc::clone(kept);
        }
        *entry = Some((Arc::downgrade(a), Arc::downgrade(b), Arc::clone(&value)));
        value
    }

    /// Whether `a` and `b` are the inputs of the kept value, without computing or storing anything.
    pub fn matches(&self, a: &Arc<A>, b: &Arc<B>) -> bool {
        self.kept(a, b).is_some()
    }

    fn kept(&self, a: &Arc<A>, b: &Arc<B>) -> Option<Arc<V>> {
        let entry = self.entry.lock().unwrap_or_else(PoisonError::into_inner);
        let value = entry
            .as_ref()
            .filter(|(kept_a, kept_b, _)| {
                kept_a.as_ptr() == Arc::as_ptr(a) && kept_b.as_ptr() == Arc::as_ptr(b)
            })
            .map(|(_, _, value)| Arc::clone(value));
        drop(entry);
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn the_same_inputs_reuse_the_value_and_a_new_input_computes_it_again() {
        let memo = Memo::default();
        let a = Arc::new(1);
        let b = Arc::new("x");
        let first = memo.get(&a, &b, || 10);
        let second = memo.get(&a, &b, || panic!("the same inputs must not compute again"));
        assert!(Arc::ptr_eq(&first, &second));
        assert!(memo.matches(&a, &b));
        assert!(!memo.matches(&Arc::new(1), &b));

        // Equal content in a new allocation is a new input: the caches make one only when the file changed.
        let other = Arc::new(1);
        assert_eq!(*memo.get(&other, &b, || 20), 20);
    }

    #[test]
    fn a_race_keeps_the_first_value_for_both_callers() {
        let memo = Memo::default();
        let a = Arc::new(());
        let b = Arc::new(());
        let computed = AtomicUsize::new(0);
        let (first_entered, wait_first) = std::sync::mpsc::channel();
        let (second_stored, wait_second) = std::sync::mpsc::channel();

        {
            let memo = &memo;
            let computed = &computed;
            let a_first = Arc::clone(&a);
            let b_first = Arc::clone(&b);
            std::thread::scope(|scope| {
                let first = scope.spawn(move || {
                    memo.get(&a_first, &b_first, || {
                        computed.fetch_add(1, Ordering::SeqCst);
                        first_entered.send(()).unwrap();
                        // Hold this computation until the other caller has stored its value.
                        wait_second
                            .recv_timeout(std::time::Duration::from_secs(10))
                            .expect("the second caller must compute without waiting for this lock");
                        1
                    })
                });
                wait_first.recv().unwrap();
                let a_second = Arc::clone(&a);
                let b_second = Arc::clone(&b);
                let second = scope.spawn(move || {
                    let value = memo.get(&a_second, &b_second, || {
                        computed.fetch_add(1, Ordering::SeqCst);
                        2
                    });
                    second_stored.send(()).unwrap();
                    value
                });
                let second = second.join().unwrap();
                let first = first.join().unwrap();
                assert_eq!(*second, 2);
                assert!(
                    Arc::ptr_eq(&first, &second),
                    "the first stored value is kept"
                );
            });
        }
        assert_eq!(computed.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_dropped_input_does_not_match_its_successor() {
        let memo = Memo::default();
        let b = Arc::new(());
        let a = Arc::new(String::from("old"));
        memo.get(&a, &b, || 1);
        drop(a);
        let next = Arc::new(String::from("new"));
        assert_eq!(*memo.get(&next, &b, || 2), 2);
    }
}
