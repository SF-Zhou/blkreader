//! Global block device cache.
//!
//! This module provides a global cache for block device file handles,
//! keyed by the device ID (major:minor). This allows multiple reads
//! from files on the same filesystem to share a single file handle
//! to the underlying block device.

use blkpath::ResolveDevice;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// A cached block device entry containing the path and file handle.
#[derive(Debug)]
pub struct CachedDevice {
    /// Path to the block device.
    pub path: PathBuf,
    /// File handle opened with O_DIRECT for reading.
    pub file: File,
}

impl CachedDevice {
    /// Create a new cached device entry.
    fn new(path: PathBuf) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECT)
            .open(&path)?;
        Ok(Self { path, file })
    }
}

/// Global cache for block device handles.
///
/// The cache is keyed by the device ID (from `stat.st_dev`), which
/// uniquely identifies a filesystem. All files on the same filesystem
/// share the same underlying block device.
static DEVICE_CACHE: Lazy<RwLock<HashMap<u64, Arc<CachedDevice>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Get or create a cached block device entry for the given file.
///
/// This function resolves the block device path from the file only if
/// the device is not already cached. This avoids the expensive
/// `resolve_device()` call on every read operation.
///
/// # Arguments
///
/// * `file` - A reference to an open file
///
/// # Returns
///
/// An `Arc` to the cached device entry, or an error if the device
/// could not be resolved or opened.
pub fn get_or_create_cached_device(file: &File) -> io::Result<Arc<CachedDevice>> {
    let dev_id = file.metadata()?.dev();

    // First, try to get from cache with a read lock
    {
        let cache = DEVICE_CACHE.read().unwrap();
        if let Some(entry) = cache.get(&dev_id) {
            return Ok(Arc::clone(entry));
        }
    }

    // Not in cache, resolve device path and acquire write lock
    let device_path = file.resolve_device()?;
    let mut cache = DEVICE_CACHE.write().unwrap();

    // Double-check in case another thread added it
    if let Some(entry) = cache.get(&dev_id) {
        return Ok(Arc::clone(entry));
    }

    // Create new entry
    let entry = Arc::new(CachedDevice::new(device_path)?);
    cache.insert(dev_id, Arc::clone(&entry));
    Ok(entry)
}

/// Open a block device without caching.
///
/// This resolves the block device path from the file and opens it.
///
/// # Arguments
///
/// * `file` - A reference to an open file
///
/// # Returns
///
/// A `CachedDevice` entry (not actually cached), or an error if
/// the device could not be resolved or opened.
pub fn open_device_uncached(file: &File) -> io::Result<CachedDevice> {
    let device_path = file.resolve_device()?;
    CachedDevice::new(device_path)
}

/// Clear the global device cache.
///
/// This is mainly useful for testing.
#[cfg(test)]
pub fn clear_cache() {
    let mut cache = DEVICE_CACHE.write().unwrap();
    cache.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_operations() {
        // Just test that the cache can be cleared without panicking
        clear_cache();
    }
}
