//! Global block device cache.
//!
//! This module provides a global cache for block device file handles,
//! keyed by the device ID (major:minor). This allows multiple reads
//! from files on the same filesystem to share a single file handle
//! to the underlying block device.

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// A cached block device entry containing the path and file handle.
#[derive(Debug)]
pub struct CachedDevice {
    /// Path to the block device.
    #[allow(dead_code)]
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

/// Get or create a cached block device entry.
///
/// # Arguments
///
/// * `dev_id` - The device ID from file metadata (`stat.st_dev`)
/// * `device_path` - The path to the block device
///
/// # Returns
///
/// An `Arc` to the cached device entry, or an error if the device
/// could not be opened.
pub fn get_or_create_device(dev_id: u64, device_path: PathBuf) -> io::Result<Arc<CachedDevice>> {
    // First, try to get from cache with a read lock
    {
        let cache = DEVICE_CACHE.read().unwrap();
        if let Some(entry) = cache.get(&dev_id) {
            return Ok(Arc::clone(entry));
        }
    }

    // Not in cache, acquire write lock and create
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
/// # Arguments
///
/// * `device_path` - The path to the block device
///
/// # Returns
///
/// A `CachedDevice` entry (not actually cached), or an error if
/// the device could not be opened.
pub fn open_device_uncached(device_path: PathBuf) -> io::Result<CachedDevice> {
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
