// SPDX-License-Identifier: AGPL-3.0-or-later
// Part of NihAV by Kostya Shishkov (https://nihav.org), trimmed; see vendor/README.md and COPYING.
//! Codebook support for bitstream reader.
//!
//! Codebook is a set of unique bit strings and values assigned to them.
//! Since there are many ways to define codebook, this implementation employs [`CodebookDescReader`] trait to provide codebook generator with the codes.
//! Please also pay attention to the codebook creation mode: if bitstream reader reads bits starting from most significant bit first then you should use [`MSB`] mode and [`LSB`] mode otherwise.
//!
//! # Examples
//!
//! Create a codebook from arrays with codeword descriptions:
//! ```
//! use nihav_core::io::codebook::{ShortCodebookDesc, ShortCodebookDescReader, Codebook, CodebookMode};
//!
//! let cb_desc: &[ShortCodebookDesc] = &[
//!             ShortCodebookDesc { code: 0b00,   bits: 2 },
//!             ShortCodebookDesc { code: 0,      bits: 0 },
//!             ShortCodebookDesc { code: 0b01,   bits: 2 },
//!             ShortCodebookDesc { code: 0b1,    bits: 1 }];
//! let mut cr = ShortCodebookDescReader::new(cb_desc);
//! let cb = Codebook::new(&mut cr, CodebookMode::MSB).unwrap();
//! ```
//!
//! Create a codebook using more flexible [`TableCodebookDescReader`] approach.
//! This will create a codebook for the following set: `1` -> -2, `01` -> -1, `001` -> 0, `0001` -> 1, `00001` -> 2.
//! ```
//! use nihav_core::io::codebook::{TableCodebookDescReader, Codebook, CodebookMode};
//!
//! fn map_cb_index(index: usize) -> i16 { (index as i16) - 2 }
//! const CB_BITS:  [u8; 5] = [ 1, 2, 3, 4, 5 ];
//! const CB_CODES: [u8; 5] = [ 1, 1, 1, 1, 1 ];
//!
//! let mut tcr = TableCodebookDescReader::new(&CB_CODES, &CB_BITS, map_cb_index);
//! let cb = Codebook::new(&mut tcr, CodebookMode::MSB).unwrap();
//! ```
//!
//! Read value using a codebook:
//! ```no_run
//! use nihav_core::io::bitreader::BitReader;
//! use nihav_core::io::codebook::{Codebook, CodebookReader, CodebookMode};
//! # use nihav_core::io::codebook::{ShortCodebookDesc, ShortCodebookDescReader, CodebookDescReader, CodebookResult};
//!
//! # fn foo(br: &mut BitReader) -> CodebookResult<()> {
//! # let mut cr = ShortCodebookDescReader::new(&[ShortCodebookDesc { code: 0b00,   bits: 2 }]);
//! let cb = Codebook::new(&mut cr, CodebookMode::MSB).unwrap();
//! let value = br.read_cb(&cb)?;
//! # Ok(())
//! # }
//! ```
//!
//! [`MSB`]: ./enum.CodebookMode.html#variant.MSB
//! [`LSB`]: ./enum.CodebookMode.html#variant.LSB
//! [`CodebookDescReader`]: ./trait.CodebookDescReader.html
//! [`TableCodebookDescReader`]: ./struct.TableCodebookDescReader.html


/// A list specifying general codebook operations errors.
#[derive(Debug)]
pub enum CodebookError {
    /// Codebook description contains errors.
    InvalidCodebook,
    /// Could not allocate memory for codebook.
    MemoryError,
    /// Bitstream contains a sequence not present in codebook.
    InvalidCode,
}


#[cfg(test)]
mod test {
    use super::*;
    use crate::io::bitreader::*;

}
