//! Values derived from the open project, kept between frames while the
//! project stays exactly one undo snapshot (`History::state_serial`).
//!
//! Views and panes that draw every frame compute totals, spreads and row
//! lists from the whole project; on a 3,000-activity project that is
//! milliseconds a frame. Each kind of value has a name and keeps one entry
//! per argument key. When the snapshot changes (an edit, undo, redo, another
//! project) everything is dropped; while an edit is not yet recorded
//! (`serial` is `None`) nothing is kept.
use std::any::Any;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

/// Most entries one kind keeps; past this it starts afresh, so scrolling
/// through many rows or periods cannot grow it without bound.
const LIMIT: usize = 4096;

#[derive(Default)]
pub struct Memo {
    serial: Option<u64>,
    kinds: HashMap<&'static str, Box<dyn Any>>,
    /// Values handed out without computing them again.
    #[cfg(test)]
    pub hits: usize,
}

impl Memo {
    fn table<K: Hash + Eq + 'static, T: 'static>(
        &mut self,
        serial: u64,
        name: &'static str,
    ) -> &mut HashMap<K, Rc<T>> {
        if self.serial != Some(serial) {
            self.kinds.clear();
            self.serial = Some(serial);
        }
        let entry = self
            .kinds
            .entry(name)
            .or_insert_with(|| Box::new(HashMap::<K, Rc<T>>::new()));
        if !entry.is::<HashMap<K, Rc<T>>>() {
            // A name reused with other types: start that kind afresh.
            *entry = Box::new(HashMap::<K, Rc<T>>::new());
        }
        entry
            .downcast_mut::<HashMap<K, Rc<T>>>()
            .expect("the kind's table type was just checked")
    }

    /// The kept value of kind `name` for `key` in snapshot `serial`.
    pub fn get<K: Hash + Eq + 'static, T: 'static>(
        &mut self,
        serial: Option<u64>,
        name: &'static str,
        key: &K,
    ) -> Option<Rc<T>> {
        let serial = serial?;
        let v = self.table::<K, T>(serial, name).get(key).cloned();
        #[cfg(test)]
        if v.is_some() {
            self.hits += 1;
        }
        v
    }

    /// Keep `value` as kind `name` for `key` in snapshot `serial`.
    pub fn put<K: Hash + Eq + 'static, T: 'static>(
        &mut self,
        serial: Option<u64>,
        name: &'static str,
        key: K,
        value: Rc<T>,
    ) {
        let Some(serial) = serial else {
            return;
        };
        let table = self.table::<K, T>(serial, name);
        if table.len() >= LIMIT {
            table.clear();
        }
        table.insert(key, value);
    }
}

impl crate::App {
    /// The undo snapshot serial of the active project when it is exactly
    /// that snapshot; `None` while an edit is not recorded yet.
    pub(crate) fn project_serial(&self) -> Option<u64> {
        if self.active_rows().is_none() {
            if let Some(pinned) = self.pinned_serial() {
                return pinned;
            }
        }
        self.history
            .state_serial(&self.project, self.records_revision)
    }

    /// `compute()` for kind `name` and `key`, kept while the active project
    /// stays snapshot `serial` (from `project_serial`, taken once per frame
    /// by the caller). `compute` must depend only on the project and `key`.
    pub(crate) fn memo<K: Hash + Eq + 'static, T: 'static>(
        &self,
        serial: Option<u64>,
        name: &'static str,
        key: K,
        compute: impl FnOnce() -> T,
    ) -> Rc<T> {
        if let Some(v) = self.views.memo.borrow_mut().get(serial, name, &key) {
            return v;
        }
        // Computed with the memo released, so `compute` may use it too.
        let v = Rc::new(compute());
        self.views
            .memo
            .borrow_mut()
            .put(serial, name, key, v.clone());
        v
    }

    /// Forget every value kept between frames (memo, rows and lookups of
    /// both tables, the network layout, the workspace listings), so the next
    /// frame computes everything afresh.
    #[cfg(test)]
    pub(crate) fn drop_kept_values(&self) {
        let hits = self.views.memo.borrow().hits;
        *self.views.memo.borrow_mut() = Memo {
            hits,
            ..Default::default()
        };
        self.grid.drop_rows_cache();
        self.drop_frame_cache();
        self.panes.drop_kept_values();
        self.store.forget_listings();
    }
}
