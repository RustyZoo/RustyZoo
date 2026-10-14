//! The intro movies: AVI files with Indeo 5 (`IV50`) video and PCM audio.
//!
//! [`Movie::parse`] reads the container (the streams are found from the headers, the data chunks
//! of the `movi` list by scanning it, without the index) and [`Movie::frames`] decodes the video
//! into RGBA pictures. The audio is the raw PCM of the file, for the caller to play. Everything is
//! untrusted: malformed data gives an error, never a panic.

use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};

use nihav_core::codecs::{NABufferType, NACodecInfo, NACodecTypeInfo, NADecoder, NADecoderSupport};
use nihav_core::codecs::{NAVideoBufferRef, StreamType};
use nihav_core::formats::YUV410_FORMAT;
use nihav_core::frame::{NAPacket, NAStream, NATimeInfo, NAVideoInfo};

use crate::Framebuffer;

/// The largest picture size the container may announce.
const MAX_SIDE: u32 = 4096;
/// The tags of the chunks of a `LIST` that the reader enters.
const LISTS: [&[u8; 4]; 3] = [b"hdrl", b"strl", b"movi"];

/// Why a movie cannot be read or played.
#[derive(Debug, PartialEq)]
pub enum MovieError {
    /// The file is not a well-formed AVI file.
    Malformed(&'static str),
    /// The file is well formed but uses something the engine does not play.
    Unsupported(String),
    /// The video decoder rejected a frame.
    Decode(String),
}

impl fmt::Display for MovieError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Malformed(what) => write!(f, "malformed AVI file: {what}"),
            Self::Unsupported(what) => write!(f, "unsupported movie: {what}"),
            Self::Decode(what) => write!(f, "cannot decode a video frame: {what}"),
        }
    }
}

impl Error for MovieError {}

/// The format of the PCM audio of a movie.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioFormat {
    pub channels: u16,
    /// Samples per second and channel.
    pub rate: u32,
    /// Bits per sample: 8 (unsigned) or 16 (signed, little-endian).
    pub bits: u16,
}

/// What the headers of a stream say.
struct Stream {
    kind: [u8; 4],
    scale: u32,
    rate: u32,
    format: Vec<u8>,
}

/// A parsed movie: the file and where its video frames and audio chunks are.
pub struct Movie {
    data: Vec<u8>,
    width: u32,
    height: u32,
    scale: u32,
    rate: u32,
    video: Vec<Range<usize>>,
    audio_format: Option<AudioFormat>,
    audio: Vec<Range<usize>>,
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The chunks of `data[range]` as (tag, body range): a chunk is a tag, a little-endian size and
/// the body, padded to an even size. A body cut short by the end of the range is cut there.
fn chunks(data: &[u8], range: Range<usize>) -> impl Iterator<Item = ([u8; 4], Range<usize>)> + '_ {
    let mut at = range.start;
    let end = range.end.min(data.len());
    std::iter::from_fn(move || {
        if at >= end {
            return None;
        }
        let tag: [u8; 4] = data.get(at..)?.get(..4)?.try_into().ok()?;
        let size = u32_at(data, at + 4)? as usize;
        let body = at + 8..(at + 8).saturating_add(size).min(end);
        if body.start > end {
            return None;
        }
        at = body.start.saturating_add(size).saturating_add(size & 1);
        Some((tag, body))
    })
}

