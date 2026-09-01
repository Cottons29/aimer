//! A small byte-oriented storage seam.
//!
//! The current implementation provides deterministic, bounded memory and
//! native file adapters plus a pure byte-migration contract. The web
//! IndexedDB adapter and typed serialization helpers remain separate W15
//! slices so this core stays free of browser and serializer dependencies.

#![deny(missing_docs)]

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
#[cfg(not(target_arch = "wasm32"))]
use std::task::{Context, Poll};

#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;
#[cfg(target_arch = "wasm32")]
use std::rc::Rc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use crossbeam::channel::{Receiver, Sender, TryRecvError, bounded, unbounded};
#[cfg(not(target_arch = "wasm32"))]
use futures_util::task::AtomicWaker;

#[cfg(not(target_arch = "wasm32"))]
use std::fs::{self, File, OpenOptions};
#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicU64, Ordering};

/// The result type returned by storage operations.
pub type StorageResult<T> = Result<T, StorageError>;

/// A boxed asynchronous storage operation.
pub type StorageFuture<T> = Pin<Box<dyn Future<Output = StorageResult<T>> + 'static>>;

/// A platform capability that may be supplied by a future adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageCapability {
    /// A native filesystem-backed adapter.
    NativeFile,
    /// A browser IndexedDB-backed adapter.
    WebIndexedDb,
}

/// An operation that can fail at the storage boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageOperation {
    /// Reading one value.
    Read,
    /// Replacing or creating one value.
    Write,
    /// Removing one value.
    Remove,
    /// Clearing one namespace.
    Clear,
    /// Listing one namespace.
    List,
    /// Reading one value's metadata.
    Metadata,
}

/// A coarse native I/O failure that does not fit one of the typed storage
/// capability errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageIoKind {
    /// The underlying operation failed for another operating-system reason.
    Other,
}

/// The quota dimension that rejected a write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotaKind {
    /// One value exceeds the per-value byte limit.
    ValueBytes,
    /// The resulting adapter exceeds the total byte limit.
    TotalBytes,
    /// The resulting adapter exceeds the entry-count limit.
    Entries,
}

/// Errors returned by storage adapters.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageError {
    /// A namespace or key is empty, too long, or contains an unsafe byte.
    InvalidKey,
    /// A limits configuration contains a zero bound.
    InvalidLimits,
    /// A configured quota would be exceeded.
    QuotaExceeded {
        /// The quota dimension that would be exceeded.
        kind: QuotaKind,
        /// The resulting amount requested by the operation.
        requested: usize,
        /// The configured maximum.
        limit: usize,
    },
    /// A bounded listing contains more entries than the requested bound.
    LimitExceeded {
        /// The number of entries needed to return the complete listing.
        requested: usize,
        /// The configured listing bound.
        limit: usize,
    },
    /// The requested platform capability is unavailable.
    Unavailable {
        /// The unavailable capability.
        capability: StorageCapability,
    },
    /// The operating system denied an operation.
    PermissionDenied {
        /// The denied operation.
        operation: StorageOperation,
    },
    /// Stored bytes or metadata are corrupt.
    Corrupt {
        /// The operation that encountered corrupt data.
        operation: StorageOperation,
    },
    /// The operating system reported an unclassified I/O failure.
    Io {
        /// The operation that failed.
        operation: StorageOperation,
        /// The coarse native error category.
        kind: StorageIoKind,
    },
    /// No exact next step exists for a requested schema migration.
    MigrationUnavailable {
        /// The version at which migration stopped.
        from_version: u32,
        /// The requested final version.
        to_version: u32,
    },
    /// A migration does not move monotonically toward its target version.
    InvalidMigration {
        /// The version accepted by the invalid migration.
        from_version: u32,
        /// The invalid version it produces.
        to_version: u32,
    },
    /// An adapter does not support the requested operation.
    Unsupported {
        /// The unsupported operation.
        operation: StorageOperation,
    },
}

/// Bounded resource limits applied by a storage adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageLimits {
    max_value_bytes: usize,
    max_total_bytes: usize,
    max_entries: usize,
    max_list_entries: usize,
}

