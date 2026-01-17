//! CLI tool for reading file data directly from block devices.
//!
//! This tool uses the `blkreader` library to read file data directly from
//! the underlying block device using extent information.

use blkmap::Fiemap;
use blkpath::ResolveDevice;
use blkreader::{BlkReader, Options};
use clap::Parser;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;

/// Default chunk size for reading large files (1 MB).
const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024;

/// Read file data directly from block device using extent information.
///
/// This tool queries the file's extent information via FIEMAP and reads
/// data directly from the physical locations on the underlying block device.
#[derive(Parser, Debug)]
#[command(name = "blkreader")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the file to read
    path: PathBuf,

    /// Byte offset to start reading from
    #[arg(short, long, default_value = "0")]
    offset: u64,

    /// Number of bytes to read (default: entire file from offset)
    #[arg(short, long)]
    length: Option<u64>,

    /// Enable verbose output (show block device path, extent info, etc.)
    #[arg(short, long)]
    verbose: bool,

    /// Output file path (default: stdout)
    #[arg(short = 'O', long)]
    output: Option<PathBuf>,

    /// Fill holes with zeros instead of stopping
    #[arg(long)]
    fill_holes: bool,

    /// Fill unwritten extents with zeros instead of reading raw block data
    #[arg(long)]
    zero_unwritten: bool,

    /// Allow fallback to regular file I/O when safe
    #[arg(long)]
    allow_fallback: bool,

    /// Disable block device caching
    #[arg(long)]
    no_cache: bool,

    /// Alignment for direct IO.
    #[arg(long, default_value_t = 512)]
    alignment: u64,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = run(&args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

/// Allocate an aligned buffer for Direct I/O.
fn alloc_aligned_buffer(size: usize, align: usize) -> Vec<u8> {
    // Allocate with extra space for alignment
    let layout = std::alloc::Layout::from_size_align(size, align).unwrap();
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
    if ptr.is_null() {
        panic!("Failed to allocate aligned buffer");
    }
    unsafe { Vec::from_raw_parts(ptr, size, size) }
}

/// Align offset down to the alignment boundary.
fn align_down(offset: u64, alignment: u64) -> u64 {
    offset & !(alignment - 1)
}

/// Align length up to the alignment boundary.
fn align_up(length: u64, alignment: u64) -> u64 {
    (length + alignment - 1) & !(alignment - 1)
}

fn run(args: &Args) -> io::Result<()> {
    // Determine the length to read
    let file = File::open(&args.path)?;
    let file_size = file.metadata()?.len();

    let length = match args.length {
        Some(len) => len,
        None => file_size.saturating_sub(args.offset),
    };

    if length == 0 {
        if args.verbose {
            eprintln!("Nothing to read (length is 0)");
        }
        return Ok(());
    }

    // Request sudo privileges only if not using fallback mode
    // or if we need to access the block device directly
    if !args.allow_fallback {
        sudo::escalate_if_needed().map_err(|e| {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("Failed to escalate privileges: {}", e),
            )
        })?;
    }

    // Print verbose information
    if args.verbose {
        print_verbose_info(&args.path, args.offset, length, args.alignment)?;
    }

    // Build options
    let options = Options::new()
        .with_cache(!args.no_cache)
        .with_fill_holes(args.fill_holes)
        .with_zero_unwritten(args.zero_unwritten)
        .with_allow_fallback(args.allow_fallback);

    // Open output file or use stdout
    let mut output: Box<dyn Write> = if let Some(output_path) = &args.output {
        Box::new(File::create(output_path)?)
    } else {
        Box::new(io::stdout())
    };

    // Calculate aligned read parameters for Direct I/O
    let aligned_offset = align_down(args.offset, args.alignment);
    let offset_adjustment = (args.offset - aligned_offset) as usize;
    let total_length = align_up(length + offset_adjustment as u64, args.alignment);

    // Determine chunk size (aligned to ALIGNMENT)
    let chunk_size = DEFAULT_CHUNK_SIZE;

    // Allocate aligned buffer.
    let mut buf = alloc_aligned_buffer(chunk_size, args.alignment as usize);

    // Read in chunks to handle large files
    let mut total_bytes_read = 0usize;
    let mut current_aligned_offset = aligned_offset;
    let mut remaining = total_length;
    let mut first_chunk = true;
    let mut block_device_path = PathBuf::new();

    while remaining > 0 {
        let read_size = std::cmp::min(remaining as usize, chunk_size);
        let aligned_size = align_up(read_size as u64, args.alignment) as usize;

        // Perform the read
        let state = args.path.blk_read_at_opt(
            &mut buf[..aligned_size],
            current_aligned_offset,
            &options,
        )?;

        if first_chunk {
            block_device_path = state.block_device_path.clone();
            first_chunk = false;
        }

        if state.bytes_read == 0 {
            break;
        }

        // Calculate the actual data to output from this chunk
        let skip = if current_aligned_offset == aligned_offset {
            offset_adjustment
        } else {
            0
        };

        let bytes_to_write = std::cmp::min(
            state.bytes_read.saturating_sub(skip),
            (length as usize).saturating_sub(total_bytes_read),
        );

        if bytes_to_write > 0 {
            output.write_all(&buf[skip..skip + bytes_to_write])?;
            total_bytes_read += bytes_to_write;
        }

        // Check if we've read enough
        if total_bytes_read >= length as usize {
            break;
        }

        // Short read indicates EOF
        if state.bytes_read < read_size {
            break;
        }

        current_aligned_offset += read_size as u64;
        remaining -= read_size as u64;
    }

    if args.verbose {
        eprintln!();
        eprintln!("Read {} bytes", total_bytes_read);
        if !block_device_path.as_os_str().is_empty() {
            eprintln!("Block device: {}", block_device_path.display());
        }
        if let Some(output_path) = &args.output {
            eprintln!("Output written to: {}", output_path.display());
        }
    }

    Ok(())
}