impl Movie {
    /// Reads the container of an AVI file: one Indeo 5 video stream and at most one PCM audio
    /// stream are played, other streams are ignored.
    pub fn parse(data: Vec<u8>) -> Result<Movie, MovieError> {
        if data.get(..4) != Some(b"RIFF") || data.get(8..12) != Some(b"AVI ") {
            return Err(MovieError::Malformed("not a RIFF AVI file"));
        }
        let mut streams = Vec::new();
        let mut video = Vec::new();
        let mut audio = Vec::new();
        let mut movi: Vec<Range<usize>> = Vec::new();
        // The lists are entered breadth first.
        let mut todo = VecDeque::new();
        todo.push_back(12..data.len());
        while let Some(range) = todo.pop_front() {
            for (tag, body) in chunks(&data, range) {
                let list = data.get(body.start..body.start + 4);
                if &tag == b"LIST" && list.is_some_and(|l| LISTS.iter().any(|t| *t == l)) {
                    if list == Some(b"movi") {
                        movi.push(body.start + 4..body.end);
                    } else {
                        todo.push_back(body.start + 4..body.end);
                    }
                } else if &tag == b"strh" {
                    let h = &data[body];
                    let kind = h
                        .get(..4)
                        .and_then(|k| k.try_into().ok())
                        .unwrap_or_default();
                    let (scale, rate) = (u32_at(h, 20).unwrap_or(0), u32_at(h, 24).unwrap_or(0));
                    streams.push(Stream {
                        kind,
                        scale,
                        rate,
                        format: Vec::new(),
                    });
                } else if &tag == b"strf"
                    && let Some(stream) = streams.last_mut()
                {
                    stream.format = data[body].to_vec();
                }
            }
        }
        let video_stream = streams.iter().position(|s| &s.kind == b"vids");
        let audio_stream = streams.iter().position(|s| &s.kind == b"auds");
        let Some(video_stream) = video_stream else {
            return Err(MovieError::Malformed("no video stream"));
        };
        // Data chunks sit in the `movi` list, possibly grouped in `rec ` lists inside it.
        let mut data_chunks = Vec::new();
        for range in movi {
            let mut lists = vec![chunks(&data, range)];
            while let Some(list) = lists.last_mut() {
                let Some((tag, body)) = list.next() else {
                    lists.pop();
                    continue;
                };
                if &tag == b"LIST" && data.get(body.start..body.start + 4) == Some(b"rec ") {
                    if lists.len() < 3 {
                        lists.push(chunks(&data, body.start + 4..body.end));
                    }
                } else {
                    data_chunks.push((tag, body));
                }
            }
        }
        for (tag, body) in data_chunks {
            let number = std::str::from_utf8(&tag[..2])
                .ok()
                .and_then(|n| n.parse::<usize>().ok());
            match (number, &tag[2..]) {
                (Some(n), b"dc" | b"db") if n == video_stream => video.push(body),
                (Some(n), b"wb") if Some(n) == audio_stream => audio.push(body),
                _ => {}
            }
        }

        let v = &streams[video_stream];
        let (width, height) = (u32_at(&v.format, 4), u32_at(&v.format, 8));
        let (Some(width), Some(height)) = (width, height) else {
            return Err(MovieError::Malformed("video format too short"));
        };
        let height = (height as i32).unsigned_abs();
        if width == 0 || width > MAX_SIDE || height == 0 || height > MAX_SIDE {
            return Err(MovieError::Malformed("picture size out of range"));
        }
        if v.format.get(16..20) != Some(b"IV50") {
            return Err(MovieError::Unsupported(
                "the video is not Indeo 5 (IV50)".into(),
            ));
        }
        if v.scale == 0 || v.rate == 0 {
            return Err(MovieError::Malformed("frame rate is zero"));
        }
        let audio_format = match audio_stream.map(|n| &streams[n].format) {
            None => None,
            Some(f) => {
                let format = AudioFormat {
                    channels: u16_at(f, 2).unwrap_or(0),
                    rate: u32_at(f, 4).unwrap_or(0),
                    bits: u16_at(f, 14).unwrap_or(0),
                };
                if u16_at(f, 0) != Some(1)
                    || !(1..=8).contains(&format.channels)
                    || format.rate == 0
                    || ![8, 16].contains(&format.bits)
                {
                    return Err(MovieError::Unsupported(
                        "the audio is not 8 or 16 bit PCM".into(),
                    ));
                }
                Some(format)
            }
        };
        Ok(Movie {
            width,
            height,
            scale: v.scale,
            rate: v.rate,
            video,
            audio_format,
            audio,
            data,
        })
    }

    /// The picture size the container announces.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The number of video frames.
    pub fn frame_count(&self) -> usize {
        self.video.len()
    }

    /// The frame rate as a fraction (frames, seconds).
    pub fn frame_rate(&self) -> (u32, u32) {
        (self.rate, self.scale)
    }

