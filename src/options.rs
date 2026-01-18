//! Configuration options for blkreader operations.

/// Options for controlling the read behavior.
#[derive(Debug, Clone)]
pub struct Options {
    /// Enable global block device cache.
    ///
    /// When enabled, block device file handles are cached globally
    /// based on the device ID, improving performance for repeated reads
    /// from files on the same filesystem.
    pub enable_cache: bool,

    /// Fill holes in file extents with zeros.
    ///
    /// When disabled, reading a hole will cause an early EOF return.
    pub fill_holes: bool,

    /// Fill unwritten extents with zeros instead of reading raw data.
    ///
    /// When disabled (default), unwritten extents are read from the block
    /// device, returning whatever raw data exists at those physical locations.
    /// This is useful for data recovery scenarios.
    ///
    /// When enabled, unwritten extents are filled with zeros (matching
    /// normal filesystem read behavior).
    pub zero_unwritten: bool,

    /// Allow fallback to regular file read when safe.
    ///
    /// When enabled, if the queried extents fully cover the read range
    /// and contain no unwritten extents, the read will be performed
    /// using regular file I/O instead of direct block device I/O.
    /// This avoids the need for root privileges in such cases.
    pub allow_fallback: bool,

    /// Require reading the exact requested length.
    ///
    /// When enabled, the read operation will return an error if the amount
    /// of data read is less than the requested buffer size.
    /// This is similar to the behavior of [`std::io::Read::read_exact`].
    ///
    /// When disabled, partial reads are allowed and the actual number of
    /// bytes read is returned (similar to [`std::io::Read::read`]).
    pub read_exact: bool,

    /// Dry run mode - skip actual device reads.
    ///
    /// When enabled, no actual I/O operations are performed on block devices
    /// or files. Instead, the operation pretends to successfully read the
    /// requested amount of data.
    ///
    /// This is useful for testing the extent mapping logic and validating
    /// that a file's extents are accessible without performing time-consuming
    /// I/O operations.
    ///
    /// When disabled (default), normal read operations are performed.
    pub dry_run: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enable_cache: true,
            fill_holes: false,
            zero_unwritten: false,
            allow_fallback: false,
            read_exact: false,
            dry_run: false,
        }
    }
}

impl Options {
    /// Create a new Options with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable or disable the global block device cache.
    pub fn with_cache(mut self, enable: bool) -> Self {
        self.enable_cache = enable;
        self
    }

    /// Enable or disable filling holes with zeros.
    pub fn with_fill_holes(mut self, fill: bool) -> Self {
        self.fill_holes = fill;
        self
    }

    /// Enable or disable filling unwritten extents with zeros.
    ///
    /// When disabled (default), unwritten extents are read from the block
    /// device, returning raw data. When enabled, they are filled with zeros.
    pub fn with_zero_unwritten(mut self, zero: bool) -> Self {
        self.zero_unwritten = zero;
        self
    }

    /// Enable or disable fallback to regular file read.
    pub fn with_allow_fallback(mut self, allow: bool) -> Self {
        self.allow_fallback = allow;
        self
    }

    /// Enable or disable requiring exact read length.
    ///
    /// When enabled, the read will fail if less than the requested number of
    /// bytes are read. When disabled, partial reads are allowed.
    pub fn with_read_exact(mut self, exact: bool) -> Self {
        self.read_exact = exact;
        self
    }

    /// Enable or disable dry run mode.
    ///
    /// When enabled, no actual I/O operations are performed. Instead, the
    /// operation pretends to successfully read the requested amount of data.
    /// This is useful for testing extent mapping logic without performing
    /// time-consuming I/O operations.
    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_options() {
        let opts = Options::default();
        assert!(opts.enable_cache);
        assert!(!opts.fill_holes);
        assert!(!opts.zero_unwritten);
        assert!(!opts.allow_fallback);
        assert!(!opts.read_exact);
        assert!(!opts.dry_run);
    }

    #[test]
    fn test_builder_pattern() {
        let opts = Options::new()
            .with_cache(false)
            .with_fill_holes(true)
            .with_zero_unwritten(true)
            .with_allow_fallback(true)
            .with_read_exact(true)
            .with_dry_run(true);

        assert!(!opts.enable_cache);
        assert!(opts.fill_holes);
        assert!(opts.zero_unwritten);
        assert!(opts.allow_fallback);
        assert!(opts.read_exact);
        assert!(opts.dry_run);
    }
}
