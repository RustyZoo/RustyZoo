// SPDX-License-Identifier: AGPL-3.0-or-later
// Part of NihAV by Kostya Shishkov (https://nihav.org), trimmed; see vendor/README.md and COPYING.
//! Packets and decoded frames functionality.
use std::cmp::max;
//use std::collections::HashMap;
pub use std::sync::Arc;
pub use crate::formats::*;
pub use crate::refs::*;

/// Video stream information.
#[derive(Clone,Copy,PartialEq)]
pub struct NAVideoInfo {
    /// Picture width.
    pub width:      usize,
    /// Picture height.
    pub height:     usize,
    /// Picture is stored downside up.
    pub flipped:    bool,
    /// Picture pixel format.
    pub format:     NAPixelFormaton,
    /// Declared bits per sample.
    pub bits:       u8,
}

impl NAVideoInfo {
    /// Constructs a new `NAVideoInfo` instance.
    pub fn new(w: usize, h: usize, flip: bool, fmt: NAPixelFormaton) -> Self {
        let bits = fmt.get_total_depth();
        NAVideoInfo { width: w, height: h, flipped: flip, format: fmt, bits }
    }
    /// Returns picture width.
    pub fn get_width(&self)  -> usize { self.width }
    /// Returns picture height.
    pub fn get_height(&self) -> usize { self.height }
    /// Returns picture orientation.
    pub fn is_flipped(&self) -> bool { self.flipped }
    /// Returns picture pixel format.
    pub fn get_format(&self) -> NAPixelFormaton { self.format }
    /// Sets new picture width.
    pub fn set_width(&mut self, w: usize)  { self.width  = w; }
    /// Sets new picture height.
    pub fn set_height(&mut self, h: usize) { self.height = h; }
}


/// A list of possible stream information types.
#[derive(Clone,Copy,PartialEq)]
pub enum NACodecTypeInfo {
    /// No codec present.
    None,
    /// Video codec information.
    Video(NAVideoInfo),
}


/// Decoded video frame.
///
/// NihAV frames are stored in native type (8/16/32-bit elements) inside a single buffer.
/// In case of image with several components those components are stored sequentially and can be accessed in the buffer starting at corresponding component offset.
#[derive(Clone)]
pub struct NAVideoBuffer<T> {
    info:    NAVideoInfo,
    data:    NABufferRef<Vec<T>>,
    offs:    Vec<usize>,
    strides: Vec<usize>,
}

impl<T: Clone> NAVideoBuffer<T> {
    /// Returns the component offset (0 for all unavailable offsets).
    pub fn get_offset(&self, idx: usize) -> usize {
        if idx >= self.offs.len() { 0 }
        else { self.offs[idx] }
    }
    /// Returns picture info.
    pub fn get_info(&self) -> NAVideoInfo { self.info }
    /// Returns an immutable reference to the data.
    pub fn get_data(&self) -> &Vec<T> { self.data.as_ref() }
    /// Returns a mutable reference to the data.
    pub fn get_data_mut(&mut self) -> Option<&mut Vec<T>> { self.data.as_mut() }
    /// Returns stride (distance between subsequent lines) for the requested component.
    pub fn get_stride(&self, idx: usize) -> usize {
        if idx >= self.strides.len() { return 0; }
        self.strides[idx]
    }
    /// Returns requested component dimensions.
    pub fn get_dimensions(&self, idx: usize) -> (usize, usize) {
        get_plane_size(&self.info, idx)
    }
    /// Converts current instance into buffer reference.
    pub fn into_ref(self) -> NABufferRef<Self> {
        NABufferRef::new(self)
    }

}

/// A specialised type for reference-counted `NAVideoBuffer`.
pub type NAVideoBufferRef<T> = NABufferRef<NAVideoBuffer<T>>;

/// A list of possible decoded frame types.
#[derive(Clone)]
pub enum NABufferType {
    /// 8-bit video buffer.
    Video      (NAVideoBufferRef<u8>),
    /// 16-bit video buffer (i.e. every component or packed pixel fits into 16 bits).
    Video16    (NAVideoBufferRef<u16>),
    /// 32-bit video buffer (i.e. every component or packed pixel fits into 32 bits).
    Video32    (NAVideoBufferRef<u32>),
    /// Packed video buffer.
    VideoPacked(NAVideoBufferRef<u8>),
    /// Buffer with generic data (e.g. subtitles).
    Data       (NABufferRef<Vec<u8>>),
    /// No data present.
    None,
}

impl NABufferType {
    /// Returns reference to 8-bit (or packed) video buffer.
    pub fn get_vbuf(&self) -> Option<NAVideoBufferRef<u8>> {
        match *self {
            NABufferType::Video(ref vb)       => Some(vb.clone()),
            NABufferType::VideoPacked(ref vb) => Some(vb.clone()),
            _ => None,
        }
    }
}


