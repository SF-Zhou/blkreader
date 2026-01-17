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

    /// Fill unwritten extents with zeros instead of stopping
    #[arg(long)]
    fill_unwritten: bool,

    /// Allow fallback to regular file I/O when safe
    #[arg(long)]
    allow_fallback: bool,

    /// Disable block device caching
    #[arg(long)]
    no_cache: bool,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = run(&args) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
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
        print_verbose_info(&args.path, args.offset, length)?;
    }

    // Prepare buffer
    let mut buf = vec![0u8; length as usize];

    // Build options
    let options = Options::new()
        .with_cache(!args.no_cache)
        .with_fill_holes(args.fill_holes)
        .with_fill_unwritten(args.fill_unwritten)
        .with_allow_fallback(args.allow_fallback);

    // Perform the read
    let state = args.path.blk_read_at_opt(&mut buf, args.offset, &options)?;

    if args.verbose {
        eprintln!();
        eprintln!("Read {} bytes", state.bytes_read);
        if state.used_fallback {
            eprintln!("(Used fallback to regular file I/O)");
        } else {
            eprintln!("Block device: {}", state.block_device_path.display());
        }
    }

    // Truncate buffer to actual bytes read
    buf.truncate(state.bytes_read);

    // Write output
    if let Some(output_path) = &args.output {
        let mut output_file = File::create(output_path)?;
        output_file.write_all(&buf)?;
        if args.verbose {
            eprintln!("Output written to: {}", output_path.display());
        }
    } else {
        let stdout = io::stdout();
        let mut handle = stdout.lock();
        handle.write_all(&buf)?;
    }

    Ok(())
}

fn print_verbose_info(path: &PathBuf, offset: u64, length: u64) -> io::Result<()> {
    eprintln!("File: {}", path.display());
    eprintln!("Offset: {} (0x{:x})", offset, offset);
    eprintln!("Length: {} (0x{:x})", length, length);

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
