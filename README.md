# blkreader

[![CI](https://github.com/SF-Zhou/blkreader/actions/workflows/ci.yml/badge.svg)](https://github.com/SF-Zhou/blkreader/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/blkreader.svg)](https://crates.io/crates/blkreader)
[![Documentation](https://docs.rs/blkreader/badge.svg)](https://docs.rs/blkreader)
[![License](https://img.shields.io/crates/l/blkreader.svg)](https://github.com/SF-Zhou/blkreader#license)

Read file data directly from block device using extent information.

## Overview

`blkreader` provides a mechanism to read file data directly from the underlying block device by querying the file's extent information via the Linux `FIEMAP` ioctl. This is particularly useful in scenarios where:

- Storage space has been pre-allocated using `fallocate` + `fdatasync`
- Extent information has been persisted to disk
- The file's data may not have been fully synced (written extent state not persisted)
- You need to recover raw data from the block device

### Use Case

Consider an I/O pattern where:

1. Before each write, you use `fallocate` + `fdatasync` to pre-allocate a complete storage extent
2. The extent information has been confirmed persisted to disk
3. Subsequent Direct I/O writes fall within these extents
4. However, the written extent state may not have been persisted before a crash

In this case, while the file metadata might not reflect the written data, the raw data definitely exists on the block device. If you maintain the written length in a reliable location, you can use `blkreader` to recover the raw data directly from the block device.

## Features

- Query file extent information using `FIEMAP` ioctl via [`blkmap`](https://crates.io/crates/blkmap)
- Resolve block device paths using [`blkpath`](https://crates.io/crates/blkpath)
- Read data directly from block devices using Direct I/O
- Global block device cache for improved performance
- Configurable handling of holes and unwritten extents
- Fallback to regular file I/O when safe (no root required)

## Installation

Add `blkreader` to your `Cargo.toml`:

```toml
[dependencies]
blkreader = "0.1"
```

Or install the CLI tool:

```bash
cargo install blkreader
```

## Library Usage

### Simple Read

```rust
use blkreader::BlkReader;
use std::path::Path;

fn main() -> std::io::Result<()> {
    let path = Path::new("/path/to/file");
    let mut buf = vec![0u8; 4096];

    // Read 4096 bytes from offset 0
    let bytes_read = path.blk_read_at(&mut buf, 0)?;
    println!("Read {} bytes", bytes_read);

    Ok(())
}
```

### Read with Options

```rust
use blkreader::{BlkReader, Options};
use std::path::Path;

fn main() -> std::io::Result<()> {
    let path = Path::new("/path/to/file");
    let mut buf = vec![0u8; 4096];

    // Configure read options
    let options = Options::new()
        .with_cache(true)           // Enable block device caching (default)
        .with_fill_holes(true)      // Fill holes with zeros
        .with_zero_unwritten(true)  // Fill unwritten extents with zeros
        .with_allow_fallback(true); // Allow fallback to regular file I/O

    // Read with detailed state information
    let state = path.blk_read_at_opt(&mut buf, 0, &options)?;

    println!("Read {} bytes", state.bytes_read);
    println!("Block device: {}", state.block_device_path.display());
    println!("Extents: {:?}", state.extents);
    println!("Used fallback: {}", state.used_fallback);

    Ok(())
}
```

### Read from File Handle

```rust
use blkreader::BlkReader;
use std::fs::File;

fn main() -> std::io::Result<()> {
    let file = File::open("/path/to/file")?;
    let mut buf = vec![0u8; 4096];

    let bytes_read = file.blk_read_at(&mut buf, 0)?;
    println!("Read {} bytes", bytes_read);

    Ok(())
}
```

## CLI Usage

```bash
# Basic usage - read entire file
blkreader /path/to/file

# Read from specific offset
blkreader /path/to/file --offset 1024

# Read specific length
blkreader /path/to/file --offset 0 --length 4096

# Verbose output (show extents and block device info)
blkreader /path/to/file -v

# Write output to file
blkreader /path/to/file -O output.bin

# Fill holes and unwritten extents with zeros
blkreader /path/to/file --fill-holes --zero-unwritten

# Allow fallback to regular file I/O when safe
blkreader /path/to/file --allow-fallback
```

### CLI Options

| Option | Description |
|--------|-------------|
| `-o, --offset <OFFSET>` | Byte offset to start reading from (default: 0) |
| `-l, --length <LENGTH>` | Number of bytes to read (default: entire file) |
| `-v, --verbose` | Enable verbose output |
| `-O, --output <FILE>` | Write output to file instead of stdout |
| `--fill-holes` | Fill holes with zeros instead of stopping |
| `--zero-unwritten` | Fill unwritten extents with zeros instead of reading raw block data |
| `--allow-fallback` | Allow fallback to regular file I/O when safe |
| `--no-cache` | Disable block device caching |
| `--dry-run` | Skip actual device reads (for testing extent mapping) |

## Options

### `enable_cache` (default: `true`)

When enabled, block device file handles are cached globally based on the device ID. This improves performance for repeated reads from files on the same filesystem.

### `fill_holes` (default: `false`)

When enabled, holes in file extents are filled with zeros. When disabled, reading a hole causes an early EOF return.

### `zero_unwritten` (default: `false`)

When enabled, unwritten (preallocated but not yet written) extents are filled with zeros, matching normal filesystem read behavior.

When disabled (default), unwritten extents are read directly from the block device, returning whatever raw data exists at those physical locations. This is useful for data recovery scenarios where you want to access the actual data written to pre-allocated extents.

### `allow_fallback` (default: `false`)

When enabled, if the queried extents fully cover the read range and contain no unwritten extents, the read will be performed using regular file I/O instead of direct block device I/O. This avoids the need for root privileges in such cases.

### `dry_run` (default: `false`)

When enabled, no actual I/O operations are performed on block devices or files. Instead, the operation pretends to successfully read the requested amount of data. This is useful for:

- Testing extent mapping logic without performing time-consuming I/O operations
- Validating that a file's extents are accessible
- Debugging and development without needing root privileges

The extent information is still queried via FIEMAP to ensure the file structure is valid, but the actual data reading step is skipped.

## Direct I/O Alignment Requirements

When using the library API to read directly from block devices (not using fallback mode), the following alignment requirements must be met:

- **Buffer alignment**: The buffer should be aligned to at least 512 bytes (sector size). For optimal performance, 4096-byte alignment is recommended.
- **Offset alignment**: The read offset should be aligned to 512 bytes.
- **Length alignment**: The buffer length should be aligned to 512 bytes.

If alignment requirements are not met, the read operation may fail with an `EINVAL` error.

**Note**: The CLI tool handles alignment automatically by adjusting offsets and using aligned buffers internally.

## Requirements

- Linux operating system
- Root privileges (for direct block device access, unless using fallback mode)
- Access to `/sys/dev/block/` or `/proc/self/mountinfo` (for block device resolution)

## Platform Support

This crate only works on Linux systems. It has been tested on:

- x86_64 (Intel/AMD)
- aarch64 (ARM64)

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
