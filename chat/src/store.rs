//! Where the sign-in is kept. Tokens are secrets and go to Windows Credential
//! Manager ([`WindowsCredentialStore`]); tests use [`MemoryStore`].
//!
//! Three items are stored ([`Item`]): the session (tokens and the account
//! they belong to, secret), the issued client ID and this computer's host
//! ID (neither is secret). An app that keeps preferences elsewhere can wrap
//! a store and send the two non-secret items there; the session must stay
//! in Credential Manager.

use crate::util::Secret;
use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

/// One stored item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Item {
    /// The token set and account (JSON). Secret.
    Session,
    /// The client ID OpenAI issued on the first sign-in (`oaiapp_...`).
    ClientId,
    /// This computer's stable host ID (`urn:uuid:...`), made before the
    /// first sign-in and never changed.
    HostId,
    /// The API key of one compatible-endpoint connection. Secret.
    ApiKey(KeyId),
}

/// Which connection an [`Item::ApiKey`] belongs to: the connection's ID
/// (1 to 32 letters, digits, `-` or `_`).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyId {
    bytes: [u8; 32],
    len: u8,
}

impl KeyId {
    /// `None` when `id` is empty, longer than 32 or has other characters.
    pub fn new(id: &str) -> Option<KeyId> {
        let ok = !id.is_empty()
            && id.len() <= 32
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !ok {
            return None;
        }
        let mut bytes = [0u8; 32];
        bytes[..id.len()].copy_from_slice(id.as_bytes());
        Some(KeyId {
            bytes,
            len: id.len() as u8,
        })
    }

    pub fn as_str(&self) -> &str {
        // Only ASCII is ever stored.
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

impl fmt::Debug for KeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "KeyId({})", self.as_str())
    }
}

impl Item {
    pub fn is_secret(self) -> bool {
        matches!(self, Item::Session | Item::ApiKey(_))
    }
    fn name(self) -> String {
        match self {
            Item::Session => "session".into(),
            Item::ClientId => "client-id".into(),
            Item::HostId => "host-id".into(),
            Item::ApiKey(id) => format!("api-key/{}", id.as_str()),
        }
    }
}

/// The store could not be read or written. Holds no stored content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreError(pub String);

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "credential storage failed: {}", self.0)
    }
}

/// Storage for the sign-in. Values are handled as [`Secret`] whether or not
/// the item is secret, so nothing is printed by accident.
pub trait CredentialStore: Send + Sync {
    fn read(&self, item: Item) -> Result<Option<Secret>, StoreError>;
    fn write(&self, item: Item, value: &Secret) -> Result<(), StoreError>;
    /// Remove the item; removing one that is not there is not an error.
    fn delete(&self, item: Item) -> Result<(), StoreError>;
}

/// An in-memory store (tests, or a session that must not outlive the app).
#[derive(Default)]
pub struct MemoryStore {
    items: Mutex<HashMap<Item, Secret>>,
    /// Count of writes, for tests.
    writes: Mutex<usize>,
}

impl fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<Item> = self.items.lock().unwrap().keys().copied().collect();
        write!(f, "MemoryStore({keys:?})")
    }
}

impl MemoryStore {
    pub fn new() -> MemoryStore {
        MemoryStore::default()
    }
    /// How many writes this store has taken.
    pub fn write_count(&self) -> usize {
        *self.writes.lock().unwrap()
    }
    /// Whether an item is present.
    pub fn has(&self, item: Item) -> bool {
        self.items.lock().unwrap().contains_key(&item)
    }
}

impl CredentialStore for MemoryStore {
    fn read(&self, item: Item) -> Result<Option<Secret>, StoreError> {
        Ok(self.items.lock().unwrap().get(&item).cloned())
    }
    fn write(&self, item: Item, value: &Secret) -> Result<(), StoreError> {
        *self.writes.lock().unwrap() += 1;
        self.items.lock().unwrap().insert(item, value.clone());
        Ok(())
    }
    fn delete(&self, item: Item) -> Result<(), StoreError> {
        self.items.lock().unwrap().remove(&item);
        Ok(())
    }
}

