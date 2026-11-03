//! The hash the units' state is summed with.

/// The 64-bit FNV-1a hash of what the units hold, for the determinism checks.
pub(super) struct Fnv(pub(super) u64);

impl Fnv {
    pub(super) fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        self.bytes_end();
    }

    /// A separator, so that two names do not run into each other.
    pub(super) fn bytes_end(&mut self) {
        self.0 = (self.0 ^ 0xff).wrapping_mul(0x0000_0100_0000_01b3);
    }

    pub(super) fn word(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}