fn print_verbose_info(path: &PathBuf, offset: u64, length: u64, alignment: u64) -> io::Result<()> {
    eprintln!("File: {}", path.display());
    eprintln!("Offset: {} (0x{:x})", offset, offset);
    eprintln!("Length: {} (0x{:x})", length, length);

    // Show alignment info
    let aligned_offset = align_down(offset, alignment);
    let aligned_length = align_up(length + (offset - aligned_offset), alignment);
    if aligned_offset != offset || aligned_length != length {
        eprintln!(
            "Aligned offset: {} (0x{:x}), Aligned length: {} (0x{:x})",
            aligned_offset, aligned_offset, aligned_length, aligned_length
        );
    }

    // Resolve block device
    match path.resolve_device() {
        Ok(device) => {
            eprintln!("Block device: {}", device.display());
        }
        Err(e) => {
            eprintln!("Block device: (unable to resolve: {})", e);
        }
    }

    // Query extents
    let file = File::open(path)?;
    match file.fiemap_range(offset, length) {
        Ok(extents) => {
            eprintln!();
            eprintln!("Extents for range [{}, {}):", offset, offset + length);
            eprintln!(
                "{:<6} {:<20} {:<20} {:<20} Flags",
                "Index", "Logical", "Physical", "Length"
            );
            eprintln!("{}", "-".repeat(80));

            for (i, extent) in extents.iter().enumerate() {
                eprintln!(
                    "{:<6} 0x{:016x} 0x{:016x} 0x{:016x} {:?}",
                    i, extent.logical, extent.physical, extent.length, extent.flags
                );
            }
            eprintln!("{}", "-".repeat(80));
            eprintln!("Total: {} extent(s)", extents.len());
        }
        Err(e) => {
            eprintln!("Extents: (unable to query: {})", e);
        }
    }

    Ok(())
}