/// Generic credentials in Windows Credential Manager, for the signed-in
/// Windows user on this computer (not roaming).
///
/// Each item is kept twice over, in slots `a` and `b`: a head credential
/// `<prefix><item>.a` (`openxer-v1:<sequence>:<parts>:<sha-256 hex>`) and
/// its parts `<prefix><item>.a1`, `.a2`, ... (one value may need several,
/// as one credential holds 2560 bytes). A write fills the slot with the
/// older sequence, parts first and its head last, with the next sequence.
/// A reader takes the newest slot whose parts match its digest.
///
/// So a reader sees the old value or the new one, never a mix, and never
/// nothing: Windows replaces a credential by removing and adding it, and
/// under load a reader can see the head being replaced as missing for a
/// few milliseconds; that is always the older slot's head. A write that
/// stops half way leaves the newer slot, and the value, as they were.
/// Fixed names mean nothing piles up in Credential Manager.
#[derive(Clone, Debug)]
pub struct WindowsCredentialStore {
    prefix: String,
    /// The slot and sequence this value last wrote per item. Credential
    /// Manager may show a fresh write a moment late, so the next write
    /// does not trust only what it reads back.
    written: std::sync::Arc<Mutex<HashMap<Item, (&'static str, u64)>>>,
    /// Tests: fail the write after this many more credential writes.
    #[cfg(test)]
    fail_after: std::sync::Arc<Mutex<Option<usize>>>,
}

/// The prefix the app uses.
pub const DEFAULT_CREDENTIAL_PREFIX: &str = "OpenXER/chat/";

const HEAD_MARK: &str = "openxer-v1:";
/// The most parts one value may take (40 KB).
const MAX_PARTS: usize = 16;
const SLOTS: [&str; 2] = ["a", "b"];

impl Default for WindowsCredentialStore {
    fn default() -> Self {
        WindowsCredentialStore::new(DEFAULT_CREDENTIAL_PREFIX)
    }
}

/// A parsed head.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Head {
    slot: &'static str,
    sequence: u64,
    parts: usize,
    digest: String,
}

fn parse_head(slot: &'static str, s: &str) -> Option<Head> {
    let mut f = s.strip_prefix(HEAD_MARK)?.splitn(3, ':');
    let sequence = f.next()?.parse().ok()?;
    let parts: usize = f.next()?.parse().ok()?;
    let digest = f.next()?.to_string();
    (1..=MAX_PARTS).contains(&parts).then_some(Head {
        slot,
        sequence,
        parts,
        digest,
    })
}