    /// The duration of the video in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        self.video.len() as u64 * 1000 * u64::from(self.scale) / u64::from(self.rate)
    }

    /// The audio format, `None` if the movie has no sound.
    pub fn audio_format(&self) -> Option<AudioFormat> {
        self.audio_format
    }

    /// The PCM audio in order, in the format of [`Movie::audio_format`].
    pub fn audio(&self) -> Vec<u8> {
        self.audio
            .iter()
            .flat_map(|r| self.data[r.clone()].iter().copied())
            .collect()
    }

    /// The decoded video, one picture per frame in order. A frame that repeats the one before is
    /// given again. After an error the iterator ends.
    pub fn frames(&self) -> Frames<'_> {
        let info = NAVideoInfo::new(
            self.width as usize,
            self.height as usize,
            false,
            YUV410_FORMAT,
        );
        let info = NACodecInfo::new("indeo5", NACodecTypeInfo::Video(info), None);
        let stream = NAStream::new(StreamType::Video, 0, info.clone(), self.scale, self.rate, 0);
        let mut decoder = nihav_indeo::indeo5_decoder();
        let mut support = NADecoderSupport::new();
        let ready = decoder.init(&mut support, info.into_ref()).is_ok();
        Frames {
            movie: self,
            stream: stream.into(),
            decoder,
            support,
            last: None,
            next: 0,
            done: !ready,
        }
    }
}

/// The iterator of [`Movie::frames`].
pub struct Frames<'a> {
    movie: &'a Movie,
    stream: std::sync::Arc<NAStream>,
    decoder: Box<dyn NADecoder + Send>,
    support: NADecoderSupport,
    last: Option<NAVideoBufferRef<u8>>,
    next: usize,
    done: bool,
}

impl Frames<'_> {
    /// Decodes the frame in `chunk`; a frame that carries no picture repeats the previous one.
    fn decode(&mut self, chunk: &[u8]) -> Result<Framebuffer, MovieError> {
        if !chunk.is_empty() {
            let time = NATimeInfo::new(
                Some(self.next as u64),
                None,
                Some(1),
                self.movie.scale,
                self.movie.rate,
            );
            let packet = NAPacket::new(self.stream.clone(), time, false, chunk.to_vec());
            // The decoder is third-party code on untrusted data: a panic is an error too.
            let frame = catch_unwind(AssertUnwindSafe(|| {
                self.decoder.decode(&mut self.support, &packet)
            }))
            .map_err(|_| MovieError::Decode("the decoder failed".into()))?
            .map_err(|e| MovieError::Decode(format!("{e:?}")))?;
            if let NABufferType::Video(picture) = frame.get_buffer() {
                self.last = Some(picture);
            }
        }
        let picture = self
            .last
            .as_ref()
            .ok_or(MovieError::Decode("no picture to show yet".into()))?;
        Ok(rgba(picture))
    }
}

impl Iterator for Frames<'_> {
    type Item = Result<Framebuffer, MovieError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let range = self.movie.video.get(self.next)?.clone();
        let result = self.decode(&self.movie.data[range]);
        self.next += 1;
        self.done = result.is_err();
        Some(result)
    }
}