/// A list of possible frame allocator errors.
#[derive(Debug,Clone,Copy,PartialEq)]
pub enum AllocatorError {
    /// Requested picture dimensions are too large.
    TooLargeDimensions,
    /// Invalid input format.
    FormatError,
}

/// Constructs a new video buffer with requested format.
///
/// `align` is power of two alignment for image. E.g. the value of 5 means that frame dimensions will be padded to be multiple of 32.
pub fn alloc_video_buffer(vinfo: NAVideoInfo, align: u8) -> Result<NABufferType, AllocatorError> {
    let fmt = &vinfo.format;
    let mut new_size: usize = 0;
    let mut offs:    Vec<usize> = Vec::new();
    let mut strides: Vec<usize> = Vec::new();

    for i in 0..fmt.get_num_comp() {
        if fmt.get_chromaton(i).is_none() { return Err(AllocatorError::FormatError); }
    }

    let align_mod = ((1 << align) as usize) - 1;
    let width  = (vinfo.width  + align_mod) & !align_mod;
    let height = (vinfo.height + align_mod) & !align_mod;
    let mut max_depth = 0;
    let mut all_packed = true;
    let mut all_bytealigned = true;
    for i in 0..fmt.get_num_comp() {
        let ochr = fmt.get_chromaton(i);
        if ochr.is_none() { continue; }
        let chr = ochr.unwrap();
        if !chr.is_packed() {
            all_packed = false;
        } else if ((chr.get_shift() + chr.get_depth()) & 7) != 0 {
            all_bytealigned = false;
        }
        max_depth = max(max_depth, chr.get_depth());
    }
    let unfit_elem_size = !matches!(fmt.get_elem_size(), 2 | 4);

//todo semi-packed like NV12
    if fmt.is_paletted() {
//todo various-sized palettes?
        let stride = vinfo.get_format().get_chromaton(0).unwrap().get_linesize(width);
        let pic_sz = stride.checked_mul(height);
        if pic_sz.is_none() { return Err(AllocatorError::TooLargeDimensions); }
        let pal_size = 256 * (fmt.get_elem_size() as usize);
        let new_size = pic_sz.unwrap().checked_add(pal_size);
        if new_size.is_none() { return Err(AllocatorError::TooLargeDimensions); }
        offs.push(0);
        offs.push(stride * height);
        strides.push(stride);
        let data: Vec<u8> = vec![0; new_size.unwrap()];
        let buf: NAVideoBuffer<u8> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
        Ok(NABufferType::Video(buf.into_ref()))
    } else if !all_packed {
        for i in 0..fmt.get_num_comp() {
            let ochr = fmt.get_chromaton(i);
            if ochr.is_none() { continue; }
            let chr = ochr.unwrap();
            offs.push(new_size);
            let stride = chr.get_linesize(width);
            let cur_h = chr.get_height(height);
            let cur_sz = stride.checked_mul(cur_h);
            if cur_sz.is_none() { return Err(AllocatorError::TooLargeDimensions); }
            let new_sz = new_size.checked_add(cur_sz.unwrap());
            if new_sz.is_none() { return Err(AllocatorError::TooLargeDimensions); }
            new_size = new_sz.unwrap();
            strides.push(stride);
        }
        if max_depth <= 8 {
            let data: Vec<u8> = vec![0; new_size];
            let buf: NAVideoBuffer<u8> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
            Ok(NABufferType::Video(buf.into_ref()))
        } else if max_depth <= 16 {
            let data: Vec<u16> = vec![0; new_size];
            let buf: NAVideoBuffer<u16> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
            Ok(NABufferType::Video16(buf.into_ref()))
        } else {
            let data: Vec<u32> = vec![0; new_size];
            let buf: NAVideoBuffer<u32> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
            Ok(NABufferType::Video32(buf.into_ref()))
        }
    } else if all_bytealigned || unfit_elem_size {
        let elem_sz = fmt.get_elem_size();
        let line_sz = width.checked_mul(elem_sz as usize);
        if line_sz.is_none() { return Err(AllocatorError::TooLargeDimensions); }
        let new_sz = line_sz.unwrap().checked_mul(height);
        if new_sz.is_none() { return Err(AllocatorError::TooLargeDimensions); }
        new_size = new_sz.unwrap();
        let data: Vec<u8> = vec![0; new_size];
        strides.push(line_sz.unwrap());
        let buf: NAVideoBuffer<u8> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
        Ok(NABufferType::VideoPacked(buf.into_ref()))
    } else {
        let elem_sz = fmt.get_elem_size();
        let new_sz = width.checked_mul(height);
        if new_sz.is_none() { return Err(AllocatorError::TooLargeDimensions); }
        new_size = new_sz.unwrap();
        match elem_sz {
            2 => {
                    let data: Vec<u16> = vec![0; new_size];
                    strides.push(width);
                    let buf: NAVideoBuffer<u16> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
                    Ok(NABufferType::Video16(buf.into_ref()))
                },
            4 => {
                    let data: Vec<u32> = vec![0; new_size];
                    strides.push(width);
                    let buf: NAVideoBuffer<u32> = NAVideoBuffer { data: NABufferRef::new(data), info: vinfo, offs, strides };
                    Ok(NABufferType::Video32(buf.into_ref()))
                },
            _ => unreachable!(),
        }
    }
}


