// SPDX-License-Identifier: AGPL-3.0-only
// Part of NihAV by Kostya Shishkov (https://nihav.org), trimmed; see vendor/README.md and COPYING.
use nihav_core::codecs::*;

// RustyZoo change: no message on standard output when a check fails (upstream prints one in
// debug builds).
macro_rules! validate {
    ($a:expr) => { if !$a { return Err(DecoderError::InvalidData); } };
}

mod indeo5;

#[allow(clippy::too_many_arguments)]
mod ivi;
#[allow(clippy::too_many_arguments)]
mod ivibr;
#[allow(clippy::erasing_op)]
#[allow(clippy::identity_op)]
#[allow(clippy::too_many_arguments)]
mod ividsp;

/// Creates an Indeo 5 (IV50) decoder.
pub fn indeo5_decoder() -> Box<dyn NADecoder + Send> {
    indeo5::get_decoder()
}
