// SPDX-License-Identifier: AGPL-3.0-or-later
// Part of NihAV by Kostya Shishkov (https://nihav.org), trimmed; see vendor/README.md and COPYING.
//! Image sample format definitions.
//!
//! NihAV does not have a fixed list of supported formats but rather accepts format definitions for video.
//! In result exotic formats like YUV410+alpha plane that is used by Indeo 4 are supported without any additional case handing.
//! Some common format definitions are provided as constants for convenience.


/// A list of RGB colour model variants.
#[derive(Debug,Clone,Copy,PartialEq)]
pub enum RGBSubmodel {
    RGB,
    SRGB,
}


/// A list of YUV colour model variants.
#[derive(Debug,Clone,Copy,PartialEq)]
pub enum YUVSubmodel {
    YCbCr,
    /// NTSC variant.
    YIQ,
    /// The YUV variant used by JPEG.
    YUVJ,
}


/// A list of known colour models.
#[derive(Debug, Clone,Copy,PartialEq)]
pub enum ColourModel {
    RGB(RGBSubmodel),
    YUV(YUVSubmodel),
    CMYK,
    HSV,
    LAB,
    XYZ,
}


/// Single colourspace component definition.
///
/// This structure defines how components of a colourspace are subsampled and where and how they are stored.
#[derive(Debug,Clone,Copy,PartialEq)]
pub struct NAPixelChromaton {
    /// Horizontal subsampling in power of two (e.g. `0` = no subsampling, `1` = only every second value is stored).
    pub h_ss:           u8,
    /// Vertial subsampling in power of two (e.g. `0` = no subsampling, `1` = only every second value is stored).
    pub v_ss:           u8,
    /// A flag to signal that component is packed.
    pub packed:         bool,
    /// Bit depth of current component.
    pub depth:          u8,
    /// Shift for packed components.
    pub shift:          u8,
    /// Component offset for byte-packed components.
    pub comp_offs:      u8,
    /// The distance to the next packed element in bytes.
    pub next_elem:      u8,
}


/// The current limit on number of components in image colourspace model (including alpha component).
pub const MAX_CHROMATONS: usize = 5;

/// Image colourspace representation.
///
/// This structure includes both definitions for each component and some common definitions.
/// For example the format can be paletted and then components describe the palette storage format while actual data is 8-bit palette indices.
#[derive(Debug,Clone,Copy,PartialEq)]
pub struct NAPixelFormaton {
    /// Image colour model.
    pub model:      ColourModel,
    /// Actual number of components present.
    pub components: u8,
    /// Format definition for each component.
    pub comp_info:  [Option<NAPixelChromaton>; MAX_CHROMATONS],
    /// Single pixel size for packed formats.
    pub elem_size:  u8,
    /// A flag signalling that data is stored as big-endian.
    pub be:         bool,
    /// A flag signalling that image has alpha component.
    pub alpha:      bool,
    /// A flag signalling that data is paletted.
    ///
    /// This means that image data is stored as 8-bit indices (in the first image component) for the palette stored as second component of the image and actual palette format is described in this structure.
    pub palette:    bool,
}

macro_rules! chromaton {
    ($hs: expr, $vs: expr, $pck: expr, $d: expr, $sh: expr, $co: expr, $ne: expr) => ({
        Some(NAPixelChromaton{ h_ss: $hs, v_ss: $vs, packed: $pck, depth: $d, shift: $sh, comp_offs: $co, next_elem: $ne })
    });
    (yuv8; $hs: expr, $vs: expr, $co: expr) => ({
        Some(NAPixelChromaton{ h_ss: $hs, v_ss: $vs, packed: false, depth: 8, shift: 0, comp_offs: $co, next_elem: 1 })
    });
    (packrgb; $d: expr, $s: expr, $co: expr, $ne: expr) => ({
        Some(NAPixelChromaton{ h_ss: 0, v_ss: 0, packed: true, depth: $d, shift: $s, comp_offs: $co, next_elem: $ne })
    });
    (pal8; $co: expr) => ({
        Some(NAPixelChromaton{ h_ss: 0, v_ss: 0, packed: true, depth: 8, shift: 0, comp_offs: $co, next_elem: 3 })
    });
}


/// Predefined format for planar 8-bit YUV with 4:1:0 subsampling.
pub const YUV410_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColourModel::YUV(YUVSubmodel::YUVJ), components: 3,
                                        comp_info: [
                                            chromaton!(0, 0, false, 8, 0, 0, 1),
                                            chromaton!(yuv8; 2, 2, 1),
                                            chromaton!(yuv8; 2, 2, 2),
                                            None, None],
                                        elem_size: 0, be: false, alpha: false, palette: false };
/// Predefined format for planar 8-bit YUV with 4:1:0 subsampling and alpha component.
pub const YUVA410_FORMAT: NAPixelFormaton = NAPixelFormaton { model: ColourModel::YUV(YUVSubmodel::YUVJ), components: 4,
                                        comp_info: [
                                            chromaton!(0, 0, false, 8, 0, 0, 1),
                                            chromaton!(yuv8; 2, 2, 1),
                                            chromaton!(yuv8; 2, 2, 2),
                                            chromaton!(0, 0, false, 8, 0, 3, 1),
                                            None],
                                        elem_size: 0, be: false, alpha: true, palette: false };


impl NAPixelChromaton {
    /// Returns subsampling for the current component.
    pub fn get_subsampling(self) -> (u8, u8) { (self.h_ss, self.v_ss) }
    /// Reports whether current component is packed.
    pub fn is_packed(self) -> bool { self.packed }
    /// Returns bit depth of current component.
    pub fn get_depth(self) -> u8   { self.depth }
    /// Returns bit shift for packed component.
    pub fn get_shift(self) -> u8   { self.shift }

    /// Calculates the width for current component from general image width.
    pub fn get_width(self, width: usize) -> usize {
        (width  + ((1 << self.h_ss) - 1)) >> self.h_ss
    }
    /// Calculates the height for current component from general image height.
    pub fn get_height(self, height: usize) -> usize {
        (height + ((1 << self.v_ss) - 1)) >> self.v_ss
    }
    /// Calculates the minimal stride for current component from general image width.
    pub fn get_linesize(self, width: usize) -> usize {
        let d = self.depth as usize;
        if self.packed {
            (self.get_width(width) * d + d - 1) >> 3
        } else {
            self.get_width(width)
        }
    }
}


impl NAPixelFormaton {

    /// Returns the number of components.
    pub fn get_num_comp(&self) -> usize { self.components as usize }
    /// Returns selected component information.
    pub fn get_chromaton(&self, idx: usize) -> Option<NAPixelChromaton> {
        if idx < self.comp_info.len() { return self.comp_info[idx]; }
        None
    }
    /// Reports whether this is paletted format.
    pub fn is_paletted(self) -> bool { self.palette }
    /// Returns single packed pixel size.
    pub fn get_elem_size(self) -> u8 { self.elem_size }
    /// Returns the total amount of bits needed for components.
    pub fn get_total_depth(&self) -> u8 {
        let mut depth = 0;
        for chromaton in self.comp_info.iter().flatten() {
            depth += chromaton.depth;
        }
        depth
    }
}