impl StorageLimits {
    /// The maximum UTF-8 byte length of a namespace or key component.
    pub const MAX_COMPONENT_BYTES: usize = 256;
    /// The default maximum size of one stored value.
    pub const DEFAULT_MAX_VALUE_BYTES: usize = 1024 * 1024;
    /// The default total size of one adapter.
    pub const DEFAULT_MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
    /// The default number of values in one adapter.
    pub const DEFAULT_MAX_ENTRIES: usize = 10_000;
    /// The default number of entries returned by one listing.
    pub const DEFAULT_MAX_LIST_ENTRIES: usize = 1_024;

    /// Creates validated storage limits.
    pub fn new(
        max_value_bytes: usize,
        max_total_bytes: usize,
        max_entries: usize,
        max_list_entries: usize,
    ) -> StorageResult<Self> {
        if max_value_bytes == 0
            || max_total_bytes == 0
            || max_entries == 0
            || max_list_entries == 0
        {
            return Err(StorageError::InvalidLimits);
        }
        Ok(Self {
            max_value_bytes,
            max_total_bytes,
            max_entries,
            max_list_entries,
        })
    }

    /// Returns the maximum size of one value.
    pub const fn max_value_bytes(self) -> usize {
        self.max_value_bytes
    }

    /// Returns the maximum total size of the adapter.
    pub const fn max_total_bytes(self) -> usize {
        self.max_total_bytes
    }

    /// Returns the maximum number of values in the adapter.
    pub const fn max_entries(self) -> usize {
        self.max_entries
    }

    /// Returns the maximum number of entries in one listing response.
    pub const fn max_list_entries(self) -> usize {
        self.max_list_entries
    }
}

impl Default for StorageLimits {
    fn default() -> Self {
        Self {
            max_value_bytes: Self::DEFAULT_MAX_VALUE_BYTES,
            max_total_bytes: Self::DEFAULT_MAX_TOTAL_BYTES,
            max_entries: Self::DEFAULT_MAX_ENTRIES,
            max_list_entries: Self::DEFAULT_MAX_LIST_ENTRIES,
        }
    }
}

/// Metadata for one stored value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageMetadata {
    size: usize,
}

impl StorageMetadata {
    /// Returns the value's byte length.
    pub const fn size(self) -> usize {
        self.size
    }
}

/// A key and its metadata returned by a namespace listing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageEntry {
    key: String,
    metadata: StorageMetadata,
}

impl StorageEntry {
    /// Returns the stored key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the stored value's byte length.
    pub const fn size(&self) -> usize {
        self.metadata.size()
    }
}

/// A bounded listing response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageList {
    entries: Vec<StorageEntry>,
    truncated: bool,
}

impl StorageList {
    /// Returns the entries in deterministic key order.
    pub fn entries(&self) -> &[StorageEntry] {
        &self.entries
    }

    /// Returns whether additional entries exist beyond the bound.
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
}

/// A byte-oriented storage backend.
pub trait Storage {
    /// Reads a value from a namespace, returning `None` when it is absent.
    fn read(&self, namespace: &str, key: &str) -> StorageFuture<Option<Vec<u8>>>;

    /// Replaces or creates a value in a namespace.
    fn write(&self, namespace: &str, key: &str, value: &[u8]) -> StorageFuture<()>;

    /// Removes one value and reports whether it existed.
    fn remove(&self, namespace: &str, key: &str) -> StorageFuture<bool>;

    /// Removes all values in one namespace and reports the number removed.
    fn clear(&self, namespace: &str) -> StorageFuture<usize>;

    /// Lists up to the requested number of values in a namespace.
    fn list(&self, namespace: &str, limit: usize) -> StorageFuture<StorageList>;

    /// Returns metadata for one value, if it exists.
    fn metadata(&self, namespace: &str, key: &str) -> StorageFuture<Option<StorageMetadata>>;
}

/// A versioned transformation for one stored byte value.
pub trait StorageMigration {
    /// Returns the schema version accepted by this migration.
    fn from_version(&self) -> u32;

    /// Returns the schema version produced by this migration.
    fn to_version(&self) -> u32;

    /// Transforms one value without changing its storage key or namespace.
    fn migrate(&self, value: &[u8]) -> StorageResult<Vec<u8>>;
}