/// Converts a picture of three planes, the second and third at lower resolution, from limited
/// range BT.601 YUV to RGBA. Chroma is repeated over its pixels.
fn rgba(picture: &NAVideoBufferRef<u8>) -> Framebuffer {
    let info = picture.get_info();
    let (width, height) = (info.get_width(), info.get_height());
    let data = picture.get_data();
    let plane = |p: usize| {
        let (w, h) = picture.get_dimensions(p);
        (picture.get_offset(p), picture.get_stride(p), w, h)
    };
    let (luma, u, v) = (plane(0), plane(1), plane(2));
    let mut out = Framebuffer::new(width as u32, height as u32);
    let sample = |(offset, stride, w, h): (usize, usize, usize, usize), x: usize, y: usize| {
        let (x, y) = ((x * w / width).min(w - 1), (y * h / height).min(h - 1));
        i32::from(data[offset + y * stride + x])
    };
    for (i, px) in out.pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (i % width, i / width);
        let c = 298 * (sample(luma, x, y) - 16);
        let (d, e) = (sample(u, x, y) - 128, sample(v, x, y) - 128);
        let channel = |value: i32| ((value + 128) >> 8).clamp(0, 255) as u8;
        *px = [
            channel(c + 409 * e),
            channel(c - 100 * d - 208 * e),
            channel(c + 516 * d),
            255,
        ];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chunk with its padding.
    fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(body);
        out.resize(out.len() + body.len() % 2, 0);
        out
    }

    fn list(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        chunk(b"LIST", &[kind.as_slice(), body].concat())
    }

    fn strh(kind: &[u8; 4], scale: u32, rate: u32) -> Vec<u8> {
        let mut h = kind.to_vec();
        h.resize(20, 0);
        h.extend(scale.to_le_bytes());
        h.extend(rate.to_le_bytes());
        h.resize(56, 0);
        chunk(b"strh", &h)
    }

    fn video_format(width: u32, height: u32, codec: &[u8; 4]) -> Vec<u8> {
        let mut f = 40u32.to_le_bytes().to_vec();
        f.extend(width.to_le_bytes());
        f.extend(height.to_le_bytes());
        f.extend([1, 0, 9, 0]);
        f.extend(codec);
        f.resize(40, 0);
        chunk(b"strf", &f)
    }

    fn audio_format(tag: u16, channels: u16, rate: u32, bits: u16) -> Vec<u8> {
        let mut f = tag.to_le_bytes().to_vec();
        f.extend(channels.to_le_bytes());
        f.extend(rate.to_le_bytes());
        f.extend((rate * u32::from(channels * bits / 8)).to_le_bytes());
        f.extend((channels * bits / 8).to_le_bytes());
        f.extend(bits.to_le_bytes());
        f.extend([0, 0]);
        chunk(b"strf", &f)
    }

    /// An AVI file: the stream headers, then the data chunks in the order given.
    fn avi(codec: &[u8; 4], audio: u16, data: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let video = [strh(b"vids", 1001, 30000), video_format(16, 16, codec)].concat();
        let sound = [strh(b"auds", 1, 8000), audio_format(audio, 1, 8000, 16)].concat();
        let headers = [
            chunk(b"avih", &[0; 56]),
            list(b"strl", &video),
            list(b"strl", &sound),
        ]
        .concat();
        let chunks: Vec<u8> = data
            .iter()
            .flat_map(|(tag, body)| chunk(tag, body))
            .collect();
        let body = [
            b"AVI ".to_vec(),
            list(b"hdrl", &headers),
            chunk(b"JUNK", &[0; 3]),
            list(b"movi", &chunks),
        ];
        chunk(b"RIFF", &body.concat())
    }

    /// An Indeo 5 key frame of 16 x 16 pixels (one macroblock per 8 x 8 luma or 4 x 4 chroma
    /// block, none with coefficients), and a null frame.
    fn key_frame_and_null() -> (Vec<u8>, Vec<u8>) {
        let mut bits: Vec<bool> = Vec::new();
        let put = |bits: &mut Vec<bool>, value: u32, count: u32| {
            bits.extend((0..count).map(|i| value >> i & 1 == 1))
        };
        let align = |bits: &mut Vec<bool>| bits.resize(bits.len().div_ceil(8) * 8, false);
        put(&mut bits, 0x1F, 5); // sync
        put(&mut bits, 0, 3); // key frame
        put(&mut bits, 0, 8); // frame number
        put(&mut bits, 0, 8); // GOP flags: nothing optional
        put(&mut bits, 0, 2); // one luma band
        put(&mut bits, 0, 1); // one chroma band
        put(&mut bits, 15, 4); // an explicit size
        put(&mut bits, 16, 13); // height
        put(&mut bits, 16, 13); // width
        put(&mut bits, 2, 6); // luma band: macroblocks of one 8 x 8 block
        put(&mut bits, 6, 6); // chroma band: macroblocks of one 4 x 4 block
        align(&mut bits);
        put(&mut bits, 0, 24); // skipped, then no extension
        align(&mut bits);
        put(&mut bits, 0, 8); // frame flags
        put(&mut bits, 0, 3); // skipped
        align(&mut bits);
        for macroblocks in [4, 1, 1] {
            put(&mut bits, 0, 14); // band flags, checksum flag, quantiser
            align(&mut bits);
            put(&mut bits, 2, 2); // a coded tile
            put(&mut bits, 3, 8); // its size in bytes
            align(&mut bits);
            put(&mut bits, 0, 2 * macroblocks); // per macroblock: coded, no coefficients
            align(&mut bits);
        }
        let pack = |bits: &[bool]| {
            bits.chunks(8)
                .map(|c| c.iter().enumerate().map(|(i, b)| u8::from(*b) << i).sum())
                .collect()
        };
        (pack(&bits), vec![0x1F | 4 << 5, 0])
    }

    #[test]
    fn the_container_is_read() {
        let data = avi(
            b"IV50",
            1,
            &[
                (b"00dc", b"abc"),
                (b"01wb", &[1, 2]),
                (b"00dc", b""),
                (b"01wb", &[3, 4]),
                (b"02xx", b"?"),
            ],
        );
        let movie = Movie::parse(data).unwrap();
        assert_eq!(movie.size(), (16, 16));
        assert_eq!(movie.frame_count(), 2);
        assert_eq!(movie.frame_rate(), (30000, 1001));
        assert_eq!(movie.duration_ms(), 66);
        assert_eq!(
            movie.audio_format(),
            Some(AudioFormat {
                channels: 1,
                rate: 8000,
                bits: 16
            })
        );
        assert_eq!(movie.audio(), [1, 2, 3, 4]);
    }

    #[test]
    fn chunks_in_rec_lists_keep_their_order() {
        let rec =
            |a: &[u8], b: &[u8]| list(b"rec ", &[chunk(b"00dc", a), chunk(b"01wb", b)].concat());
        let data = [rec(b"1", &[1, 1]), rec(b"2", &[2, 2]), chunk(b"00dc", b"3")].concat();
        let headers = [
            chunk(b"avih", &[0; 56]),
            list(
                b"strl",
                &[strh(b"vids", 1, 1), video_format(16, 16, b"IV50")].concat(),
            ),
            list(
                b"strl",
                &[strh(b"auds", 1, 8000), audio_format(1, 1, 8000, 16)].concat(),
            ),
        ]
        .concat();
        let body = [
            b"AVI ".to_vec(),
            list(b"hdrl", &headers),
            list(b"movi", &data),
        ]
        .concat();
        let movie = Movie::parse(chunk(b"RIFF", &body)).unwrap();
        assert_eq!(
            movie
                .video
                .iter()
                .map(|r| movie.data[r.clone()].to_vec())
                .collect::<Vec<_>>(),
            [b"1", b"2", b"3"]
        );
        assert_eq!(movie.audio(), [1, 1, 2, 2]);
    }

    #[test]
    fn a_movie_without_sound_or_with_other_audio() {
        let video = [strh(b"vids", 1, 10), video_format(8, 8, b"IV50")].concat();
        let headers = list(b"strl", &video);
        let body = [
            b"AVI ".to_vec(),
            list(b"hdrl", &headers),
            list(b"movi", &chunk(b"00dc", b"x")),
        ];
        let movie = Movie::parse(chunk(b"RIFF", &body.concat())).unwrap();
        assert_eq!(
            (
                movie.audio_format(),
                movie.audio().len(),
                movie.duration_ms()
            ),
            (None, 0, 100)
        );
        let adpcm = avi(b"IV50", 2, &[]);
        assert!(matches!(
            Movie::parse(adpcm),
            Err(MovieError::Unsupported(_))
        ));
    }

    #[test]
    fn other_video_is_unsupported() {
        assert!(matches!(
            Movie::parse(avi(b"WMV2", 1, &[])),
            Err(MovieError::Unsupported(_))
        ));
    }

    #[test]
    fn malformed_files_are_errors() {
        let good = avi(b"IV50", 1, &[(b"00dc", b"abc"), (b"01wb", &[1, 2])]);
        for len in 0..good.len() {
            let _ = Movie::parse(good[..len].to_vec()); // every truncation: an error or a movie
        }
        assert!(Movie::parse(Vec::new()).is_err());
        assert!(Movie::parse(b"RIFF\0\0\0\0WAVE".to_vec()).is_err());
        let mut huge = good.clone();
        huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        let _ = Movie::parse(huge);
        // A zero rate, a zero size, a huge size.
        for (find, replace) in [(1001u32, 0u32), (30000, 0)] {
            let mut bad = good.clone();
            let at = bad
                .windows(4)
                .position(|w| w == find.to_le_bytes())
                .unwrap();
            bad[at..at + 4].copy_from_slice(&replace.to_le_bytes());
            assert!(Movie::parse(bad).is_err());
        }
        let mut bad = good.clone();
        let at = bad
            .windows(4)
            .position(|w| w == 16u32.to_le_bytes())
            .unwrap();
        for side in [0u32, 5000, u32::MAX] {
            bad[at..at + 4].copy_from_slice(&side.to_le_bytes());
            assert!(Movie::parse(bad.clone()).is_err(), "{side}");
        }
    }

    #[test]
    fn frames_are_decoded_and_repeated() {
        let (key, null) = key_frame_and_null();
        let data = avi(
            b"IV50",
            1,
            &[(b"00dc", &key), (b"00dc", &null), (b"00dc", b"")],
        );
        let movie = Movie::parse(data).unwrap();
        let frames: Vec<_> = movie.frames().collect::<Result<_, _>>().unwrap();
        assert_eq!(frames.len(), 3);
        assert!(
            frames
                .iter()
                .all(|f| (f.width, f.height) == (16, 16) && f.pixels.len() == 16 * 16 * 4)
        );
        assert!(frames[0].pixels.chunks(4).all(|p| p[3] == 255));
        assert_eq!(frames[0].pixels[..4], [130, 130, 130, 255]);
        assert!(frames[1].pixels == frames[0].pixels && frames[2].pixels == frames[0].pixels);
    }

    #[test]
    fn a_frame_before_the_first_picture_is_an_error() {
        let (_, null) = key_frame_and_null();
        for chunk in [null.as_slice(), b"", b"garbage"] {
            let movie =
                Movie::parse(avi(b"IV50", 1, &[(b"00dc", chunk), (b"00dc", chunk)])).unwrap();
            let mut frames = movie.frames();
            assert!(frames.next().unwrap().is_err());
            assert!(frames.next().is_none(), "the iterator ends after an error");
        }
    }

    #[test]
    fn damaged_frames_do_not_panic() {
        let (key, _) = key_frame_and_null();
        for n in 0..key.len() * 8 {
            let mut damaged = key.clone();
            damaged[n / 8] ^= 1 << (n % 8);
            let movie =
                Movie::parse(avi(b"IV50", 1, &[(b"00dc", &damaged), (b"00dc", &damaged)])).unwrap();
            let _ = movie.frames().count();
        }
        for len in 0..key.len() {
            let movie = Movie::parse(avi(b"IV50", 1, &[(b"00dc", &key[..len])])).unwrap();
            let _ = movie.frames().count();
        }
    }

    #[test]
    fn colours_are_converted_in_limited_range() {
        let info = NAVideoInfo::new(4, 4, false, YUV410_FORMAT);
        let picture = nihav_core::frame::alloc_video_buffer(info, 0).unwrap();
        let mut picture = picture.get_vbuf().unwrap();
        let fill = |picture: &mut NAVideoBufferRef<u8>, y, u, v| {
            let planes: Vec<_> = (0..3)
                .map(|p| (picture.get_offset(p), picture.get_dimensions(p)))
                .collect();
            let data = picture.get_data_mut().unwrap();
            for (value, (offset, (w, h))) in [y, u, v].into_iter().zip(planes) {
                data[offset..offset + w * h].fill(value);
            }
        };
        for (yuv, expected) in [
            ((16, 128, 128), [0, 0, 0]),
            ((235, 128, 128), [255, 255, 255]),
            ((81, 90, 240), [254, 0, 0]),
        ] {
            fill(&mut picture, yuv.0, yuv.1, yuv.2);
            let out = rgba(&picture);
            let got: Vec<i32> = out.pixels[..3].iter().map(|c| i32::from(*c)).collect();
            assert!(
                got.iter().zip(expected).all(|(g, e)| (g - e).abs() <= 2),
                "{yuv:?} gave {got:?}"
            );
        }
    }

    /// The counts below were measured on Zoo Tycoon: Complete Collection.
    #[test]
    #[ignore = "needs the game: set ZT_DATA_DIR to the install folder"]
    fn full_game_facts() {
        let dir = std::env::var_os("ZT_DATA_DIR").expect("ZT_DATA_DIR is not set");
        let vfs = crate::vfs::Vfs::open(std::path::Path::new(&dir)).unwrap();
        for (member, size, frames, audio) in [
            ("mslogo.avi", (640, 480), 302, 1934728),
            ("bflogo.avi", (640, 480), 450, 1655072),
        ] {
            let movie = Movie::parse(vfs.read(member).unwrap()).unwrap();
            assert_eq!(
                (movie.size(), movie.frame_count()),
                (size, frames),
                "{member}"
            );
            assert_eq!(movie.audio().len(), audio, "{member}");
            assert_eq!(
                movie.frames().filter(Result::is_ok).count(),
                frames,
                "{member}"
            );
        }
    }
}
