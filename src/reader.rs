//! Core reader trait and implementations.
//!
//! This module provides the [`BlkReader`] trait which enables reading file data
//! directly from the underlying block device using extent information.

use crate::cache::{get_or_create_device, open_device_uncached, CachedDevice};
use crate::options::Options;
use crate::state::State;

use blkmap::{Fiemap, FiemapExtent};
use blkpath::ResolveDevice;

use std::fs::File;
use std::io;
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Trait for reading file data directly from block devices.
///
/// This trait provides two methods for reading:
/// - [`blk_read_at`](BlkReader::blk_read_at): Simple read that returns the number of bytes read
/// - [`blk_read_at_opt`](BlkReader::blk_read_at_opt): Advanced read with options that returns detailed state
///
/// # Example
///
/// ```no_run
/// use blkreader::{BlkReader, Options};
/// use std::path::Path;
///
/// let path = Path::new("/path/to/file");
/// let mut buf = vec![0u8; 4096];
///
/// // Simple read
/// let bytes = path.blk_read_at(&mut buf, 0).unwrap();
///
/// // Read with options
/// let opts = Options::new().with_fill_holes(true);
/// let state = path.blk_read_at_opt(&mut buf, 0, &opts).unwrap();
/// ```
pub trait BlkReader {
    /// Read data from the file at the specified offset.
    ///
    /// This is a convenience method that calls [`blk_read_at_opt`](BlkReader::blk_read_at_opt)
    /// with default options and returns just the number of bytes read.
    ///
    /// # Arguments
    ///
    /// * `buf` - Buffer to read data into
    /// * `offset` - Byte offset in the file to start reading from
    ///
    /// # Returns
    ///
    /// The number of bytes successfully read, or an error.
    fn blk_read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        let state = self.blk_read_at_opt(buf, offset, &Options::default())?;
        Ok(state.bytes_read)
    }

    /// Read data from the file at the specified offset with options.
    ///
    /// This method queries the file's extent information, resolves the block device,
    /// and reads data directly from the physical locations on disk.
    ///
    /// # Arguments
    ///
    /// * `buf` - Buffer to read data into
    /// * `offset` - Byte offset in the file to start reading from
    /// * `options` - Configuration options for the read operation
    ///
    /// # Returns
    ///
    /// A [`State`] containing the block device path, extent information,
    /// and number of bytes read, or an error.
    fn blk_read_at_opt(&self, buf: &mut [u8], offset: u64, options: &Options) -> io::Result<State>;
}

/// Internal helper to perform the actual read operation.
struct ReadContext<'a> {
    file: &'a File,
    file_path: Option<&'a Path>,
    options: &'a Options,
}