/// Applies exact-version migrations in order until `to_version` is reached.
pub fn migrate_bytes(
    value: &[u8],
    from_version: u32,
    to_version: u32,
    migrations: &[&dyn StorageMigration],
) -> StorageResult<Vec<u8>> {
    if from_version == to_version {
        return Ok(value.to_vec());
    }
    if from_version > to_version {
        return Err(StorageError::MigrationUnavailable {
            from_version,
            to_version,
        });
    }

    let mut version = from_version;
    let mut migrated = value.to_vec();
    for _ in 0..=migrations.len() {
        if version == to_version {
            return Ok(migrated);
        }
        let Some(migration) = migrations.iter().find(|migration| migration.from_version() == version)
        else {
            return Err(StorageError::MigrationUnavailable {
                from_version: version,
                to_version,
            });
        };
        if migration.to_version() <= version || migration.to_version() > to_version {
            return Err(StorageError::InvalidMigration {
                from_version: version,
                to_version: migration.to_version(),
            });
        }
        migrated = migration.migrate(&migrated)?;
        version = migration.to_version();
    }

    Err(StorageError::MigrationUnavailable {
        from_version: version,
        to_version,
    })
}

#[derive(Clone, Default)]
struct MemoryState {
    values: BTreeMap<(String, String), Vec<u8>>,
    total_bytes: usize,
}

#[cfg(not(target_arch = "wasm32"))]
type MemoryJob = Box<dyn FnOnce(&mut MemoryState) + Send + 'static>;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct MemoryDispatcher {
    sender: Sender<MemoryJob>,
}

#[cfg(not(target_arch = "wasm32"))]
impl MemoryDispatcher {
    fn new() -> Self {
        let (sender, receiver) = unbounded::<MemoryJob>();
        std::thread::Builder::new()
            .name("aimer-memory-storage".to_owned())
            .spawn(move || {
                let mut state = MemoryState::default();
                while let Ok(job) = receiver.recv() {
                    job(&mut state);
                }
            })
            .expect("memory storage worker thread should be available");
        Self { sender }
    }

    fn submit<T, F>(&self, operation: F, operation_kind: StorageOperation) -> StorageFuture<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut MemoryState) -> StorageResult<T> + Send + 'static,
    {
        let (future, result_sender, waker) = channel_future(operation_kind);
        let fallback_sender = result_sender.clone();
        let fallback_waker = Arc::clone(&waker);
        let job = Box::new(move |state: &mut MemoryState| {
            let _ = result_sender.send(operation(state));
            waker.wake();
        });
        if self.sender.send(job).is_err() {
            let _ = fallback_sender.send(Err(disconnected(operation_kind)));
            fallback_waker.wake();
        }
        future
    }
}

/// A deterministic in-memory storage adapter.
#[derive(Clone)]
pub struct MemoryStorage {
    #[cfg(not(target_arch = "wasm32"))]
    state: MemoryDispatcher,
    #[cfg(target_arch = "wasm32")]
    state: Rc<RefCell<MemoryState>>,
    limits: StorageLimits,
}

impl MemoryStorage {
    /// Creates an empty memory adapter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty memory adapter with explicit resource limits.
    pub fn with_limits(limits: StorageLimits) -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            state: MemoryDispatcher::new(),
            #[cfg(target_arch = "wasm32")]
            state: Rc::new(RefCell::new(MemoryState::default())),
            limits,
        }
    }

    /// Returns the limits enforced by this adapter.
    pub const fn limits(&self) -> StorageLimits {
        self.limits
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::with_limits(StorageLimits::default())
    }
}

fn ready<T: 'static>(result: StorageResult<T>) -> StorageFuture<T> {
    Box::pin(async move { result })
}

impl MemoryStorage {
    #[cfg(not(target_arch = "wasm32"))]
    fn execute<T, F>(&self, operation: F, operation_kind: StorageOperation) -> StorageFuture<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut MemoryState) -> StorageResult<T> + Send + 'static,
    {
        self.state.submit(operation, operation_kind)
    }

    #[cfg(target_arch = "wasm32")]
    fn execute<T, F>(&self, operation: F, _operation_kind: StorageOperation) -> StorageFuture<T>
    where
        T: 'static,
        F: FnOnce(&mut MemoryState) -> StorageResult<T> + 'static,
    {
        ready(operation(&mut self.state.borrow_mut()))
    }
}

fn validate_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= StorageLimits::MAX_COMPONENT_BYTES
        && value != "."
        && value != ".."
        && value.bytes().all(|byte| {
            byte >= 0x20 && byte != 0x7f && byte != b'/' && byte != b'\\' && byte != b':'
        })
}

