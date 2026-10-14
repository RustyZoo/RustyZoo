// SPDX-License-Identifier: AGPL-3.0-or-later
// Part of NihAV by Kostya Shishkov (https://nihav.org), trimmed; see vendor/README.md and COPYING.
//! Crate for providing support for Intel multimedia formats.
#![allow(dead_code)]
extern crate nihav_core;
extern crate nihav_codec_support;

#[allow(clippy::upper_case_acronyms)]
mod codecs;

pub use crate::codecs::indeo5_decoder;
