//! # Introduction
//! DDS can store the vast majority of both compressed and uncompressed GPU texture data.
//! This includes uncompressed formats like [ImageFormat::R8G8B8A8Unorm].
//! Libraries and applications for working with custom GPU texture file formats often support DDS.
//! This makes DDS a good interchange format for texture conversion workflows.
//!
//! DDS has more limited application support compared to
//! standard formats like TIFF or PNG especially on Linux and MacOS.
//! GPU compression formats tend to be lossy, which makes it a poor choice for archival purposes.
//! For this reason, it's often more convenient to work with texture data in an uncompressed format.
//!
//! image_dds enables safe and efficient compressed GPU texture conversion across platforms.
//! A conversion pipeline may look like GPU Texture <-> DDS <-> image with the
//! conversions to and from image and DDS provided by image_dds.
//!
//! Although widely supported by modern desktop and console hardware, not all contexts
//! support compressed texture formats. DDS plugins for image editors often don't support newer
//! compression formats like BC7. Rendering APIs may not support compressed formats or only make it available
//! via an extension such as in the browser.
//! image_dds supports decoding surfaces to RGBA8 for
//! better compatibility at the cost of increased memory usage.
//!
//! # Features
//! Despite the name, neither the `ddsfile` nor `image` crates are required
//! and can be disabled in the Cargo.toml by setting `default-features = false`.
//! The `"ddsfile"` and `"image"` features can then be enabled individually.
//! Surface data can still be encoded and decoded using lower level functions like
//! [decode_surface_rgba8] or [encode_surface_rgba8]. These lower level functions are
//! ideal for internal conversions in libraries or applications that want to skip intermediate formats like DDS.
//! Texture conversion utilities will probably want to use the higher level functions like
//! [image_from_dds] for convenience.
//!
//! # Limitations
//! BC2 data can be decoded but not encoded due to limitations in intel-tex-rs-2.
//! This format is very rarely used in practice.
//! Not all targets will compile by default due to intel-tex-rs-2 using the Intel ISPC compiler.
//! Precompiled kernels aren't available for all targets but can be compiled from source if needed.
//! 3D textures as well as cube map and array layers are not supported but will be added in a future update.
//! Creating DDS files with custom mipmaps or extracting mipmap data is not yet supported.
//! Supporting for floating point data will also be added in a future update.
//! This mostly impacts BC6H compression since it encodes half precision floating point data.

use bcn::*;
use rgba::*;

use thiserror::Error;

// TODO: Module level documentation explaining limitations and showing basic usage.

mod bcn;
mod rgba;
mod surface;

pub use surface::{Surface, SurfaceRgba8};

// TODO: Don't export all the functions at the crate root?
// TODO: Document that this is only available on certain features?
#[cfg(feature = "ddsfile")]
mod dds;
#[cfg(feature = "ddsfile")]
pub use dds::*;

/// The conversion quality when converting to compressed formats.
///
/// Higher quality settings run significantly slower.
/// Block compressed formats like BC7 use a fixed compression ratio,
/// so lower quality settings do not use less space than slower ones.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Quality {
    /// Faster exports with slightly lower quality.
    Fast,
    /// Normal export speed and quality.
    Normal,
    /// Slower exports for slightly higher quality.
    Slow,
}

/// Options for how many mipmaps to generate.
/// Mipmaps are counted starting from the base level,
/// so a surface with only the full resolution base level has 1 mipmap.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Mipmaps {
    /// No mipmapping. Only the base mip level will be used.
    Disabled,
    /// Use the number of mipmaps specified in the input surface.
    FromSurface,
    /// Generate mipmaps to create a surface with a desired number of mipmaps.
    /// A value of `0` or `1` is equivalent to [Mipmaps::Disabled].
    GeneratedExact(u32),
    /// Generate mipmaps starting from the base level
    /// until dimensions can be reduced no further.
    GeneratedAutomatic,
}