fn validate_namespace(namespace: &str) -> StorageResult<()> {
    if !validate_component(namespace) {
        Err(StorageError::InvalidKey)
    } else {
        Ok(())
    }
}

fn validate(namespace: &str, key: &str) -> StorageResult<()> {
    validate_namespace(namespace)?;
    if !validate_component(key) {
        Err(StorageError::InvalidKey)
    } else {
        Ok(())
    }
}

impl Storage for MemoryStorage {
    fn read(&self, namespace: &str, key: &str) -> StorageFuture<Option<Vec<u8>>> {
        if let Err(error) = validate(namespace, key) {
            return ready(Err(error));
        }
        let namespace = namespace.to_owned();
        let key = key.to_owned();
        self.execute(
            move |state| Ok(state.values.get(&(namespace, key)).cloned()),
            StorageOperation::Read,
        )
    }

    fn write(&self, namespace: &str, key: &str, value: &[u8]) -> StorageFuture<()> {
        if let Err(error) = validate(namespace, key) {
            return ready(Err(error));
        }
        if value.len() > self.limits.max_value_bytes() {
            return ready(Err(StorageError::QuotaExceeded {
                kind: QuotaKind::ValueBytes,
                requested: value.len(),
                limit: self.limits.max_value_bytes(),
            }));
        }
        let namespace = namespace.to_owned();
        let key = key.to_owned();
        let value = value.to_vec();
        let limits = self.limits;
        self.execute(
            move |state| {
                let storage_key = (namespace, key);
                let old_len = state.values.get(&storage_key).map_or(0, Vec::len);
                let new_total = state
                    .total_bytes
                    .checked_sub(old_len)
                    .and_then(|total| total.checked_add(value.len()))
                    .unwrap_or(usize::MAX);
                if new_total > limits.max_total_bytes() {
                    return Err(StorageError::QuotaExceeded {
                        kind: QuotaKind::TotalBytes,
                        requested: new_total,
                        limit: limits.max_total_bytes(),
                    });
                }
                if !state.values.contains_key(&storage_key)
                    && state.values.len() >= limits.max_entries()
                {
                    return Err(StorageError::QuotaExceeded {
                        kind: QuotaKind::Entries,
                        requested: state.values.len().saturating_add(1),
                        limit: limits.max_entries(),
                    });
                }
                state.values.insert(storage_key, value);
                state.total_bytes = new_total;
                Ok(())
            },
            StorageOperation::Write,
        )
    }

    fn remove(&self, namespace: &str, key: &str) -> StorageFuture<bool> {
        if let Err(error) = validate(namespace, key) {
            return ready(Err(error));
        }
        let namespace = namespace.to_owned();
        let key = key.to_owned();
        self.execute(
            move |state| {
                let removed = state.values.remove(&(namespace, key));
                let existed = removed.is_some();
                if let Some(value) = removed {
                    state.total_bytes = state.total_bytes.saturating_sub(value.len());
                }
                Ok(existed)
            },
            StorageOperation::Remove,
        )
    }

    fn clear(&self, namespace: &str) -> StorageFuture<usize> {
        if let Err(error) = validate_namespace(namespace) {
            return ready(Err(error));
        }
        let namespace = namespace.to_owned();
        self.execute(
            move |state| {
                let keys = state
                    .values
                    .keys()
                    .filter(|(stored_namespace, _)| stored_namespace == &namespace)
                    .cloned()
                    .collect::<Vec<_>>();
                let mut removed = 0;
                for key in keys {
                    if let Some(value) = state.values.remove(&key) {
                        state.total_bytes = state.total_bytes.saturating_sub(value.len());
                        removed += 1;
                    }
                }
                Ok(removed)
            },
            StorageOperation::Clear,
        )
    }