/// Information about codec contained in a stream.
#[derive(Clone)]
pub struct NACodecInfo {
    name:       &'static str,
    properties: NACodecTypeInfo,
    extradata:  Option<Arc<Vec<u8>>>,
}

/// A specialised type for reference-counted `NACodecInfo`.
pub type NACodecInfoRef = Arc<NACodecInfo>;

impl NACodecInfo {
    /// Constructs a new instance of `NACodecInfo`.
    pub fn new(name: &'static str, p: NACodecTypeInfo, edata: Option<Vec<u8>>) -> Self {
        NACodecInfo { name, properties: p, extradata: edata.map(Arc::new) }
    }
    /// Constructs a new reference-counted instance of `NACodecInfo`.
    pub fn new_ref(name: &'static str, p: NACodecTypeInfo, edata: Option<Arc<Vec<u8>>>) -> Self {
        NACodecInfo { name, properties: p, extradata: edata }
    }
    /// Converts current instance into a reference-counted one.
    pub fn into_ref(self) -> NACodecInfoRef { Arc::new(self) }
    /// Returns codec information.
    pub fn get_properties(&self) -> NACodecTypeInfo { self.properties }
    /// Returns additional initialisation data required by the codec.
    pub fn get_extradata(&self) -> Option<Arc<Vec<u8>>> {
        if let Some(ref vec) = self.extradata { return Some(vec.clone()); }
        None
    }
    /// Returns codec name.
    pub fn get_name(&self) -> &'static str { self.name }
    /// Constructs a new empty reference-counted instance of `NACodecInfo`.
    pub fn new_dummy() -> Arc<Self> {
        Arc::new(DUMMY_CODEC_INFO)
    }
}

impl Default for NACodecInfo {
    fn default() -> Self { DUMMY_CODEC_INFO }
}


/// Default empty codec information.
pub const DUMMY_CODEC_INFO: NACodecInfo = NACodecInfo {
                                name: "none",
                                properties: NACodecTypeInfo::None,
                                extradata: None };

/// A list of recognised frame types.
#[derive(Debug,Clone,Copy,PartialEq)]
pub enum FrameType {
    /// Intra frame type.
    I,
    /// Inter frame type.
    P,
    /// Bidirectionally predicted frame.
    B,
    /// Skip frame.
    ///
    /// When such frame is encountered then last frame should be used again if it is needed.
    Skip,
    /// Some other frame type.
    Other,
}


/// Timestamp information.
#[derive(Debug,Clone,Copy)]
pub struct NATimeInfo {
    /// Presentation timestamp.
    pub pts:            Option<u64>,
    /// Decode timestamp.
    pub dts:            Option<u64>,
    /// Duration (in timebase units).
    pub duration:       Option<u64>,
    /// Timebase numerator.
    pub tb_num:         u32,
    /// Timebase denominator.
    pub tb_den:         u32,
}

impl NATimeInfo {
    /// Constructs a new `NATimeInfo` instance.
    pub fn new(pts: Option<u64>, dts: Option<u64>, duration: Option<u64>, tb_num: u32, tb_den: u32) -> Self {
        NATimeInfo { pts, dts, duration, tb_num, tb_den }
    }

}


/// Decoded frame information.
#[derive(Clone)]
pub struct NAFrame {
    /// Frame timestamp.
    pub ts:             NATimeInfo,
    /// Frame ID.
    pub id:             i64,
        buffer:         NABufferType,
        info:           NACodecInfoRef,
    /// Frame type.
    pub frame_type:     FrameType,
    /// Keyframe flag.
    pub key:            bool,
//        options:        HashMap<String, NAValue>,
}

/// A specialised type for reference-counted `NAFrame`.
pub type NAFrameRef = Arc<NAFrame>;

fn get_plane_size(info: &NAVideoInfo, idx: usize) -> (usize, usize) {
    let chromaton = info.get_format().get_chromaton(idx);
    if chromaton.is_none() { return (0, 0); }
    let (hs, vs) = chromaton.unwrap().get_subsampling();
    let w = (info.get_width()  + ((1 << hs) - 1)) >> hs;
    let h = (info.get_height() + ((1 << vs) - 1)) >> vs;
    (w, h)
}

