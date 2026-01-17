//! State returned from read operations.

use blkmap::FiemapExtent;
use std::path::PathBuf;

/// Result state from a read operation.
#[derive(Debug, Clone)]
pub struct State {
    /// Path to the block device used for reading.
    pub block_device_path: PathBuf,

    /// List of extents that were involved in the read operation.
    pub extents: Vec<FiemapExtent>,

    /// Number of bytes successfully read.
    pub bytes_read: usize,

    /// Whether the read used fallback (regular file I/O instead of block device).
    pub used_fallback: bool,
}

impl State {
    /// Create a new State with the given parameters.
    pub fn new(
        block_device_path: PathBuf,
        extents: Vec<FiemapExtent>,
        bytes_read: usize,
        used_fallback: bool,
    ) -> Self {
        Self {
            block_device_path,
            extents,
            bytes_read,
            used_fallback,
        }
    }

    /// Create a State for a fallback read (no block device access).
    pub fn fallback(bytes_read: usize) -> Self {
        Self {
            block_device_path: PathBuf::new(),
            extents: Vec::new(),
            bytes_read,
            used_fallback: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blkmap::ExtentFlags;

    #[test]
    fn test_state_new() {
        let state = State::new(
            PathBuf::from("/dev/sda"),
            vec![FiemapExtent {
                logical: 0,
                physical: 1000,
                length: 4096,
                flags: ExtentFlags::empty(),
            }],
            4096,
            false,
        );

        assert_eq!(state.block_device_path, PathBuf::from("/dev/sda"));
        assert_eq!(state.extents.len(), 1);
        assert_eq!(state.bytes_read, 4096);
        assert!(!state.used_fallback);
    }

    #[test]
    fn test_state_fallback() {
        let state = State::fallback(1024);

        assert!(state.block_device_path.as_os_str().is_empty());
        assert!(state.extents.is_empty());
        assert_eq!(state.bytes_read, 1024);
        assert!(state.used_fallback);
    }
}