    fn list(&self, namespace: &str, limit: usize) -> StorageFuture<StorageList> {
        if let Err(error) = validate_namespace(namespace) {
            return ready(Err(error));
        }
        if limit > self.limits.max_list_entries() {
            return ready(Err(StorageError::LimitExceeded {
                requested: limit,
                limit: self.limits.max_list_entries(),
            }));
        }
        let namespace = namespace.to_owned();
        self.execute(
            move |state| {
                let mut values = state
                    .values
                    .iter()
                    .filter(|((stored_namespace, _), _)| stored_namespace == &namespace);
                let mut entries = Vec::with_capacity(limit);
                for _ in 0..limit {
                    let Some(((_, key), value)) = values.next() else {
                        break;
                    };
                    entries.push(StorageEntry {
                        key: key.clone(),
                        metadata: StorageMetadata { size: value.len() },
                    });
                }
                let truncated = values.next().is_some();
                Ok(StorageList { entries, truncated })
            },
            StorageOperation::List,
        )
    }

    fn metadata(&self, namespace: &str, key: &str) -> StorageFuture<Option<StorageMetadata>> {
        if let Err(error) = validate(namespace, key) {
            return ready(Err(error));
        }
        let namespace = namespace.to_owned();
        let key = key.to_owned();
        self.execute(
            move |state| {
                Ok(state
                    .values
                    .get(&(namespace, key))
                    .map(|value| StorageMetadata { size: value.len() }))
            },
            StorageOperation::Metadata,
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct ChannelIo<T> {
    receiver: Receiver<StorageResult<T>>,
    waker: Arc<AtomicWaker>,
    operation: StorageOperation,
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> Future for ChannelIo<T> {
    type Output = StorageResult<T>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this.receiver.try_recv() {
            Ok(result) => Poll::Ready(result),
            Err(TryRecvError::Disconnected) => Poll::Ready(Err(disconnected(this.operation))),
            Err(TryRecvError::Empty) => {
                this.waker.register(context.waker());
                match this.receiver.try_recv() {
                    Ok(result) => Poll::Ready(result),
                    Err(TryRecvError::Disconnected) => {
                        Poll::Ready(Err(disconnected(this.operation)))
                    }
                    Err(TryRecvError::Empty) => Poll::Pending,
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<T> Unpin for ChannelIo<T> {}

#[cfg(not(target_arch = "wasm32"))]
fn channel_future<T: 'static>(
    operation: StorageOperation,
) -> (
    StorageFuture<T>,
    Sender<StorageResult<T>>,
    Arc<AtomicWaker>,
) {
    let (sender, receiver) = bounded(1);
    let waker = Arc::new(AtomicWaker::new());
    let future = Box::pin(ChannelIo {
        receiver,
        waker: Arc::clone(&waker),
        operation,
    });
    (future, sender, waker)
}

#[cfg(not(target_arch = "wasm32"))]
fn disconnected(operation: StorageOperation) -> StorageError {
    StorageError::Io {
        operation,
        kind: StorageIoKind::Other,
    }
}

#[cfg(not(target_arch = "wasm32"))]
type SerialJob = Box<dyn FnOnce() + Send + 'static>;

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone)]
struct SerialDispatcher {
    sender: Sender<SerialJob>,
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Debug for SerialDispatcher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SerialDispatcher")
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl SerialDispatcher {
    fn new(name: &'static str) -> Self {
        let (sender, receiver) = unbounded::<SerialJob>();
        std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    job();
                }
            })
            .expect("storage worker thread should be available");
        Self { sender }
    }

    fn submit<T, F>(&self, operation: F, operation_kind: StorageOperation) -> StorageFuture<T>
    where
        T: Send + 'static,
        F: FnOnce() -> StorageResult<T> + Send + 'static,
    {
        let (future, result_sender, waker) = channel_future(operation_kind);
        let fallback_sender = result_sender.clone();
        let fallback_waker = Arc::clone(&waker);
        let job = Box::new(move || {
            let _ = result_sender.send(operation());
            waker.wake();
        });
        if self.sender.send(job).is_err() {
            let _ = fallback_sender.send(Err(disconnected(operation_kind)));
            fallback_waker.wake();
        }
        future
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_io<T, F>(operation: F, operation_kind: StorageOperation) -> StorageFuture<T>
where
    T: Send + 'static,
    F: FnOnce() -> StorageResult<T> + Send + 'static,
{
    let (future, result_sender, waker) = channel_future(operation_kind);
    std::thread::Builder::new()
        .name("aimer-storage-io".to_owned())
        .spawn(move || {
            let _ = result_sender.send(operation());
            waker.wake();
        })
        .expect("storage worker thread should be available");
    future
}

#[cfg(not(target_arch = "wasm32"))]
fn native_error(operation: StorageOperation, error: io::Error) -> StorageError {
    match error.kind() {
        io::ErrorKind::PermissionDenied => StorageError::PermissionDenied { operation },
        io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof => {
            StorageError::Corrupt { operation }
        }
        _ => StorageError::Io {
            operation,
            kind: StorageIoKind::Other,
        },
    }
}

#[cfg(not(target_arch = "wasm32"))]
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

#[cfg(not(target_arch = "wasm32"))]
fn encode_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        encoded.push(HEX_DIGITS[(byte >> 4) as usize] as char);
        encoded.push(HEX_DIGITS[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_component(value: &str, operation: StorageOperation) -> StorageResult<String> {
    if value.is_empty() || value.len() % 2 != 0 {
        return Err(StorageError::Corrupt { operation });
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let Some(high) = decode_hex(pair[0]) else {
            return Err(StorageError::Corrupt { operation });
        };
        let Some(low) = decode_hex(pair[1]) else {
            return Err(StorageError::Corrupt { operation });
        };
        bytes.push((high << 4) | low);
    }
    let decoded = String::from_utf8(bytes).map_err(|_| StorageError::Corrupt { operation })?;
    if validate_component(&decoded) {
        Ok(decoded)
    } else {
        Err(StorageError::Corrupt { operation })
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn file_path(root: &Path, namespace: &str, key: &str) -> PathBuf {
    root.join(encode_component(namespace)).join(encode_component(key))
}

#[cfg(not(target_arch = "wasm32"))]
fn value_size(path: &Path, operation: StorageOperation) -> StorageResult<usize> {
    let metadata = fs::metadata(path).map_err(|error| native_error(operation, error))?;
    if !metadata.is_file() {
        return Err(StorageError::Corrupt { operation });
    }
    usize::try_from(metadata.len()).map_err(|_| StorageError::Corrupt { operation })
}

#[cfg(not(target_arch = "wasm32"))]
struct DiskStats {
    entries: usize,
    total_bytes: usize,
}

#[cfg(not(target_arch = "wasm32"))]
fn scan_disk(root: &Path, operation: StorageOperation) -> StorageResult<DiskStats> {
    let namespaces = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(DiskStats {
                entries: 0,
                total_bytes: 0,
            });
        }
        Err(error) => return Err(native_error(operation, error)),
    };

    let mut stats = DiskStats {
        entries: 0,
        total_bytes: 0,
    };
    for namespace_entry in namespaces {
        let namespace_entry = namespace_entry.map_err(|error| native_error(operation, error))?;
        let file_type = namespace_entry
            .file_type()
            .map_err(|error| native_error(operation, error))?;
        if !file_type.is_dir() {
            return Err(StorageError::Corrupt { operation });
        }
        let encoded_namespace = namespace_entry
            .file_name()
            .into_string()
            .map_err(|_| StorageError::Corrupt { operation })?;
        decode_component(&encoded_namespace, operation)?;
        for value_entry in fs::read_dir(namespace_entry.path())
            .map_err(|error| native_error(operation, error))?
        {
            let value_entry = value_entry.map_err(|error| native_error(operation, error))?;
            let name = value_entry
                .file_name()
                .into_string()
                .map_err(|_| StorageError::Corrupt { operation })?;
            if name.starts_with(".aimer-tmp-") {
                continue;
            }
            let file_type = value_entry
                .file_type()
                .map_err(|error| native_error(operation, error))?;
            if !file_type.is_file() {
                return Err(StorageError::Corrupt { operation });
            }
            decode_component(&name, operation)?;
            let size = value_size(&value_entry.path(), operation)?;
            stats.entries = stats.entries.checked_add(1).ok_or(StorageError::Corrupt { operation })?;
            stats.total_bytes = stats
                .total_bytes
                .checked_add(size)
                .ok_or(StorageError::Corrupt { operation })?;
        }
    }
    Ok(stats)
}

#[cfg(not(target_arch = "wasm32"))]
fn namespace_entries(
    root: &Path,
    namespace: &str,
    operation: StorageOperation,
) -> StorageResult<Vec<(String, usize)>> {
    let namespace_path = root.join(encode_component(namespace));
    let values = match fs::read_dir(&namespace_path) {
        Ok(values) => values,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(native_error(operation, error)),
    };
    let mut entries = Vec::new();
    for value_entry in values {
        let value_entry = value_entry.map_err(|error| native_error(operation, error))?;
        let name = value_entry
            .file_name()
            .into_string()
            .map_err(|_| StorageError::Corrupt { operation })?;
        if name.starts_with(".aimer-tmp-") {
            continue;
        }
        let file_type = value_entry
            .file_type()
            .map_err(|error| native_error(operation, error))?;
        if !file_type.is_file() {
            return Err(StorageError::Corrupt { operation });
        }
        let key = decode_component(&name, operation)?;
        let size = value_size(&value_entry.path(), operation)?;
        entries.push((key, size));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(entries)
}

#[cfg(not(target_arch = "wasm32"))]
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

#[cfg(not(target_arch = "wasm32"))]
fn write_atomically(path: &Path, value: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "storage path has no parent"))?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(
        ".aimer-tmp-{}-{}",
        std::process::id(),
        NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(value)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// A native file-backed storage adapter.
///
/// Each operation runs on a worker thread, values are stored below an
/// encoded namespace/key path, and writes use a temporary file plus rename so
/// readers never observe a partially-written value. The adapter is available
/// on native targets only; browser storage requires a separate IndexedDB
/// adapter.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
pub struct FileStorage {
    root: Arc<PathBuf>,
    writer: SerialDispatcher,
    limits: StorageLimits,
}

#[cfg(not(target_arch = "wasm32"))]
impl FileStorage {
    /// Creates a file-backed adapter rooted at `root` with default limits.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_limits(root, StorageLimits::default())
    }

    /// Creates a file-backed adapter rooted at `root` with explicit limits.
    pub fn with_limits(root: impl Into<PathBuf>, limits: StorageLimits) -> Self {
        Self {
            root: Arc::new(root.into()),
            writer: SerialDispatcher::new("aimer-file-storage-writer"),
            limits,
        }
    }

    /// Returns the directory used by this adapter.
    pub fn root(&self) -> &Path {
        self.root.as_path()
    }

    /// Returns the limits enforced by this adapter.
    pub const fn limits(&self) -> StorageLimits {
        self.limits
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Storage for FileStorage {
    fn read(&self, namespace: &str, key: &str) -> StorageFuture<Option<Vec<u8>>> {
        if let Err(error) = validate(namespace, key) {
            return ready(Err(error));
        }
        let path = file_path(self.root.as_path(), namespace, key);
        let limits = self.limits;
        spawn_io(
            move || match fs::read(&path) {
                Ok(value) if value.len() <= limits.max_value_bytes() => Ok(Some(value)),
                Ok(value) => Err(StorageError::QuotaExceeded {
                    kind: QuotaKind::ValueBytes,
                    requested: value.len(),
                    limit: limits.max_value_bytes(),
                }),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(native_error(StorageOperation::Read, error)),
            },
            StorageOperation::Read,
        )
    }

    fn write(&self, namespace: &str, key: &str, value: &[u8]) -> StorageFuture<()> {
        if let Err(error) = validate(namespace, key) {
            return ready(Err(error));
        }
        if value.len() > self.limits.max_value_bytes() {
            return ready(Err(StorageError::QuotaExceeded {
                kind: QuotaKind::ValueBytes,
                requested: value.len(),
                limit: self.limits.max_value_bytes(),
            }));
        }
        let root = Arc::clone(&self.root);
        let writer = self.writer.clone();
        let limits = self.limits;
        let namespace = namespace.to_owned();
        let key = key.to_owned();
        let value = value.to_vec();
        writer.submit(move || {
            let stats = scan_disk(root.as_path(), StorageOperation::Write)?;
            let path = file_path(root.as_path(), &namespace, &key);
            let old_len = match fs::metadata(&path) {
                Ok(metadata) if metadata.is_file() => usize::try_from(metadata.len())
                    .map_err(|_| StorageError::Corrupt {
                        operation: StorageOperation::Write,
                    })?,
                Ok(_) => {
                    return Err(StorageError::Corrupt {
                        operation: StorageOperation::Write,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
                Err(error) => return Err(native_error(StorageOperation::Write, error)),
            };
            let total_bytes = stats
                .total_bytes
                .checked_sub(old_len)
                .and_then(|total| total.checked_add(value.len()))
                .ok_or(StorageError::Corrupt {
                    operation: StorageOperation::Write,
                })?;
            if total_bytes > limits.max_total_bytes() {
                return Err(StorageError::QuotaExceeded {
                    kind: QuotaKind::TotalBytes,
                    requested: total_bytes,
                    limit: limits.max_total_bytes(),
                });
            }
            if old_len == 0 && !path.exists() && stats.entries >= limits.max_entries() {
                return Err(StorageError::QuotaExceeded {
                    kind: QuotaKind::Entries,
                    requested: stats.entries.saturating_add(1),
                    limit: limits.max_entries(),
                });
            }
            write_atomically(&path, &value)
                .map_err(|error| native_error(StorageOperation::Write, error))
        }, StorageOperation::Write)
    }

    fn remove(&self, namespace: &str, key: &str) -> StorageFuture<bool> {
        if let Err(error) = validate(namespace, key) {
            return ready(Err(error));
        }
        let path = file_path(self.root.as_path(), namespace, key);
        let writer = self.writer.clone();
        writer.submit(move || {
            match fs::remove_file(&path) {
                Ok(()) => Ok(true),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(native_error(StorageOperation::Remove, error)),
            }
        }, StorageOperation::Remove)
    }

    fn clear(&self, namespace: &str) -> StorageFuture<usize> {
        if let Err(error) = validate_namespace(namespace) {
            return ready(Err(error));
        }
        let root = Arc::clone(&self.root);
        let writer = self.writer.clone();
        let namespace = namespace.to_owned();
        writer.submit(move || {
            let namespace_path = root.join(encode_component(&namespace));
            let values = match fs::read_dir(&namespace_path) {
                Ok(values) => values,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
                Err(error) => return Err(native_error(StorageOperation::Clear, error)),
            };
            let mut removed = 0;
            for value_entry in values {
                let value_entry = value_entry
                    .map_err(|error| native_error(StorageOperation::Clear, error))?;
                let name = value_entry
                    .file_name()
                    .into_string()
                    .map_err(|_| StorageError::Corrupt {
                        operation: StorageOperation::Clear,
                    })?;
                let file_type = value_entry
                    .file_type()
                    .map_err(|error| native_error(StorageOperation::Clear, error))?;
                if !file_type.is_file()
                    || (!name.starts_with(".aimer-tmp-")
                        && decode_component(&name, StorageOperation::Clear).is_err())
                {
                    return Err(StorageError::Corrupt {
                        operation: StorageOperation::Clear,
                    });
                }
                fs::remove_file(value_entry.path())
                    .map_err(|error| native_error(StorageOperation::Clear, error))?;
                if !name.starts_with(".aimer-tmp-") {
                    removed += 1;
                }
            }
            let _ = fs::remove_dir(&namespace_path);
            Ok(removed)
        }, StorageOperation::Clear)
    }

    fn list(&self, namespace: &str, limit: usize) -> StorageFuture<StorageList> {
        if let Err(error) = validate_namespace(namespace) {
            return ready(Err(error));
        }
        if limit > self.limits.max_list_entries() {
            return ready(Err(StorageError::LimitExceeded {
                requested: limit,
                limit: self.limits.max_list_entries(),
            }));
        }
        let root = Arc::clone(&self.root);
        let namespace = namespace.to_owned();
        spawn_io(
            move || {
                let values = namespace_entries(root.as_path(), &namespace, StorageOperation::List)?;
                let truncated = values.len() > limit;
                let entries = values
                    .into_iter()
                    .take(limit)
                    .map(|(key, size)| StorageEntry {
                        key,
                        metadata: StorageMetadata { size },
                    })
                    .collect();
                Ok(StorageList { entries, truncated })
            },
            StorageOperation::List,
        )
    }

    fn metadata(&self, namespace: &str, key: &str) -> StorageFuture<Option<StorageMetadata>> {
        if let Err(error) = validate(namespace, key) {
            return ready(Err(error));
        }
        let path = file_path(self.root.as_path(), namespace, key);
        spawn_io(
            move || match value_size(&path, StorageOperation::Metadata) {
                Ok(size) => Ok(Some(StorageMetadata { size })),
                Err(StorageError::Io { kind: _, .. }) if !path.exists() => Ok(None),
                Err(StorageError::Unavailable { .. }) if !path.exists() => Ok(None),
                Err(error) => Err(error),
            },
            StorageOperation::Metadata,
        )
    }
}