// Each format should have conversions to and from rgba8 and rgbaf32 for convenience.
// Document the channels and bit depths for each format (i.e bc6 is half precision float, bc7 is rgba8, etc).
/// Supported image formats for encoding and decoding.
///
/// Not all DDS formats are supported,
/// but all current variants for [ImageFormat] are supported by some version of DDS.
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ImageFormat {
    R8Unorm,
    R8G8B8A8Unorm,
    R8G8B8A8Srgb,
    R32G32B32A32Float,
    B8G8R8A8Unorm,
    B8G8R8A8Srgb,
    BC1Unorm,
    BC1Srgb,
    BC2Unorm,
    BC2Srgb,
    BC3Unorm,
    BC3Srgb,
    BC4Unorm,
    BC4Snorm,
    BC5Unorm,
    BC5Snorm,
    BC6Ufloat,
    BC6Sfloat,
    BC7Unorm,
    BC7Srgb,
}

impl ImageFormat {
    // TODO: Is it worth making these public?
    fn block_dimensions(&self) -> (u32, u32, u32) {
        match self {
            ImageFormat::BC1Unorm => (4, 4, 1),
            ImageFormat::BC1Srgb => (4, 4, 1),
            ImageFormat::BC2Unorm => (4, 4, 1),
            ImageFormat::BC2Srgb => (4, 4, 1),
            ImageFormat::BC3Unorm => (4, 4, 1),
            ImageFormat::BC3Srgb => (4, 4, 1),
            ImageFormat::BC4Unorm => (4, 4, 1),
            ImageFormat::BC4Snorm => (4, 4, 1),
            ImageFormat::BC5Unorm => (4, 4, 1),
            ImageFormat::BC5Snorm => (4, 4, 1),
            ImageFormat::BC6Ufloat => (4, 4, 1),
            ImageFormat::BC6Sfloat => (4, 4, 1),
            ImageFormat::BC7Unorm => (4, 4, 1),
            ImageFormat::BC7Srgb => (4, 4, 1),
            ImageFormat::R8Unorm => (1, 1, 1),
            ImageFormat::R8G8B8A8Unorm => (1, 1, 1),
            ImageFormat::R8G8B8A8Srgb => (1, 1, 1),
            ImageFormat::R32G32B32A32Float => (1, 1, 1),
            ImageFormat::B8G8R8A8Unorm => (1, 1, 1),
            ImageFormat::B8G8R8A8Srgb => (1, 1, 1),
        }
    }

    fn block_size_in_bytes(&self) -> usize {
        match self {
            ImageFormat::R8Unorm => 1,
            ImageFormat::R8G8B8A8Unorm => 4,
            ImageFormat::R8G8B8A8Srgb => 4,
            ImageFormat::R32G32B32A32Float => 16,
            ImageFormat::B8G8R8A8Unorm => 4,
            ImageFormat::B8G8R8A8Srgb => 4,
            ImageFormat::BC1Unorm => 8,
            ImageFormat::BC1Srgb => 8,
            ImageFormat::BC2Unorm => 16,
            ImageFormat::BC2Srgb => 16,
            ImageFormat::BC3Unorm => 16,
            ImageFormat::BC3Srgb => 16,
            ImageFormat::BC4Unorm => 8,
            ImageFormat::BC4Snorm => 8,
            ImageFormat::BC5Unorm => 16,
            ImageFormat::BC5Snorm => 16,
            ImageFormat::BC6Ufloat => 16,
            ImageFormat::BC6Sfloat => 16,
            ImageFormat::BC7Unorm => 16,
            ImageFormat::BC7Srgb => 16,
        }
    }
}

// TODO: error module?
#[derive(Debug, Error)]
pub enum CreateImageError {
    #[error("data length {data_length} is not valid for a {width}x{height} image")]
    InvalidSurfaceDimensions {
        width: u32,
        height: u32,
        data_length: usize,
    },

    #[error("error decompressing surface")]
    DecompressSurface(#[from] DecompressSurfaceError),
}

#[derive(Debug, Error)]
pub enum CompressSurfaceError {
    // TODO: Split this into two error types
    #[error("surface dimensions {width} x {height} x {depth} are zero sized or would overflow")]
    InvalidDimensions { width: u32, height: u32, depth: u32 },

    #[error("surface dimensions {width} x {height} x {depth} are not divisibly by the block dimensions {block_width} x {block_height}")]
    NonIntegralDimensionsInBlocks {
        width: u32,
        height: u32,
        depth: u32,
        block_width: u32,
        block_height: u32,
    },

