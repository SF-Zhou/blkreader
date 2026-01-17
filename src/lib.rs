//! # blkreader
//!
//! A Rust crate for reading file data directly from block devices using extent information.
//!
//! ## Overview
//!
//! `blkreader` provides a mechanism to read file data directly from the underlying block device
//! by querying the file's extent information via the Linux `FIEMAP` ioctl. This is particularly
//! useful in scenarios where:
//!
//! - Storage space has been pre-allocated using `fallocate` + `fdatasync`
//! - Extent information has been persisted to disk
//! - The file's data may not have been fully synced (written extent state not persisted)
//! - You need to recover raw data from the block device
//!
//! ## Features
//!
//! - Query file extent information using `FIEMAP` ioctl via [`blkmap`]
//! - Resolve block device paths using [`blkpath`]
//! - Read data directly from block devices using Direct I/O
//! - Global block device cache for improved performance
//! - Configurable handling of holes and unwritten extents
//! - Fallback to regular file I/O when safe
//!
//! ## Direct I/O Alignment Requirements
//!
//! When reading directly from block devices (not using fallback mode), the following
//! alignment requirements must be met for Direct I/O:
//!
//! - **Buffer alignment**: The buffer must be aligned to at least 512 bytes (sector size).
//!   For optimal performance on modern devices, 4096-byte alignment is recommended.
//! - **Offset alignment**: The read offset should be aligned to 512 bytes.
//! - **Length alignment**: The read length should be aligned to 512 bytes.
//!
//! If alignment requirements are not met, the underlying read may fail with an
//! `EINVAL` error. The CLI tool handles alignment automatically.
//!
//! ## Example
//!
//! ```no_run
//! use blkreader::{BlkReader, Options};
//! use std::path::Path;
//!
//! let path = Path::new("/path/to/file");
//! // Buffer should be aligned; using 4096 bytes which is a common block size
//! let mut buf = vec![0u8; 4096];
//!
//! // Simple read (offset 0 is aligned)
//! let bytes_read = path.blk_read_at(&mut buf, 0).unwrap();
//!
//! // Read with options
//! let options = Options::default();
//! let state = path.blk_read_at_opt(&mut buf, 0, &options).unwrap();
//! println!("Read {} bytes from {}", state.bytes_read, state.block_device_path.display());
//! ```
//!
//! ## Safety
//!
//! This crate requires root privileges to read from block devices. The CLI tool
//! automatically requests sudo permissions when needed.

mod cache;
mod options;
mod reader;
mod state;

pub use blkmap::FiemapExtent as Extent;
pub use options::Options;
pub use reader::BlkReader;
pub use state::State;