impl<'a> ReadContext<'a> {
    fn new(file: &'a File, file_path: Option<&'a Path>, options: &'a Options) -> Self {
        Self {
            file,
            file_path,
            options,
        }
    }

    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<State> {
        if buf.is_empty() {
            return Ok(State::fallback(0));
        }

        let length = buf.len() as u64;

        // Query extent information for the requested range
        let extents = self.file.fiemap_range(offset, length)?;

        if extents.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "file has no extents",
            ));
        }

        // Check if fallback is allowed and safe
        if self.options.allow_fallback && self.can_use_fallback(&extents, offset, length) {
            return self.fallback_read(buf, offset);
        }

        // Get block device path
        let device_path = self.resolve_device_path()?;

        // Get device file handle (cached or uncached)
        let device = self.get_device_handle(&device_path)?;

        // Perform the read
        let bytes_read = self.read_from_device(&device, buf, offset, &extents)?;

        Ok(State::new(device_path, extents, bytes_read, false))
    }

    /// Check if we can safely use fallback (regular file I/O).
    ///
    /// Fallback is safe if:
    /// 1. All extents fully cover the requested range
    /// 2. No extents are unwritten
    /// 3. No holes in the range
    fn can_use_fallback(&self, extents: &[FiemapExtent], offset: u64, length: u64) -> bool {
        if extents.is_empty() {
            return false;
        }

        let end = offset + length;
        let mut current = offset;

        for extent in extents {
            // Check for hole before this extent
            if extent.logical > current {
                return false;
            }

            // Check for unwritten extent
            if extent.flags.is_unwritten() {
                return false;
            }

            // Check for unknown/delalloc (hole-like)
            if extent.flags.is_unknown() || extent.flags.is_delalloc() {
                return false;
            }

            // Update current position
            let extent_end = extent.logical + extent.length;
            if extent_end >= end {
                return true;
            }
            current = extent_end;
        }

        false
    }

    /// Perform a fallback read using regular file I/O.
    fn fallback_read(&self, buf: &mut [u8], offset: u64) -> io::Result<State> {
        let bytes_read = FileExt::read_at(self.file, buf, offset)?;
        Ok(State::fallback(bytes_read))
    }

    /// Resolve the block device path for the file.
    fn resolve_device_path(&self) -> io::Result<PathBuf> {
        if let Some(path) = self.file_path {
            path.resolve_device()
        } else {
            self.file.resolve_device()
        }
    }

    /// Get a device handle, either cached or uncached based on options.
    fn get_device_handle(&self, device_path: &Path) -> io::Result<DeviceHandle> {
        if self.options.enable_cache {
            let dev_id = self.file.metadata()?.dev();
            let cached = get_or_create_device(dev_id, device_path.to_path_buf())?;
            Ok(DeviceHandle::Cached(cached))
        } else {
            let uncached = open_device_uncached(device_path.to_path_buf())?;
            Ok(DeviceHandle::Uncached(uncached))
        }
    }

    /// Read data from the block device based on extent information.
    fn read_from_device(
        &self,
        device: &DeviceHandle,
        buf: &mut [u8],
        offset: u64,
        extents: &[FiemapExtent],
    ) -> io::Result<usize> {
        let length = buf.len() as u64;
        let end = offset + length;
        let mut bytes_read = 0usize;
        let mut current_offset = offset;

        for extent in extents {
            if current_offset >= end {
                break;
            }

            let extent_end = extent.logical + extent.length;

            // Handle hole before this extent
            if extent.logical > current_offset {
                let hole_end = extent.logical.min(end);
                let hole_len = (hole_end - current_offset) as usize;

                if !self.options.fill_holes {
                    // EOF at hole
                    return Ok(bytes_read);
                }

                // Fill with zeros
                let buf_start = bytes_read;
                let buf_end = buf_start + hole_len;
                buf[buf_start..buf_end].fill(0);
                bytes_read += hole_len;
                current_offset = hole_end;

                if current_offset >= end {
                    break;
                }
            }

            // Handle unwritten extent
            if extent.flags.is_unwritten() {
                if !self.options.fill_unwritten {
                    // EOF at unwritten
                    return Ok(bytes_read);
                }

                // Fill with zeros for unwritten extent
                let read_start = current_offset.max(extent.logical);
                let read_end = extent_end.min(end);
                let read_len = (read_end - read_start) as usize;

                let buf_start = bytes_read;
                let buf_end = buf_start + read_len;
                buf[buf_start..buf_end].fill(0);
                bytes_read += read_len;
                current_offset = read_end;
                continue;
            }

            // Handle hole-like extents (UNKNOWN, DELALLOC)
            if extent.flags.is_unknown() || extent.flags.is_delalloc() {
                let read_start = current_offset.max(extent.logical);
                let read_end = extent_end.min(end);
                let hole_len = (read_end - read_start) as usize;

                if !self.options.fill_holes {
                    return Ok(bytes_read);
                }

                let buf_start = bytes_read;
                let buf_end = buf_start + hole_len;
                buf[buf_start..buf_end].fill(0);
                bytes_read += hole_len;
                current_offset = read_end;
                continue;
            }

            // Normal extent - read from block device
            let read_start = current_offset.max(extent.logical);
            let read_end = extent_end.min(end);
            let read_len = (read_end - read_start) as usize;

            // Calculate physical offset
            let physical_offset = extent.physical + (read_start - extent.logical);

            // Read from device
            let buf_start = bytes_read;
            let buf_end = buf_start + read_len;
            let actual_read = device.read_at(&mut buf[buf_start..buf_end], physical_offset)?;

            bytes_read += actual_read;
            current_offset = read_start + actual_read as u64;

            if actual_read < read_len {
                // Short read
                break;
            }
        }

        // Handle trailing hole
        if current_offset < end && self.options.fill_holes {
            let remaining = (end - current_offset) as usize;
            let buf_start = bytes_read;
            let buf_end = buf_start + remaining;
            if buf_end <= buf.len() {
                buf[buf_start..buf_end].fill(0);
                bytes_read += remaining;
            }
        }

        Ok(bytes_read)
    }
}