impl NAFrame {
    /// Constructs a new `NAFrame` instance.
    pub fn new(ts:             NATimeInfo,
               ftype:          FrameType,
               keyframe:       bool,
               info:           NACodecInfoRef,
               /*options:        HashMap<String, NAValue>,*/
               buffer:         NABufferType) -> Self {
        NAFrame { ts, id: 0, buffer, info, frame_type: ftype, key: keyframe/*, options*/ }
    }
    /// Sets new frame type.
    pub fn set_frame_type(&mut self, ftype: FrameType) { self.frame_type = ftype; }
    /// Sets keyframe flag.
    pub fn set_keyframe(&mut self, key: bool) { self.key = key; }

    /// Returns a reference to the frame data.
    pub fn get_buffer(&self) -> NABufferType { self.buffer.clone() }

    /// Converts current instance into a reference-counted one.
    pub fn into_ref(self) -> NAFrameRef { Arc::new(self) }

    /// Creates new frame with metadata from `NAPacket`.
    pub fn new_from_pkt(pkt: &NAPacket, info: NACodecInfoRef, buf: NABufferType) -> NAFrame {
        NAFrame::new(pkt.ts, FrameType::Other, pkt.keyframe, info, /*HashMap::new(),*/ buf)
    }
}


/// A list of possible stream types.
#[derive(Default,Debug,Clone,Copy,PartialEq)]
pub enum StreamType {
    /// Video stream.
    Video,
    /// Audio stream.
    Audio,
    /// Subtitles.
    Subtitles,
    /// Any data stream (or might be an unrecognised audio/video stream).
    Data,
    /// Nonexistent stream.
    #[default]
    None,
}


/// Stream data.
#[derive(Clone)]
pub struct NAStream {
        media_type:     StreamType,
    /// Stream ID.
    pub id:             u32,
        num:            usize,
        info:           NACodecInfoRef,
    /// Timebase numerator.
    pub tb_num:         u32,
    /// Timebase denominator.
    pub tb_den:         u32,
    /// Duration in timebase units (zero if not available).
    pub duration:       u64,
}

/// A specialised reference-counted `NAStream` type.
pub type NAStreamRef = Arc<NAStream>;

/// Downscales the timebase by its greatest common denominator.
#[allow(clippy::comparison_chain)]
pub fn reduce_timebase(tb_num: u32, tb_den: u32) -> (u32, u32) {
    if tb_num == 0 { return (tb_num, tb_den); }
    if (tb_den % tb_num) == 0 { return (1, tb_den / tb_num); }

    let mut a = tb_num;
    let mut b = tb_den;

    while a != b {
        if a > b { a -= b; }
        else if b > a { b -= a; }
    }

    (tb_num / a, tb_den / a)
}

impl NAStream {
    /// Constructs a new `NAStream` instance.
    pub fn new(mt: StreamType, id: u32, info: NACodecInfo, tb_num: u32, tb_den: u32, duration: u64) -> Self {
        let (n, d) = reduce_timebase(tb_num, tb_den);
        NAStream { media_type: mt, id, num: 0, info: info.into_ref(), tb_num: n, tb_den: d, duration }
    }
}


/// Side data that may accompany demuxed data.
#[derive(Clone)]
pub enum NASideData {
    /// Palette information.
    ///
    /// This side data contains a flag signalling that palette has changed since previous time and a reference to the current palette.
    /// Palette is stored in 8-bit RGBA format.
    Palette(bool, Arc<[u8; 1024]>),
    /// Generic user data.
    UserData(Arc<Vec<u8>>),
}

/// Packet with compressed data.
pub struct NAPacket {
        stream:         NAStreamRef,
    /// Packet timestamp.
    pub ts:             NATimeInfo,
        buffer:         NABufferRef<Vec<u8>>,
    /// Keyframe flag.
    pub keyframe:       bool,
//    options:        HashMap<String, NAValue<'a>>,
    /// Packet side data (e.g. palette for paletted formats).
    pub side_data:      Vec<NASideData>,
}

impl NAPacket {
    /// Constructs a new `NAPacket` instance.
    pub fn new(stream: NAStreamRef, ts: NATimeInfo, kf: bool, vec: Vec<u8>) -> Self {
//        let mut vec: Vec<u8> = Vec::new();
//        vec.resize(size, 0);
        NAPacket { stream, ts, keyframe: kf, buffer: NABufferRef::new(vec), side_data: Vec::new() }
    }
    /// Returns a reference to packet data.
    pub fn get_buffer(&self) -> NABufferRef<Vec<u8>> { self.buffer.clone() }
}

impl Drop for NAPacket {
    fn drop(&mut self) {}
}


#[cfg(test)]
mod test {
    use super::*;

}
