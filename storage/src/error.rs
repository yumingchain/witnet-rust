//! Error type definitions for the Storage module.

use witnet_util as util;

/// Enumerates all the error types generated by the Storage module.
#[derive(Debug)]
pub enum StorageErrors {
    /// Error while trying to open a connection to the storage backend.
    ConnectionError,
    /// Error while trying to put a value for a certain key.
    PutError,
    /// Error while trying to get the value for a certain key.
    GetError,
    /// Error while trying to delete a key/value pair.
    DeleteError
}

/// Error type for the Storage module.
/// Storage backends can only `Err()` this type.
pub type Error = util::error::Error<StorageErrors>;

/// Result type for the Storage module.
/// This is the only return type acceptable for any public method in a storage backend.
pub type Result<T> = util::error::Result<T, StorageErrors>;