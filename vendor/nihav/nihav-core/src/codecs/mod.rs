// SPDX-License-Identifier: AGPL-3.0-or-later
// Part of NihAV by Kostya Shishkov (https://nihav.org), trimmed; see vendor/README.md and COPYING.
//! Decoder interface definitions.
pub use crate::frame::*;
use crate::io::bitreader::BitReaderError;
use crate::io::codebook::CodebookError;
pub use crate::options::*;
pub use std::str::FromStr;

/// A list specifying general decoding errors.
#[derive(Debug,Clone,Copy,PartialEq)]
pub enum DecoderError {
    /// No frame was provided.
    NoFrame,
    /// Allocation failed.
    AllocError,
    /// Operation requires repeating.
    TryAgain,
    /// Invalid input data was provided.
    InvalidData,
    /// Checksum verification failed.
    ChecksumError,
    /// Provided input turned out to be incomplete.
    ShortData,
    /// Decoder could not decode provided frame because it references some missing previous frame.
    MissingReference,
    /// Feature is not implemented.
    NotImplemented,
    /// Some bug in decoder. It should not happen yet it might.
    Bug,
}

/// A specialised `Result` type for decoding operations.
pub type DecoderResult<T> = Result<T, DecoderError>;

impl From<BitReaderError> for DecoderError {
    fn from(e: BitReaderError) -> Self {
        match e {
            BitReaderError::BitstreamEnd => DecoderError::ShortData,
            _ => DecoderError::InvalidData,
        }
    }
}

impl From<CodebookError> for DecoderError {
    fn from(_: CodebookError) -> Self { DecoderError::InvalidData }
}

impl From<AllocatorError> for DecoderError {
    fn from(_: AllocatorError) -> Self { DecoderError::AllocError }
}

/// Auxiliary structure for storing data used by decoder but also controlled by the caller.
///
/// RustyZoo change: upstream holds frame buffer pools here; the Indeo 5 decoder uses none.
pub struct NADecoderSupport {}

impl NADecoderSupport {
    /// Constructs a new instance of `NADecoderSupport`.
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for NADecoderSupport {
    fn default() -> Self { Self::new() }
}

/// Decoder trait.
pub trait NADecoder: NAOptionHandler {
    /// Initialises the decoder.
    ///
    /// It takes [`NADecoderSupport`] allocated by the caller and `NACodecInfoRef` provided by demuxer.
    ///
    /// [`NADecoderSupport`]: ./struct.NADecoderSupport.html
    fn init(&mut self, supp: &mut NADecoderSupport, info: NACodecInfoRef) -> DecoderResult<()>;
    /// Decodes a single frame.
    fn decode(&mut self, supp: &mut NADecoderSupport, pkt: &NAPacket) -> DecoderResult<NAFrameRef>;
    /// Tells decoder to clear internal state (e.g. after error or seeking).
    fn flush(&mut self);
}