fn err(code: u32) -> StoreError {
    StoreError(format!("Windows error {code}"))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

enum Read {
    Value(Option<Secret>),
    /// No slot could be read whole right now; read again.
    Torn,
}

impl WindowsCredentialStore {
    pub fn new(prefix: impl Into<String>) -> WindowsCredentialStore {
        WindowsCredentialStore {
            prefix: prefix.into(),
            written: Default::default(),
            #[cfg(test)]
            fail_after: Default::default(),
        }
    }

    fn head_name(&self, item: Item, slot: &str) -> String {
        format!("{}{}.{slot}", self.prefix, item.name())
    }

    fn part_name(&self, item: Item, slot: &str, i: usize) -> String {
        format!("{}{}.{slot}{i}", self.prefix, item.name())
    }

    /// Tests: make the credential write after `n` more succeed fail.
    #[cfg(test)]
    fn fail_after(&self, n: usize) {
        *self.fail_after.lock().unwrap() = Some(n);
    }

    fn put(&self, target: &str, blob: &[u8]) -> Result<(), StoreError> {
        #[cfg(test)]
        {
            let mut f = self.fail_after.lock().unwrap();
            if let Some(n) = f.as_mut() {
                if *n == 0 {
                    *f = None;
                    return Err(StoreError("simulated failure".into()));
                }
                *n -= 1;
            }
        }
        crate::win32::cred_write(target, blob).map_err(err)
    }

    /// The heads present now, newest first. `present` counts head
    /// credentials found, readable or not.
    fn heads(&self, item: Item) -> Result<(Vec<Head>, usize), StoreError> {
        let mut out = Vec::new();
        let mut present = 0;
        for slot in SLOTS {
            if let Some(raw) = crate::win32::cred_read(&self.head_name(item, slot)).map_err(err)? {
                present += 1;
                if let Some(h) = parse_head(slot, &String::from_utf8_lossy(&raw)) {
                    out.push(h);
                }
            }
        }
        out.sort_by_key(|h| std::cmp::Reverse(h.sequence));
        Ok((out, present))
    }

    /// The value of one slot, if its parts are all there and match.
    fn read_slot(&self, item: Item, h: &Head) -> Result<Option<Secret>, StoreError> {
        let mut all = Vec::new();
        for i in 1..=h.parts {
            match crate::win32::cred_read(&self.part_name(item, h.slot, i)).map_err(err)? {
                Some(part) => all.extend_from_slice(&part),
                None => return Ok(None),
            }
        }
        let ok = hex(&crate::win32::sha256(&all)) == h.digest;
        let value = Secret::new(String::from_utf8_lossy(&all).into_owned());
        for b in all.iter_mut() {
            *b = 0;
        }
        Ok(ok.then_some(value))
    }

    fn read_once(&self, item: Item) -> Result<Read, StoreError> {
        let (heads, present) = self.heads(item)?;
        for h in &heads {
            if let Some(v) = self.read_slot(item, h)? {
                return Ok(Read::Value(Some(v)));
            }
        }
        Ok(if present == 0 {
            Read::Value(None)
        } else {
            Read::Torn
        })
    }

    fn delete_parts(&self, item: Item, slot: &str, from: usize) -> Result<(), StoreError> {
        for i in from..=MAX_PARTS {
            crate::win32::cred_delete(&self.part_name(item, slot, i)).map_err(err)?;
        }
        Ok(())
    }
}

impl CredentialStore for WindowsCredentialStore {
    fn read(&self, item: Item) -> Result<Option<Secret>, StoreError> {
        // Torn: both slots were changing at once (a reader two writes
        // behind); read again for up to about three seconds.
        for attempt in 0..80u64 {
            match self.read_once(item)? {
                Read::Value(v) => return Ok(v),
                Read::Torn => {
                    std::thread::sleep(std::time::Duration::from_millis((2 + attempt * 2).min(50)))
                }
            }
        }
        Err(StoreError(
            "the stored value kept changing while it was read".into(),
        ))
    }

    /// Write, then read back until the value shows; write again (up to
    /// three times) if it does not. On a busy machine Credential Manager
    /// was seen to report a write done that it then did not keep.
    fn write(&self, item: Item, value: &Secret) -> Result<(), StoreError> {
        if value.expose().len().div_ceil(crate::win32::CRED_MAX_BLOB) > MAX_PARTS {
            return Err(StoreError("the value is too large to store".into()));
        }
        for _ in 0..3 {
            self.write_once(item, value)?;
            for _ in 0..30 {
                if let Read::Value(Some(v)) = self.read_once(item)? {
                    if v == *value {
                        return Ok(());
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            // Choose the slot again from what is visible.
            self.written.lock().unwrap().remove(&item);
        }
        Err(StoreError("Windows did not keep the stored value".into()))
    }

    /// Delete, then check every name is gone; delete again if not.
    fn delete(&self, item: Item) -> Result<(), StoreError> {
        for _ in 0..3 {
            self.delete_once(item)?;
            for _ in 0..30 {
                if self.all_gone(item)? {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
        Err(StoreError("Windows did not delete the stored value".into()))
    }
}

impl WindowsCredentialStore {
    fn all_gone(&self, item: Item) -> Result<bool, StoreError> {
        for slot in SLOTS {
            if crate::win32::cred_read(&self.head_name(item, slot))
                .map_err(err)?
                .is_some()
            {
                return Ok(false);
            }
            for i in 1..=MAX_PARTS {
                if crate::win32::cred_read(&self.part_name(item, slot, i))
                    .map_err(err)?
                    .is_some()
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    fn write_once(&self, item: Item, value: &Secret) -> Result<(), StoreError> {
        let bytes = value.expose().as_bytes();
        let parts: Vec<&[u8]> = if bytes.is_empty() {
            vec![&[][..]]
        } else {
            bytes.chunks(crate::win32::CRED_MAX_BLOB).collect()
        };
        let (heads, _) = self.heads(item)?;
        let mut newest = heads.first().map(|h| (h.slot, h.sequence));
        if let Some(&(slot, sequence)) = self.written.lock().unwrap().get(&item) {
            if newest.is_none_or(|(_, s)| s < sequence) {
                newest = Some((slot, sequence));
            }
        }
        // The slot that is not the newest one.
        let slot = match newest.map(|(s, _)| s) {
            Some("a") => "b",
            _ => "a",
        };
        let sequence = newest.map(|(_, s)| s).unwrap_or(0) + 1;
        for (i, part) in parts.iter().enumerate() {
            self.put(&self.part_name(item, slot, i + 1), part)?;
        }
        let head = format!(
            "{HEAD_MARK}{sequence}:{}:{}",
            parts.len(),
            hex(&crate::win32::sha256(bytes))
        );
        self.put(&self.head_name(item, slot), head.as_bytes())?;
        self.written.lock().unwrap().insert(item, (slot, sequence));
        // This slot's parts past the new end, left by a longer value.
        self.delete_parts(item, slot, parts.len() + 1)
    }

    fn delete_once(&self, item: Item) -> Result<(), StoreError> {
        // The remembered sequence is kept, so a later write still counts up.
        for slot in SLOTS {
            crate::win32::cred_delete(&self.head_name(item, slot)).map_err(err)?;
        }
        for slot in SLOTS {
            self.delete_parts(item, slot, 1)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_reads_writes_and_deletes() {
        let s = MemoryStore::new();
        assert_eq!(s.read(Item::Session).unwrap(), None);
        s.write(Item::Session, &Secret::new("secret-value"))
            .unwrap();
        assert_eq!(
            s.read(Item::Session).unwrap().unwrap().expose(),
            "secret-value"
        );
        assert!(!format!("{s:?}").contains("secret-value"));
        s.delete(Item::Session).unwrap();
        s.delete(Item::Session).unwrap();
        assert!(!s.has(Item::Session));
        assert!(Item::Session.is_secret());
        assert!(!Item::ClientId.is_secret());
    }

    /// Only one test at a time touches the real Credential Manager: many
    /// at once once left hundreds of entries behind and made Windows refuse
    /// writes.
    static REAL_STORE: Mutex<()> = Mutex::new(());

    /// A store in the real Credential Manager under a fixed test prefix
    /// (`OpenXER-test/<name>/`, so a run that was killed leaves nothing new
    /// behind: the next run reuses and clears the same names). It holds the
    /// serial lock, clears every name before the test and again when
    /// dropped, also when the test fails.
    struct TestStore {
        store: std::sync::Arc<WindowsCredentialStore>,
        _serial: std::sync::MutexGuard<'static, ()>,
    }

    impl TestStore {
        fn new(name: &str) -> TestStore {
            let serial = REAL_STORE.lock().unwrap_or_else(|p| p.into_inner());
            let store =
                std::sync::Arc::new(WindowsCredentialStore::new(format!("OpenXER-test/{name}/")));
            clear(&store);
            TestStore {
                store,
                _serial: serial,
            }
        }
    }

    /// Every name of every item, deleted directly (no read-back waits).
    fn clear(s: &WindowsCredentialStore) {
        *s.fail_after.lock().unwrap_or_else(|p| p.into_inner()) = None;
        for item in [Item::Session, Item::ClientId, Item::HostId] {
            for name in all_names(s, item) {
                let _ = crate::win32::cred_delete(&name);
            }
        }
    }

    impl Drop for TestStore {
        fn drop(&mut self) {
            clear(&self.store);
            for item in [Item::Session, Item::ClientId, Item::HostId] {
                let _ = self.store.delete(item);
            }
        }
    }

    /// Every credential name an item can use.
    fn all_names(s: &WindowsCredentialStore, item: Item) -> Vec<String> {
        let mut out = Vec::new();
        for slot in SLOTS {
            out.push(s.head_name(item, slot));
            for i in 1..=MAX_PARTS {
                out.push(s.part_name(item, slot, i));
            }
        }
        out
    }

    fn present(s: &WindowsCredentialStore, item: Item) -> Vec<String> {
        all_names(s, item)
            .into_iter()
            .filter(|n| crate::win32::cred_read(n).unwrap().is_some())
            .collect()
    }

    /// (slot, sequence, parts) of each head, newest first.
    fn heads(s: &WindowsCredentialStore) -> Vec<(&'static str, u64, usize)> {
        s.heads(Item::Session)
            .unwrap()
            .0
            .into_iter()
            .map(|h| (h.slot, h.sequence, h.parts))
            .collect()
    }

    /// Credential Manager can show a fresh write or delete a moment late
    /// when the machine is busy; wait up to two seconds for `f`.
    fn eventually(f: impl Fn() -> bool) -> bool {
        for _ in 0..200 {
            if f() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }

    fn reads(s: &WindowsCredentialStore, item: Item, want: &str) -> bool {
        eventually(|| s.read(item).unwrap().is_some_and(|v| v.expose() == want))
    }

    fn value(tag: char, len: usize) -> String {
        std::iter::repeat_n(tag, len).collect()
    }

    /// Writes throwaway values under a fixed test prefix in the real
    /// Credential Manager of the user running the tests, then removes them.
    #[test]
    fn windows_credential_store_round_trips_small_and_split_values() {
        let guard = TestStore::new("round-trip");
        let s = &*guard.store;
        assert_eq!(s.read(Item::HostId).unwrap(), None);
        s.write(Item::HostId, &Secret::new("urn:uuid:test"))
            .unwrap();
        assert!(reads(s, Item::HostId, "urn:uuid:test"));
        // Larger than one credential: split over parts, in slot a.
        let big = value('b', 7000);
        s.write(Item::Session, &Secret::new(big.clone())).unwrap();
        assert!(reads(s, Item::Session, &big));
        assert!(eventually(|| heads(s) == [("a", 1, 3)]), "{:?}", heads(s));
        // The next write fills slot b with the next sequence.
        s.write(Item::Session, &Secret::new(value('c', 3000)))
            .unwrap();
        assert!(
            eventually(|| heads(s) == [("b", 2, 2), ("a", 1, 3)]),
            "{:?}",
            heads(s)
        );
        assert!(reads(s, Item::Session, &value('c', 3000)));
        // Back in slot a, shorter: its third part is removed.
        s.write(Item::Session, &Secret::new("short")).unwrap();
        assert!(
            eventually(|| heads(s) == [("a", 3, 1), ("b", 2, 2)]),
            "{:?}",
            heads(s)
        );
        assert!(reads(s, Item::Session, "short"));

        // An empty value is a value, not a missing one.
        s.write(Item::Session, &Secret::new("")).unwrap();
        assert!(reads(s, Item::Session, ""));
        // Delete removes every head and part.
        s.delete(Item::Session).unwrap();
        s.delete(Item::HostId).unwrap();
        assert!(eventually(|| s.read(Item::Session).unwrap().is_none()));
        assert!(eventually(|| present(s, Item::Session).is_empty()));
        assert!(eventually(|| present(s, Item::HostId).is_empty()));
        // Too large to store.
        assert!(s
            .write(Item::Session, &Secret::new(value('z', 17 * 2560)))
            .is_err());
        assert!(eventually(|| present(s, Item::Session).is_empty()));
    }

    /// Readers running during repeated writes only ever see a whole old or
    /// whole new value, never nothing (real Credential Manager, fixed test
    /// prefix). Each reader has its own store value, like another process.
    ///
    /// Heavy (four readers and thirty writes at once): run by hand only,
    /// `cargo test -p openxer-chat -- --ignored readers_never_see`.
    #[test]
    #[ignore = "heavy use of the real Credential Manager; run by hand"]
    fn readers_never_see_a_torn_or_missing_value_during_writes() {
        let guard = TestStore::new("readers");
        let s = guard.store.clone();
        let a = value('a', 6000);
        let b = value('b', 3000);
        let c = "small".to_string();
        s.write(Item::Session, &Secret::new(a.clone())).unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let readers: Vec<_> = (0..4)
            .map(|_| {
                let (s, stop) = (WindowsCredentialStore::new(s.prefix.clone()), stop.clone());
                let ok = [a.clone(), b.clone(), c.clone()];
                std::thread::spawn(move || {
                    let mut reads = 0;
                    while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                        let got = s.read(Item::Session).unwrap().expect("never missing");
                        assert!(
                            ok.iter().any(|v| v == got.expose()),
                            "a torn value was read"
                        );
                        reads += 1;
                    }
                    reads
                })
            })
            .collect();
        for i in 0..30 {
            let v = [&a, &b, &c][i % 3];
            s.write(Item::Session, &Secret::new(v.clone())).unwrap();
        }
        stop.store(true, std::sync::atomic::Ordering::SeqCst);
        let total: usize = readers.into_iter().map(|r| r.join().unwrap()).sum();
        assert!(total > 0);
        // Under this much load Windows was seen to bring back an earlier
        // write a moment after a later one had been read back (one of the
        // reasons Auth keeps its own copy of the session); the value is
        // still always one whole written value.
        let last = s.read(Item::Session).unwrap().expect("a value");
        assert!([&a, &b, &c].iter().any(|v| **v == last.expose()));
    }

    /// A write that stops between steps leaves the old value readable.
    #[test]
    fn a_write_that_fails_half_way_keeps_the_old_value() {
        let guard = TestStore::new("half-way");
        let s = &*guard.store;
        s.write(Item::Session, &Secret::new(value('x', 100)))
            .unwrap();
        let old = value('o', 6000);
        s.write(Item::Session, &Secret::new(old.clone())).unwrap();
        assert!(
            eventually(|| heads(s) == [("b", 2, 3), ("a", 1, 1)]),
            "{:?}",
            heads(s)
        );
        // Fail after 0, 1, 2 or all 3 new parts (then only the head write
        // is missing). Slot a is overwritten each time; b stays.
        for written in 0..4 {
            s.fail_after(written);
            assert!(s
                .write(Item::Session, &Secret::new(value('n', 7000)))
                .is_err());
            assert!(reads(s, Item::Session, &old));
        }
        // A small value whose single head write fails leaves the old one.
        s.fail_after(1);
        assert!(s.write(Item::Session, &Secret::new("tiny")).is_err());
        assert!(reads(s, Item::Session, &old));
        // The next good write takes over the half-written slot.
        let new = value('n', 5000);
        s.write(Item::Session, &Secret::new(new.clone())).unwrap();
        assert!(reads(s, Item::Session, &new));
        assert!(
            eventually(|| heads(s) == [("a", 3, 2), ("b", 2, 3)]),
            "{:?}",
            heads(s)
        );
    }
}
