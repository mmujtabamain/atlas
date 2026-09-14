//! A captured frame: window pixels as packed RGB, plus PNG encoding and the
//! ZPixmap -> RGB conversion that `GetImage` replies need.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use anyhow::{Context, Result, bail};

/// How the X server lays out one pixel in a ZPixmap reply. Everything needed to
/// pull red/green/blue out of a pixel value without assuming BGRX.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelLayout {
    /// Bits per pixel in the image data (32 for depth 24 and 32 on every real server).
    pub bits_per_pixel: u8,
    /// `true` when the server's `image_byte_order` is LSB first (x86 servers).
    pub lsb_first: bool,
    /// TrueColor channel masks from the visual.
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
}

impl PixelLayout {
    /// The layout of practically every X server on x86: 32bpp, little-endian, 0xRRGGBB.
    pub const BGRX_LE: PixelLayout = PixelLayout {
        bits_per_pixel: 32,
        lsb_first: true,
        red_mask: 0x00ff_0000,
        green_mask: 0x0000_ff00,
        blue_mask: 0x0000_00ff,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// `width * height * 3` bytes, row-major, RGB.
    pub rgb: Vec<u8>,
}

impl Frame {
    /// Converts raw `GetImage` ZPixmap data to RGB.
    ///
    /// `data` must hold `height` rows of `width` pixels, each pixel
    /// `layout.bits_per_pixel / 8` bytes (X pads rows to the scanline unit, but
    /// for 32 bpp the padding is zero so the rows are contiguous).
    pub fn from_zpixmap(width: u32, height: u32, data: &[u8], layout: PixelLayout) -> Result<Frame> {
        let bytes_per_pixel = match layout.bits_per_pixel {
            32 => 4,
            24 => 3,
            other => bail!("unsupported bits_per_pixel {other}; expected 24 or 32"),
        };
        let expected = width as usize * height as usize * bytes_per_pixel;
        if data.len() < expected {
            bail!(
                "GetImage returned {} bytes, need {} for {}x{} at {} bpp",
                data.len(),
                expected,
                width,
                height,
                layout.bits_per_pixel
            );
        }

        let channel = |pixel: u32, mask: u32| -> u8 {
            if mask == 0 {
                return 0;
            }
            let shift = mask.trailing_zeros();
            let width_bits = (mask >> shift).count_ones();
            let value = (pixel & mask) >> shift;
            // Scale sub-8-bit channels (e.g. 16-bit visuals) up to 0..=255.
            if width_bits >= 8 {
                (value >> (width_bits - 8)) as u8
            } else {
                ((value * 255) / ((1 << width_bits) - 1)) as u8
            }
        };

        let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
        for px in data[..expected].chunks_exact(bytes_per_pixel) {
            let pixel = match (bytes_per_pixel, layout.lsb_first) {
                (4, true) => u32::from_le_bytes([px[0], px[1], px[2], px[3]]),
                (4, false) => u32::from_be_bytes([px[0], px[1], px[2], px[3]]),
                (3, true) => u32::from_le_bytes([px[0], px[1], px[2], 0]),
                (3, false) => u32::from_be_bytes([0, px[0], px[1], px[2]]),
                _ => unreachable!(),
            };
            rgb.push(channel(pixel, layout.red_mask));
            rgb.push(channel(pixel, layout.green_mask));
            rgb.push(channel(pixel, layout.blue_mask));
        }
        Ok(Frame { width, height, rgb })
    }

    /// A frame where every pixel has the same colour: the app has not drawn
    /// anything yet (or crashed before its first frame).
    pub fn is_blank(&self) -> bool {
        match self.rgb.first_chunk::<3>() {
            None => true,
            Some(first) => self.rgb.chunks_exact(3).all(|px| px == first),
        }
    }

    /// Number of distinct colours, capped at `cap` (cheap "is there real content" signal for logs).
    pub fn distinct_colors(&self, cap: usize) -> usize {
        let mut seen = std::collections::HashSet::new();
        for px in self.rgb.chunks_exact(3) {
            seen.insert([px[0], px[1], px[2]]);
            if seen.len() >= cap {
                break;
            }
        }
        seen.len()
    }

    pub fn save_png(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().context("writing PNG header")?;
        writer.write_image_data(&self.rgb).context("writing PNG data")?;
        writer.finish().context("finishing PNG")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_bgrx_little_endian() {
        // Two pixels: pure red, then (0x11, 0x22, 0x33). Memory order B,G,R,X on LSB-first servers.
        let data = [0x00, 0x00, 0xff, 0x00, 0x33, 0x22, 0x11, 0x00];
        let f = Frame::from_zpixmap(2, 1, &data, PixelLayout::BGRX_LE).unwrap();
        assert_eq!(f.rgb, vec![0xff, 0, 0, 0x11, 0x22, 0x33]);
    }

    #[test]
    fn converts_msb_first_and_24bpp() {
        let msb = PixelLayout { lsb_first: false, ..PixelLayout::BGRX_LE };
        let data = [0x00, 0xff, 0x00, 0x00]; // X,R,G,B big-endian -> red
        let f = Frame::from_zpixmap(1, 1, &data, msb).unwrap();
        assert_eq!(f.rgb, vec![0xff, 0, 0]);

        let packed = PixelLayout { bits_per_pixel: 24, ..PixelLayout::BGRX_LE };
        let data = [0x33, 0x22, 0x11]; // B,G,R
        let f = Frame::from_zpixmap(1, 1, &data, packed).unwrap();
        assert_eq!(f.rgb, vec![0x11, 0x22, 0x33]);
    }

    #[test]
    fn scales_narrow_channels_of_16bit_visuals() {
        let rgb565 = PixelLayout {
            bits_per_pixel: 32,
            lsb_first: true,
            red_mask: 0xf800,
            green_mask: 0x07e0,
            blue_mask: 0x001f,
        };
        let data = 0xffffu32.to_le_bytes();
        let f = Frame::from_zpixmap(1, 1, &data, rgb565).unwrap();
        assert_eq!(f.rgb, vec![255, 255, 255]);
    }

    #[test]
    fn rejects_short_data_and_odd_bpp() {
        assert!(Frame::from_zpixmap(2, 2, &[0; 4], PixelLayout::BGRX_LE).is_err());
        let bad = PixelLayout { bits_per_pixel: 16, ..PixelLayout::BGRX_LE };
        assert!(Frame::from_zpixmap(1, 1, &[0; 4], bad).is_err());
    }

    #[test]
    fn blank_detection() {
        let blank = Frame { width: 2, height: 2, rgb: vec![7; 12] };
        assert!(blank.is_blank());
        assert_eq!(blank.distinct_colors(10), 1);
        let mut content = blank.clone();
        content.rgb[5] = 8;
        assert!(!content.is_blank());
        assert_eq!(content.distinct_colors(10), 2);
        assert!(Frame { width: 0, height: 0, rgb: vec![] }.is_blank());
    }

    #[test]
    fn png_round_trip_through_encoder() {
        let dir = std::env::temp_dir().join(format!("gpui-shot-test-{}", std::process::id()));
        let path = dir.join("nested").join("f.png");
        let frame = Frame { width: 3, height: 2, rgb: (0..18).collect() };
        frame.save_png(&path).unwrap();
        let decoder = png::Decoder::new(File::open(&path).unwrap());
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (3, 2));
        assert_eq!(&buf[..info.buffer_size()], &frame.rgb[..]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
