//! Exit codes, fixed by the CLI contract. Scripts branch on these, so the
//! numbers are part of the interface, not an implementation detail.

/// The command did what was asked.
pub const SUCCESS: u8 = 0;
/// The provider or the generation failed.
pub const FAILURE: u8 = 1;
/// The command line was wrong; nothing was attempted.
pub const USAGE: u8 = 2;
/// Plainly is not configured: no key where one is needed, or no usable provider.
pub const NOT_CONFIGURED: u8 = 3;
/// Plainly refused the input on purpose: the clipboard is marked sensitive, or
/// its marker could not be read.
pub const REFUSED: u8 = 4;

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers are the contract scripts branch on; pin them.
    #[test]
    fn the_exit_codes_are_the_documented_ones() {
        assert_eq!(
            [SUCCESS, FAILURE, USAGE, NOT_CONFIGURED, REFUSED],
            [0, 1, 2, 3, 4]
        );
    }
}