    #[error("expected surface to have at least {expected} bytes but found {actual}")]
    NotEnoughData { expected: usize, actual: usize },

    #[error("compressing data to format {format:?} is not supported")]
    UnsupportedFormat { format: ImageFormat },
}

#[derive(Debug, Error)]
pub enum DecompressSurfaceError {
    #[error("surface dimensions {width} x {height} are not valid")]
    InvalidDimensions { width: u32, height: u32 },

    #[error("expected surface to have at least {expected} bytes but found {actual}")]
    NotEnoughData { expected: usize, actual: usize },

    #[error("the image format of the surface can not be determined")]
    UnrecognizedFormat,
}

fn max_mipmap_count(max_dimension: u32) -> u32 {
    // log2(x) + 1
    u32::BITS - max_dimension.leading_zeros()
}

pub fn mip_dimension(dim: u32, mipmap: u32) -> u32 {
    // Halve for each mip level.
    (dim >> mipmap).max(1)
}

/// Decode all layers and mipmaps from `surface` to RGBA8.
pub fn decode_surface_rgba8<T: AsRef<[u8]>>(
    surface: Surface<T>,
) -> Result<SurfaceRgba8<Vec<u8>>, DecompressSurfaceError> {
    let Surface {
        width,
        height,
        depth,
        layers,
        mipmaps,
        image_format,
        data,
    } = surface;

    // TODO: Add tests for different combinations of layers, mipmaps, and depth.
    // TODO: Make it possible to decode/encode a format known at compile time?
    let block_dimensions = image_format.block_dimensions();
    let block_size_in_bytes = image_format.block_size_in_bytes();

    let mut combined_surface_data = Vec::new();
    for layer in 0..layers {
        for mipmap in 0..mipmaps {
            // TODO: avoid panics in this function.
            let offset = calculate_offset(
                layer,
                mipmap,
                (width, height, depth),
                block_dimensions,
                block_size_in_bytes,
                mipmaps,
            );
            // TODO: Avoid panic here.
            let data = &data.as_ref()[offset..];

            // The mipmap index is already validated by the offset calculation.
            let width = mip_dimension(width, mipmap);
            let height = mip_dimension(height, mipmap);
            let depth = mip_dimension(depth, mipmap);

            // TODO: Avoid additional copies?
            let data = decode_data_rgba8(width, height, depth, image_format, data)?;
            combined_surface_data.extend_from_slice(&data);
        }
    }

    Ok(SurfaceRgba8 {
        width,
        height,
        depth,
        layers,
        mipmaps,
        data: combined_surface_data,
    })
}

fn decode_data_rgba8(
    width: u32,
    height: u32,
    depth: u32,
    image_format: ImageFormat,
    data: &[u8],
) -> Result<Vec<u8>, DecompressSurfaceError> {
    use ImageFormat as F;
    let data = match image_format {
        F::BC1Unorm | F::BC1Srgb => rgba8_from_bcn::<Bc1>(width, height, depth, data),
        F::BC2Unorm | F::BC2Srgb => rgba8_from_bcn::<Bc2>(width, height, depth, data),
        F::BC3Unorm | F::BC3Srgb => rgba8_from_bcn::<Bc3>(width, height, depth, data),
        F::BC4Unorm | F::BC4Snorm => rgba8_from_bcn::<Bc4>(width, height, depth, data),
        F::BC5Unorm | F::BC5Snorm => rgba8_from_bcn::<Bc5>(width, height, depth, data),
        F::BC6Ufloat | F::BC6Sfloat => rgba8_from_bcn::<Bc6>(width, height, depth, data),
        F::BC7Unorm | F::BC7Srgb => rgba8_from_bcn::<Bc7>(width, height, depth, data),
        F::R8Unorm => rgba8_from_r8(width, height, depth, data),
        F::R8G8B8A8Unorm => decode_rgba8_from_rgba8(width, height, depth, data),
        F::R8G8B8A8Srgb => decode_rgba8_from_rgba8(width, height, depth, data),
        F::R32G32B32A32Float => rgba8_from_rgbaf32(width, height, depth, data),
        F::B8G8R8A8Unorm => rgba8_from_bgra8(width, height, depth, data),
        F::B8G8R8A8Srgb => rgba8_from_bgra8(width, height, depth, data),
    }?;
    Ok(data)
}

// TODO: Add documentation showing how to use this.
/// Encode an RGBA8 surface to the given `format`.
///
/// The number of mipmaps generated depends on the `mipmaps` parameter.
/// The `rgba8_data` only needs to contain enough data for the base mip level of `width` x `height` pixels.
pub fn encode_surface_rgba8<T: AsRef<[u8]>>(
    surface: SurfaceRgba8<T>,
    format: ImageFormat,
    quality: Quality,
    mipmaps: Mipmaps,
) -> Result<Surface<Vec<u8>>, CompressSurfaceError> {
    let width = surface.width;
    let height = surface.height;
    let depth = surface.depth;

    // The width and height must be a multiple of the block dimensions.
    // This only applies to the base level.
    let (block_width, block_height, block_depth) = format.block_dimensions();
    if width % block_width != 0 || height % block_height != 0 {
        return Err(CompressSurfaceError::NonIntegralDimensionsInBlocks {
            width,
            height,
            depth,
            block_width,
            block_height,
        });
    }

    // TODO: Encode the correct number of array layers.
    let num_mipmaps = match mipmaps {
        Mipmaps::Disabled => 1,
        Mipmaps::FromSurface => surface.mipmaps,
        Mipmaps::GeneratedExact(count) => count,
        Mipmaps::GeneratedAutomatic => max_mipmap_count(width.max(height).max(depth)),
    };

    let use_surface = mipmaps == Mipmaps::FromSurface;

    // The base mip level is always included.
    let mut surface_data = encode_rgba8(
        width.max(block_width),
        height.max(block_height),
        depth.max(block_depth),
        surface.data.as_ref(),
        format,
        quality,
    )?;

    // TODO: How should the layers be arranged in the surface?
    // TODO: Avoid the initial copy.
    let mut mip_image = surface.data.as_ref().to_vec();

    for mipmap in 1..num_mipmaps {
        // The physical size must be at least a full block.
        // Applications or the GPU will use the smaller virtual size and ignore padding.
        // For example, a 1x1 BCN block still requires 4x4 pixels of data.
        // https://learn.microsoft.com/en-us/windows/win32/direct3d10/d3d10-graphics-programming-guide-resources-block-compression
        let mip_width = mip_dimension(width, mipmap).max(block_width) as usize;
        let mip_height = mip_dimension(height, mipmap).max(block_height) as usize;
        let mip_depth = mip_dimension(depth, mipmap).max(block_depth) as usize;

        // TODO: Find a simpler way to choose a data source.
        mip_image = if use_surface {
            // TODO: Array layers.
            // TODO: Avoid unwrap
            let data = surface.get_image_data(0, mipmap).unwrap();
            let expected_size = mip_width * mip_height * mip_depth * 4;

            if data.len() < expected_size {
                // Zero pad the data to the appropriate size.
                let mut padded_data = vec![0u8; expected_size];
                for z in 0..mip_depth {
                    for y in 0..mip_height {
                        for x in 0..mip_width {
                            // TODO: Make this copy technique a helper function?
                            // TODO: Optimize this for known pixel sizes?
                            // This can't be a memory copy because of the stride.
                            let i = (z * mip_width * mip_height) + y * mip_width + x;
                            padded_data[i] = data[i];
                        }
                    }
                }

                padded_data
            } else {
                data.to_vec()
            }
        } else {
            // Halve the width and height from the previous mip level.
            // This function already handles padding.
            downsample_rgba8(
                mip_width,
                mip_height,
                mip_depth,
                block_width as usize,
                block_height as usize,
                block_depth as usize,
                &mip_image,
            )
        };

        let mip_data = encode_rgba8(
            mip_width as u32,
            mip_height as u32,
            mip_depth as u32,
            &mip_image,
            format,
            quality,
        )?;
        surface_data.extend_from_slice(&mip_data);
    }

    Ok(Surface {
        width,
        height,
        depth,
        layers: 1,
        mipmaps: num_mipmaps,
        image_format: format,
        data: surface_data,
    })
}

fn encode_rgba8(
    width: u32,
    height: u32,
    depth: u32,
    data: &[u8],
    format: ImageFormat,
    quality: Quality,
) -> Result<Vec<u8>, CompressSurfaceError> {
    // TODO: Handle unorm vs srgb for uncompressed or leave the data as is?

    use ImageFormat as F;
    match format {
        F::BC1Unorm | F::BC1Srgb => bcn_from_rgba8::<Bc1>(width, height, depth, data, quality),
        F::BC2Unorm | F::BC2Srgb => bcn_from_rgba8::<Bc2>(width, height, depth, data, quality),
        F::BC3Unorm | F::BC3Srgb => bcn_from_rgba8::<Bc3>(width, height, depth, data, quality),
        F::BC4Unorm | F::BC4Snorm => bcn_from_rgba8::<Bc4>(width, height, depth, data, quality),
        F::BC5Unorm | F::BC5Snorm => bcn_from_rgba8::<Bc5>(width, height, depth, data, quality),
        F::BC6Ufloat | F::BC6Sfloat => bcn_from_rgba8::<Bc6>(width, height, depth, data, quality),
        F::BC7Unorm | F::BC7Srgb => bcn_from_rgba8::<Bc7>(width, height, depth, data, quality),
        F::R8Unorm => r8_from_rgba8(width, height, depth, data),
        F::R8G8B8A8Unorm => encode_rgba8_from_rgba8(width, height, depth, data),
        F::R8G8B8A8Srgb => encode_rgba8_from_rgba8(width, height, depth, data),
        F::R32G32B32A32Float => rgbaf32_from_rgba8(width, height, depth, data),
        F::B8G8R8A8Unorm => bgra8_from_rgba8(width, height, depth, data),
        F::B8G8R8A8Srgb => bgra8_from_rgba8(width, height, depth, data),
    }
}

fn downsample_rgba8(
    width: usize,
    height: usize,
    depth: usize,
    min_width: usize,
    min_height: usize,
    min_depth: usize,
    data: &[u8],
) -> Vec<u8> {
    // Halve the width and height by averaging pixels.
    // This is faster than resizing using the image crate.
    // TODO: How to handle the case where any of the dimensions is zero?
    let new_width = (width / 2).max(min_width);
    let new_height = (height / 2).max(min_height);
    let new_depth = (depth / 2).max(min_depth);

    let mut new_data = vec![0u8; new_width * new_height * new_depth * 4];
    for z in 0..new_depth {
        for x in 0..new_width {
            for y in 0..new_height {
                let new_index = (z * new_width * new_height) + y * new_width + x;

                // Average a 2x2x2 pixel region from data into a 1x1x1 pixel region.
                // This is equivalent to a 3D convolution or pooling operation over the pixels.
                for c in 0..4 {
                    let mut sum = 0;
                    let mut count = 0;
                    for z2 in 0..2 {
                        let sampled_z = (z * 2) + z2;
                        if sampled_z < depth {
                            for y2 in 0..2 {
                                let sampled_y = (y * 2) + y2;
                                if sampled_y < height {
                                    for x2 in 0..2 {
                                        let sampled_x = (x * 2) + x2;
                                        if sampled_x < width {
                                            let index = (sampled_z * width * height)
                                                + (sampled_y * width)
                                                + sampled_x;
                                            sum += data[index * 4 + c] as usize;
                                            count += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    new_data[new_index * 4 + c] = (sum as f64 / count as f64) as u8;
                }
            }
        }
    }

    new_data
}

fn div_round_up(x: usize, d: usize) -> usize {
    (x + d - 1) / d
}

// Surfaces typically use a row-major memory layout like surface[layer][mipmap][z][y][x].
// Not all mipmaps are the same size, so the offset calculation is slightly more complex.
fn calculate_offset(
    layer: u32,
    mipmap: u32,
    dimensions: (u32, u32, u32),
    block_dimensions: (u32, u32, u32),
    block_size_in_bytes: usize,
    mipmaps_per_layer: u32,
) -> usize {
    // TODO: Avoid destructuring this?
    let (width, height, depth) = dimensions;
    let (block_width, block_height, block_depth) = block_dimensions;

    // TODO: Check if mipmap is greater than total mipmaps.
    let mip_sizes: Vec<_> = (0..mipmaps_per_layer)
        .map(|i| {
            let mip_width = mip_dimension(width, i) as usize;
            let mip_height = mip_dimension(height, i) as usize;
            let mip_depth = mip_dimension(depth, i) as usize;

            // TODO: Avoid unwrap.
            mip_size(
                mip_width,
                mip_height,
                mip_depth,
                block_width as usize,
                block_height as usize,
                block_depth as usize,
                block_size_in_bytes,
            )
            .unwrap()
        })
        .collect();

    // Assume mipmaps are tightly packed.
    // This is the case for DDS surface data.
    let layer_size: usize = mip_sizes.iter().sum();

    // Each layer should have the same number of mipmaps.
    let layer_offset = layer as usize * layer_size;
    let mip_offset: usize = mip_sizes[0..mipmap as usize].iter().sum();
    layer_offset + mip_offset
}

fn mip_size(
    width: usize,
    height: usize,
    depth: usize,
    block_width: usize,
    block_height: usize,
    block_depth: usize,
    block_size_in_bytes: usize,
) -> Option<usize> {
    div_round_up(width, block_width)
        .checked_mul(div_round_up(height, block_height))
        .and_then(|v| v.checked_mul(div_round_up(depth, block_depth)))
        .and_then(|v| v.checked_mul(block_size_in_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_mipmap_count_zero() {
        assert_eq!(0, max_mipmap_count(0));
    }

    #[test]
    fn max_mipmap_count_1() {
        assert_eq!(1, max_mipmap_count(1));
    }

    #[test]
    fn max_mipmap_count_4() {
        assert_eq!(4, max_mipmap_count(12));
    }

    #[test]
    fn downsample_rgba8_4x4() {
        // Test that a checkerboard is averaged.
        let original: Vec<_> = std::iter::repeat([0u8, 0u8, 0u8, 0u8, 255u8, 255u8, 255u8, 255u8])
            .take(4 * 4 / 2)
            .flatten()
            .collect();
        assert_eq!(
            vec![127u8; 2 * 2 * 1 * 4],
            downsample_rgba8(4, 4, 1, 1, 1, 1, &original)
        );
    }

    #[test]
    fn downsample_rgba8_3x3() {
        // Test that a checkerboard is averaged.
        let original: Vec<_> = std::iter::repeat([
            0u8, 0u8, 0u8, 0u8, 255u8, 255u8, 255u8, 255u8, 0u8, 0u8, 0u8, 0u8,
        ])
        .take(3 * 3 / 3)
        .flatten()
        .collect();
        assert_eq!(
            vec![127u8; 1 * 1 * 4],
            downsample_rgba8(3, 3, 1, 1, 1, 1, &original)
        );
    }

    #[test]
    fn downsample_rgba8_2x2x2() {
        // Test that two slices of 2x2 pixels are averaged.
        let original = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255,
        ];
        assert_eq!(
            vec![127u8; 1 * 1 * 1 * 4],
            downsample_rgba8(2, 2, 2, 1, 1, 1, &original)
        );
    }

    #[test]
    fn downsample_rgba8_2x2x2_padded() {
        // Test that the output is correctly padded.
        let original = vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255,
        ];
        assert_eq!(
            std::iter::repeat(127u8)
                .take(4)
                .chain(std::iter::repeat(0u8).take(4 * 4 * 2 * 4 - 4))
                .collect::<Vec<_>>(),
            downsample_rgba8(2, 2, 2, 4, 4, 2, &original)
        );
    }

    #[test]
    fn downsample_rgba8_0x0() {
        assert_eq!(vec![0u8; 4], downsample_rgba8(0, 0, 1, 1, 1, 1, &[]));
    }

    #[test]
    fn encode_surface_integral_dimensions() {
        // It's ok for mipmaps to not be divisible by the block width.
        let surface = encode_surface_rgba8(
            SurfaceRgba8 {
                width: 4,
                height: 4,
                depth: 1,
                layers: 1,
                mipmaps: 1,
                data: &[0u8; 64],
            },
            ImageFormat::BC7Srgb,
            Quality::Fast,
            Mipmaps::GeneratedAutomatic,
        )
        .unwrap();

        assert_eq!(4, surface.width);
        assert_eq!(4, surface.height);
        assert_eq!(1, surface.depth);
        assert_eq!(1, surface.layers);
        assert_eq!(3, surface.mipmaps);
        assert_eq!(ImageFormat::BC7Srgb, surface.image_format);
        // Each mipmap must be at least 1 block in size.
        assert_eq!(16 * 3, surface.data.len());
    }

    #[test]
    fn encode_surface_disabled_mipmaps() {
        let surface = encode_surface_rgba8(
            SurfaceRgba8 {
                width: 4,
                height: 4,
                depth: 1,
                layers: 1,
                mipmaps: 3,
                data: &[0u8; 64 + 16 + 4],
            },
            ImageFormat::BC7Srgb,
            Quality::Fast,
            Mipmaps::Disabled,
        )
        .unwrap();

        assert_eq!(4, surface.width);
        assert_eq!(4, surface.height);
        assert_eq!(1, surface.depth);
        assert_eq!(1, surface.layers);
        assert_eq!(1, surface.mipmaps);
        assert_eq!(ImageFormat::BC7Srgb, surface.image_format);
        assert_eq!(16, surface.data.len());
    }

    #[test]
    fn encode_surface_mipmaps_from_surface() {
        let surface = encode_surface_rgba8(
            SurfaceRgba8 {
                width: 4,
                height: 4,
                depth: 1,
                layers: 1,
                mipmaps: 2,
                data: &[0u8; 64 + 16],
            },
            ImageFormat::BC7Srgb,
            Quality::Fast,
            Mipmaps::FromSurface,
        )
        .unwrap();

        assert_eq!(4, surface.width);
        assert_eq!(4, surface.height);
        assert_eq!(1, surface.depth);
        assert_eq!(1, surface.layers);
        assert_eq!(2, surface.mipmaps);
        assert_eq!(ImageFormat::BC7Srgb, surface.image_format);
        assert_eq!(16 * 2, surface.data.len());
    }

    #[test]
    fn encode_surface_non_integral_dimensions() {
        // This should still fail even though there is enough data.
        let result = encode_surface_rgba8(
            SurfaceRgba8 {
                width: 3,
                height: 5,
                depth: 2,
                layers: 1,
                mipmaps: 1,
                data: &[0u8; 256],
            },
            ImageFormat::BC7Srgb,
            Quality::Fast,
            Mipmaps::GeneratedAutomatic,
        );
        assert!(matches!(
            result,
            Err(CompressSurfaceError::NonIntegralDimensionsInBlocks {
                width: 3,
                height: 5,
                depth: 2,
                block_width: 4,
                block_height: 4
            })
        ));
    }

    #[test]
    fn calculate_offset_layer0_mip0() {
        assert_eq!(0, calculate_offset(0, 0, (8, 8, 8), (4, 4, 4), 16, 4));
    }

    #[test]
    fn calculate_offset_layer0_mip2() {
        // The sum of the first 2 mipmaps.
        assert_eq!(
            128 + 16,
            calculate_offset(0, 2, (8, 8, 8), (4, 4, 4), 16, 4)
        );
    }

    #[test]
    fn calculate_offset_layer2_mip0() {
        // The sum of the first 2 array layers.
        // Each mipmap must have at least a full block of data.
        assert_eq!(
            (128 + 16 + 16 + 16) * 2,
            calculate_offset(2, 0, (8, 8, 8), (4, 4, 4), 16, 4)
        );
    }

    #[test]
    fn calculate_offset_layer2_mip2() {
        // The sum of the first two layers and two more mipmaps.
        // Each mipmap must have at least a full block of data.
        assert_eq!(
            (128 + 16 + 16 + 16) * 2 + 128 + 16,
            calculate_offset(2, 2, (8, 8, 8), (4, 4, 4), 16, 4)
        );
    }
}
