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

    /// Fill unwritten extents with zeros.
    ///
    /// When disabled, reading an unwritten extent will cause an early EOF return.
    pub fill_unwritten: bool,

    /// Allow fallback to regular file read when safe.
    ///
    /// When enabled, if the queried extents fully cover the read range
    /// and contain no unwritten extents, the read will be performed
    /// using regular file I/O instead of direct block device I/O.
    /// This avoids the need for root privileges in such cases.
    pub allow_fallback: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            enable_cache: true,
            fill_holes: false,
            fill_unwritten: false,
            allow_fallback: false,
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
    pub fn with_fill_unwritten(mut self, fill: bool) -> Self {
        self.fill_unwritten = fill;
        self
    }

    /// Enable or disable fallback to regular file read.
    pub fn with_allow_fallback(mut self, allow: bool) -> Self {
        self.allow_fallback = allow;
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
        assert!(!opts.fill_unwritten);
        assert!(!opts.allow_fallback);
    }

    #[test]
    fn test_builder_pattern() {
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
}
