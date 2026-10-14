// SPDX-License-Identifier: AGPL-3.0-or-later
// Part of NihAV by Kostya Shishkov (https://nihav.org), trimmed; see vendor/README.md and COPYING.
//! Core functionality of NihAV intended to be used by both crates implementing format support and users.
// RustyZoo change: some fields of the kept structures are only written (upstream API); no warning.
#![allow(dead_code)]
#[allow(clippy::upper_case_acronyms)]
#[allow(clippy::too_many_arguments)]
pub mod codecs;

#[allow(clippy::too_many_arguments)]
pub mod formats;
pub mod frame;
#[allow(clippy::too_many_arguments)]
pub mod io;
pub mod options;
pub mod refs;