/// Handle to a block device, either cached or uncached.
enum DeviceHandle {
    Cached(Arc<CachedDevice>),
    Uncached(CachedDevice),
}

impl DeviceHandle {
    /// Read data from the device at the specified physical offset.
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        let file = match self {
            DeviceHandle::Cached(cached) => &cached.file,
            DeviceHandle::Uncached(uncached) => &uncached.file,
        };

        let bytes = FileExt::read_at(file, buf, offset)?;
        Ok(bytes)
    }
}

// Implementation for Path
impl BlkReader for Path {
    fn blk_read_at_opt(&self, buf: &mut [u8], offset: u64, options: &Options) -> io::Result<State> {
        let file = File::open(self)?;
        let ctx = ReadContext::new(&file, Some(self), options);
        ctx.read_at(buf, offset)
    }
}

// Implementation for PathBuf
impl BlkReader for PathBuf {
    fn blk_read_at_opt(&self, buf: &mut [u8], offset: u64, options: &Options) -> io::Result<State> {
        self.as_path().blk_read_at_opt(buf, offset, options)
    }
}

// Implementation for File
impl BlkReader for File {
    fn blk_read_at_opt(&self, buf: &mut [u8], offset: u64, options: &Options) -> io::Result<State> {
        let ctx = ReadContext::new(self, None, options);
        ctx.read_at(buf, offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_options_builder() {
        let opts = Options::new()
            .with_cache(false)
            .with_fill_holes(true)
            .with_fill_unwritten(true)
            .with_allow_fallback(true);

        assert!(!opts.enable_cache);
        assert!(opts.fill_holes);
        assert!(opts.fill_unwritten);
        assert!(opts.allow_fallback);
    }

    #[test]
    fn test_can_use_fallback() {
        use blkmap::ExtentFlags;

        let file = File::open("/proc/self/exe").unwrap();
        let options = Options::new().with_allow_fallback(true);
        let ctx = ReadContext::new(&file, None, &options);

        // Empty extents - cannot fallback
        assert!(!ctx.can_use_fallback(&[], 0, 100));

        // Normal extent covering range - can fallback
        let extents = vec![FiemapExtent {
            logical: 0,
            physical: 1000,
            length: 4096,
            flags: ExtentFlags::empty(),
        }];
        assert!(ctx.can_use_fallback(&extents, 0, 100));

        // Unwritten extent - cannot fallback
        let extents = vec![FiemapExtent {
            logical: 0,
            physical: 1000,
            length: 4096,
            flags: ExtentFlags::UNWRITTEN,
        }];
        assert!(!ctx.can_use_fallback(&extents, 0, 100));

        // Hole at start - cannot fallback
        let extents = vec![FiemapExtent {
            logical: 100,
            physical: 1000,
            length: 4096,
            flags: ExtentFlags::empty(),
        }];
        assert!(!ctx.can_use_fallback(&extents, 0, 200));
    }
}
