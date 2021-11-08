use binread::{BinRead, BinResult, ReadOptions};
use bitbuffer::{BitReadStream, LittleEndian};
use bitvec::prelude::*;
use modular_bitfield::prelude::*;
use std::{
    fmt::Debug,
    io::{Read, Seek},
    num::NonZeroU64,
};

use ssbh_write::SsbhWrite;

use ssbh_lib::{Ptr16, Ptr32, Vector3, Vector4};

use super::{Transform, UvTransform};

// The bit_count values for compression types are 64 bits wide.
// This gives a theoretical upper limit of 2^65 - 1 bits for the compressed value.
// The current uncompressed track value types are all 32 bits or smaller.
// Smash Ultimate never uses bit counts above 24, so this gives a sensible representation of u32.
// TODO: It may be helpful to give an error or panic if more than 32 bits are specified for compression.
// TODO: Can we handle arbitrary bit lengths with acceptable performance?
pub type CompressedBits = u32;

// Use the highest bit count used for Smash Ultimate to avoid quality loss.
pub const DEFAULT_F32_BIT_COUNT: u64 = 24;

#[derive(Debug, BinRead, SsbhWrite)]
pub struct CompressedTrackData<T: CompressedData> {
    pub header: CompressedHeader<T>,
    pub compression: T::Compression,
}

#[derive(Debug, BinRead, SsbhWrite)]
pub struct CompressedHeader<T: CompressedData> {
    pub unk_4: u16,              // TODO: Always 4?
    pub flags: CompressionFlags, // TODO: These are used for texture transforms as well?
    pub default_data: Ptr16<T>,
    pub bits_per_entry: u16,
    pub compressed_data: Ptr32<CompressedBuffer>,
    pub frame_count: u32,
}

// TODO: This could be a shared function/type in lib.rs.
fn read_to_end<R: Read + Seek>(reader: &mut R, _ro: &ReadOptions, _: ()) -> BinResult<Vec<u8>> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    Ok(buf)
}

#[derive(Debug, BinRead, SsbhWrite)]
#[ssbhwrite(alignment = 1)] // TODO: Is 1 byte alignment correct?
pub struct CompressedBuffer(#[br(parse_with = read_to_end)] pub Vec<u8>);

// TODO: Investigate these flags more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BitfieldSpecifier)]
#[bits = 2]
pub enum ScaleType {
    None = 0,
    ScaleNoInheritance = 1,
    Scale = 2,
    UniformScale = 3,
}

// TODO: This is redundant with the compression information.
// TODO: Does this determine transform inheritance instead?
// Determines what values are stored in the compressed bit buffer.
// Missing values are determined based on the compression's default values.
#[bitfield(bits = 16)]
#[derive(Debug, BinRead, Clone, Copy)]
#[br(map = Self::from_bytes)]
pub struct CompressionFlags {
    #[bits = 2]
    pub scale_type: ScaleType,
    pub has_rotation: bool,
    pub has_translation: bool,
    #[skip]
    __: B12,
}

ssbh_write::ssbh_write_modular_bitfield_impl!(CompressionFlags, 2);

// Shared logic for compressing track data to and from bits.
pub trait CompressedData: BinRead<Args = ()> + SsbhWrite + Default {
    type Compression: Compression + std::fmt::Debug;
    type BitStore: BitStore;
    type CompressionArgs;

    fn compress(
        &self,
        bits: &mut BitSlice<Lsb0, u8>,
        bit_index: &mut usize,
        compression: &Self::Compression,
        flags: CompressionFlags,
    );

    // TODO: Find a way to do this with bitvec to avoid an extra dependency.
    // TODO: We don't need the entire header here.
    fn decompress(
        stream: &mut BitReadStream<LittleEndian>,
        compression: &Self::Compression,
        default: &Self,
        args: Self::CompressionArgs,
    ) -> bitbuffer::Result<Self>;

    // The size in bytes for the compressed header, default, and a single frame value.
    fn compressed_overhead_in_bytes() -> u64 {
        let header_size = 16;

        // TODO: If SsbhWrite::size_in_bytes didn't take self, we wouldn't need default here.
        // The Vec<T> type currently depends on knowing self.len().
        header_size + Self::default().size_in_bytes() + Self::Compression::default().size_in_bytes()
    }

    fn get_args(header: &CompressedHeader<Self>) -> Self::CompressionArgs;

    fn get_default_and_compression(
        values: &[Self],
        compensate_scale: bool,
    ) -> (Self, Self::Compression);
}

pub trait Compression: BinRead<Args = ()> + SsbhWrite + Default {
    fn bit_count(&self, flags: CompressionFlags) -> u64;
}

pub trait BitReaderExt {
    fn decompress<T: CompressedData>(
        &mut self,
        compression: &T::Compression,
        default: &T,
        args: T::CompressionArgs,
    ) -> bitbuffer::Result<T>;
}

impl<'a> BitReaderExt for BitReadStream<'a, LittleEndian> {
    fn decompress<T: CompressedData>(
        &mut self,
        compression: &T::Compression,
        default: &T,
        args: T::CompressionArgs,
    ) -> bitbuffer::Result<T> {
        T::decompress(self, compression, default, args)
    }
}

#[derive(Debug, BinRead, Clone, SsbhWrite, Default)]
pub struct U32Compression {
    pub min: u32,
    pub max: u32,
    pub bit_count: u64,
}

impl Compression for U32Compression {
    fn bit_count(&self, _: CompressionFlags) -> u64 {
        self.bit_count
    }
}

// bools are always 1 bit.
impl Compression for u128 {
    fn bit_count(&self, _: CompressionFlags) -> u64 {
        1
    }
}

#[derive(Debug, BinRead, Clone, SsbhWrite, Default)]
pub struct F32Compression {
    pub min: f32,
    pub max: f32,
    pub bit_count: u64,
}

impl F32Compression {
    // TODO: Find a better name for this.
    // TODO: Add this to the trait?
    pub fn from_range(min: f32, max: f32) -> Self {
        let bit_count = if min == max { 0 } else { DEFAULT_F32_BIT_COUNT };

        Self {
            min,
            max,
            bit_count,
        }
    }
}

impl Compression for F32Compression {
    fn bit_count(&self, _: CompressionFlags) -> u64 {
        self.bit_count
    }
}

#[derive(Debug, BinRead, SsbhWrite, Default)]
pub struct Vector3Compression {
    pub x: F32Compression,
    pub y: F32Compression,
    pub z: F32Compression,
}

impl Vector3Compression {
    pub fn from_range(min: Vector3, max: Vector3) -> Self {
        Self {
            x: F32Compression::from_range(min.x, max.x),
            y: F32Compression::from_range(min.y, max.y),
            z: F32Compression::from_range(min.z, max.z),
        }
    }
}

impl Compression for Vector3Compression {
    fn bit_count(&self, _: CompressionFlags) -> u64 {
        self.x.bit_count + self.y.bit_count + self.z.bit_count
    }
}

#[derive(Debug, BinRead, SsbhWrite, Default)]
pub struct Vector4Compression {
    pub x: F32Compression,
    pub y: F32Compression,
    pub z: F32Compression,
    pub w: F32Compression,
}

impl Vector4Compression {
    pub fn from_range(min: Vector4, max: Vector4) -> Self {
        Self {
            x: F32Compression::from_range(min.x, max.x),
            y: F32Compression::from_range(min.y, max.y),
            z: F32Compression::from_range(min.z, max.z),
            w: F32Compression::from_range(min.w, max.w),
        }
    }
}

impl Compression for Vector4Compression {
    fn bit_count(&self, _: CompressionFlags) -> u64 {
        self.x.bit_count + self.y.bit_count + self.z.bit_count + self.w.bit_count
    }
}

#[derive(Debug, BinRead, SsbhWrite, Default)]
pub struct TransformCompression {
    // The x component is used for uniform scale.
    pub scale: Vector3Compression,
    // The w component for rotation is handled separately.
    pub rotation: Vector3Compression,
    pub translation: Vector3Compression,
}

// This is also used for compressed transforms but compensate_scale is omitted.
// Compressed transforms set compensate_scale using the header's default value.
#[derive(Debug, BinRead, PartialEq, SsbhWrite, Clone, Copy, Default)]
pub struct UncompressedTransform {
    pub scale: Vector3,
    pub rotation: Vector4,
    pub translation: Vector3,
    // TODO: Does this work the same as Maya's scale compensation?
    // TODO: Does this disable passing on scale?
    pub compensate_scale: u32,
}

impl From<&UncompressedTransform> for Transform {
    fn from(t: &UncompressedTransform) -> Self {
        Self {
            scale: t.scale,
            rotation: t.rotation,
            translation: t.translation,
        }
    }
}

impl UncompressedTransform {
    pub fn from_transform(t: &Transform, compensate_scale: bool) -> Self {
        Self {
            scale: t.scale,
            rotation: t.rotation,
            translation: t.translation,
            compensate_scale: if compensate_scale { 1 } else { 0 },
        }
    }
}

impl Compression for TransformCompression {
    fn bit_count(&self, flags: CompressionFlags) -> u64 {
        let mut bit_count = 0;

        bit_count += self.translation.bit_count(flags);

        match flags.scale_type() {
            ScaleType::Scale | ScaleType::ScaleNoInheritance => {
                bit_count += self.scale.bit_count(flags)
            }
            ScaleType::UniformScale => bit_count += self.scale.x.bit_count,
            _ => (),
        }

        // Three compressed floats and a single sign bit.
        bit_count += self.rotation.bit_count(flags);
        if flags.has_rotation() {
            bit_count += 1;
        }

        bit_count
    }
}

#[derive(Debug, BinRead, SsbhWrite, Default)]
pub struct UvTransformCompression {
    pub scale_u: F32Compression,
    pub scale_v: F32Compression,
    pub rotation: F32Compression,
    pub translate_u: F32Compression,
    pub translate_v: F32Compression,
}

impl Compression for UvTransformCompression {
    fn bit_count(&self, _: CompressionFlags) -> u64 {
        self.scale_u.bit_count
            + self.scale_v.bit_count
            + self.rotation.bit_count
            + self.translate_u.bit_count
            + self.translate_v.bit_count
    }
}

fn calculate_rotation_w(bit_stream: &mut BitReadStream<LittleEndian>, rotation: Vector3) -> f32 {
    // Rotations are encoded as xyzw unit quaternions.
    // For a unit quaternion, x^2 + y^2 + z^2 + w^2 = 1.
    // Solving for the missing w gives two expressions:
    // w = sqrt(1 - x^2 + y^2 + z^2), w = -sqrt(1 - x^2 + y^2 + z^2).
    // Thus, we need only need to store the sign bit to uniquely determine w.
    let flip_w = bit_stream.read_bool().unwrap();

    let w2 = 1.0 - (rotation.x * rotation.x + rotation.y * rotation.y + rotation.z * rotation.z);
    // TODO: Is this the right approach to preventing NaN?
    let w = if w2.is_sign_negative() {
        0.0
    } else {
        w2.sqrt()
    };

    if flip_w {
        -w
    } else {
        w
    }
}

fn read_pattern_index_compressed(
    bit_stream: &mut BitReadStream<bitbuffer::LittleEndian>,
    compression: &U32Compression,
    _default: &u32,
) -> bitbuffer::Result<u32> {
    // TODO: There's only a single track in Smash Ultimate that uses this, so this is just a guess.
    // TODO: How to compress a u32 with min, max, and bitcount?
    let value: u32 = bit_stream.read_int(compression.bit_count as usize)?;
    Ok(value + compression.min)
}

fn read_uv_transform_compressed(
    stream: &mut BitReadStream<bitbuffer::LittleEndian>,
    compression: &UvTransformCompression,
    default: &UvTransform,
    flags: CompressionFlags,
) -> bitbuffer::Result<UvTransform> {
    // UvTransforms use similar logic to Transforms.
    let (scale_u, scale_v) = match flags.scale_type() {
        ScaleType::UniformScale => {
            let uniform_scale = stream.decompress(&compression.scale_u, &default.scale_u, ())?;
            (uniform_scale, uniform_scale)
        }
        _ => {
            let scale_u = stream.decompress(&compression.scale_u, &default.scale_u, ())?;
            let scale_v = stream.decompress(&compression.scale_v, &default.scale_v, ())?;
            (scale_u, scale_v)
        }
    };

    Ok(UvTransform {
        scale_u,
        scale_v,
        // TODO: Do flags affect these values?
        rotation: stream.decompress(&compression.rotation, &default.rotation, ())?,
        translate_u: stream.decompress(&compression.translate_u, &default.translate_u, ())?,
        translate_v: stream.decompress(&compression.translate_v, &default.translate_v, ())?,
    })
}

fn bit_mask(bit_count: NonZeroU64) -> u64 {
    // Get a mask of bit_count many bits set to 1.
    // Don't allow zero to avoid overflow.
    // TODO: handle the case where bit_count is extremely large?
    (1u64 << bit_count.get()) - 1u64
}

fn compress_f32(value: f32, min: f32, max: f32, bit_count: NonZeroU64) -> CompressedBits {
    // The inverse operation of decompression.
    // We don't allow bit_count to be zero.
    // This prevents divide by zero.
    let scale = bit_mask(bit_count);

    // TODO: There could be large errors due to cancellations when the absolute difference of max and min is small.
    // This is likely rare in practice.
    let ratio = (value - min) / (max - min);
    let compressed = ratio * scale as f32;
    compressed as CompressedBits
}

// TODO: It should be possible to test the edge cases by debugging Smash running in an emulator.
// Ex: Create a vector4 animation with all frames set to the same compressed value and inspect the uniform buffer.
fn decompress_f32(value: CompressedBits, min: f32, max: f32, bit_count: NonZeroU64) -> Option<f32> {
    // Anim supports custom ranges and non standard bit counts for fine tuning compression.
    // Unsigned normalized u8 would use min: 0.0, max: 1.0, and bit_count: 8.
    // This produces 2 ^ 8 evenly spaced floating point values between 0.0 and 1.0,
    // so 0b00000000 corresponds to 0.0 and 0b11111111 corresponds to 1.0.

    // Bit count can't be zero, which prevents divide by zero below.
    let scale = bit_mask(bit_count);

    // TODO: There may be some edge cases with this implementation of linear interpolation.
    // TODO: What happens when value > scale?
    let lerp = |a, b, t| a * (1.0 - t) + b * t;
    let value = lerp(min, max, value as f32 / scale as f32);

    Some(value)
}

fn read_compressed_f32(
    bit_stream: &mut BitReadStream<bitbuffer::LittleEndian>,
    compression: &F32Compression,
) -> bitbuffer::Result<Option<f32>> {
    match NonZeroU64::new(compression.bit_count as u64) {
        Some(bit_count) => {
            if compression.min == compression.max {
                Ok(None)
            } else {
                let value = bit_stream.read_int(bit_count.get() as usize)?;
                Ok(decompress_f32(
                    value,
                    compression.min,
                    compression.max,
                    bit_count,
                ))
            }
        }
        None => Ok(None),
    }
}

fn read_transform_compressed(
    stream: &mut BitReadStream<LittleEndian>,
    compression: &TransformCompression,
    default: &UncompressedTransform,
    flags: CompressionFlags,
) -> bitbuffer::Result<UncompressedTransform> {
    let scale = match flags.scale_type() {
        ScaleType::UniformScale => {
            let uniform_scale = stream.decompress(&compression.scale.x, &default.scale.x, ())?;
            Vector3::new(uniform_scale, uniform_scale, uniform_scale)
        }
        _ => stream.decompress(&compression.scale, &default.scale, ())?,
    };

    let rotation_xyz = stream.decompress(&compression.rotation, &default.rotation.xyz(), ())?;
    let translation = stream.decompress(&compression.translation, &default.translation, ())?;
    let rotation_w = if flags.has_rotation() {
        calculate_rotation_w(stream, rotation_xyz)
    } else {
        default.rotation.w
    };

    Ok(UncompressedTransform {
        scale,
        rotation: Vector4::new(rotation_xyz.x, rotation_xyz.y, rotation_xyz.z, rotation_w),
        translation,
        // Compressed transforms don't allow specifying compensate scale per frame.
        compensate_scale: default.compensate_scale,
    })
}

impl CompressedData for UncompressedTransform {
    type Compression = TransformCompression;
    type BitStore = u32;
    type CompressionArgs = CompressionFlags;

    fn decompress(
        stream: &mut BitReadStream<LittleEndian>,
        compression: &Self::Compression,
        default: &Self,
        args: Self::CompressionArgs,
    ) -> bitbuffer::Result<Self> {
        read_transform_compressed(stream, compression, default, args)
    }

    fn compress(
        &self,
        bits: &mut BitSlice<Lsb0, u8>,
        bit_index: &mut usize,
        compression: &Self::Compression,
        flags: CompressionFlags,
    ) {
        match flags.scale_type() {
            // TODO: There's no way to access this value from the public API?
            // TODO: Find a way to expose scale inheritance.
            // TODO: Test different scale types and flags for writing.
            ScaleType::Scale | ScaleType::ScaleNoInheritance => {
                self.scale
                    .compress(bits, bit_index, &compression.scale, flags);
            }
            ScaleType::UniformScale => {
                self.scale
                    .x
                    .compress(bits, bit_index, &compression.scale.x, flags);
            }
            _ => (),
        }

        if flags.has_rotation() {
            self.rotation
                .xyz()
                .compress(bits, bit_index, &compression.rotation, flags);
        }

        if flags.has_translation() {
            self.translation
                .compress(bits, bit_index, &compression.translation, flags);
        }

        if flags.has_rotation() {
            // Add a single sign bit instead of storing w explicitly.
            *bits.get_mut(*bit_index).unwrap() = self.rotation.w.is_sign_negative();
            *bit_index += 1;
        }
    }

    fn get_args(header: &CompressedHeader<Self>) -> Self::CompressionArgs {
        header.flags
    }

    fn get_default_and_compression(
        values: &[Self],
        compensate_scale: bool,
    ) -> (Self, Self::Compression) {
        let min_scale = find_min_vector3(values.iter().map(|v| &v.scale));
        let max_scale = find_max_vector3(values.iter().map(|v| &v.scale));

        let min_rotation = find_min_vector4(values.iter().map(|v| &v.rotation));
        let max_rotation = find_max_vector4(values.iter().map(|v| &v.rotation));

        let min_translation = find_min_vector3(values.iter().map(|v| &v.translation));
        let max_translation = find_max_vector3(values.iter().map(|v| &v.translation));

        (
            UncompressedTransform {
                scale: min_scale,
                rotation: min_rotation, // TODO: How to choose a default quaternion?
                translation: min_translation,
                // Set to 1 if any of the values are 1.
                // TODO: Is it possible to preserve per frame compensate scale for compressed transforms?
                compensate_scale: if compensate_scale { 1 } else { 0 },
            },
            TransformCompression {
                scale: Vector3Compression::from_range(min_scale, max_scale),
                rotation: Vector3Compression::from_range(min_rotation.xyz(), max_rotation.xyz()),
                translation: Vector3Compression::from_range(min_translation, max_translation),
            },
        )
    }
}

impl CompressedData for UvTransform {
    type Compression = UvTransformCompression;
    type BitStore = u32;
    type CompressionArgs = CompressionFlags;

    fn decompress(
        stream: &mut BitReadStream<LittleEndian>,
        compression: &Self::Compression,
        default: &Self,
        args: Self::CompressionArgs,
    ) -> bitbuffer::Result<Self> {
        read_uv_transform_compressed(stream, compression, default, args)
    }

    fn compress(
        &self,
        bits: &mut BitSlice<Lsb0, u8>,
        bit_index: &mut usize,
        compression: &Self::Compression,
        flags: CompressionFlags,
    ) {
        self.scale_u
            .compress(bits, bit_index, &compression.scale_u, flags);
        self.scale_v
            .compress(bits, bit_index, &compression.scale_v, flags);
        self.rotation
            .compress(bits, bit_index, &compression.rotation, flags);
        self.translate_u
            .compress(bits, bit_index, &compression.translate_u, flags);
        self.translate_v
            .compress(bits, bit_index, &compression.translate_v, flags);
    }

    fn get_args(header: &CompressedHeader<Self>) -> Self::CompressionArgs {
        header.flags
    }

    fn get_default_and_compression(values: &[Self], _: bool) -> (Self, Self::Compression) {
        // TODO: How to determine the default?
        let min_scale_u = find_min_f32(values.iter().map(|v| &v.scale_u));
        let max_scale_u = find_max_f32(values.iter().map(|v| &v.scale_u));

        let min_scale_v = find_min_f32(values.iter().map(|v| &v.scale_v));
        let max_scale_v = find_max_f32(values.iter().map(|v| &v.scale_v));

        let min_rotation = find_min_f32(values.iter().map(|v| &v.rotation));
        let max_rotation = find_max_f32(values.iter().map(|v| &v.rotation));

        let min_translate_u = find_min_f32(values.iter().map(|v| &v.translate_u));
        let max_translate_u = find_max_f32(values.iter().map(|v| &v.translate_u));

        let min_translate_v = find_min_f32(values.iter().map(|v| &v.translate_v));
        let max_translate_v = find_max_f32(values.iter().map(|v| &v.translate_v));

        (
            UvTransform {
                scale_u: min_scale_u,
                scale_v: min_scale_v,
                rotation: min_rotation,
                translate_u: min_translate_u,
                translate_v: min_translate_v,
            },
            UvTransformCompression {
                scale_u: F32Compression::from_range(min_scale_u, max_scale_u),
                scale_v: F32Compression::from_range(min_scale_v, max_scale_v),
                rotation: F32Compression::from_range(min_rotation, max_rotation),
                translate_u: F32Compression::from_range(min_translate_u, max_translate_u),
                translate_v: F32Compression::from_range(min_translate_v, max_translate_v),
            },
        )
    }
}

impl CompressedData for Vector3 {
    type Compression = Vector3Compression;
    type BitStore = u32;
    type CompressionArgs = ();

    fn decompress(
        stream: &mut BitReadStream<LittleEndian>,
        compression: &Self::Compression,
        default: &Self,
        _args: (),
    ) -> bitbuffer::Result<Self> {
        Ok(Self {
            x: stream.decompress(&compression.x, &default.x, ())?,
            y: stream.decompress(&compression.y, &default.y, ())?,
            z: stream.decompress(&compression.z, &default.z, ())?,
        })
    }

    fn compress(
        &self,
        bits: &mut BitSlice<Lsb0, u8>,
        bit_index: &mut usize,
        compression: &Self::Compression,
        flags: CompressionFlags,
    ) {
        self.x.compress(bits, bit_index, &compression.x, flags);
        self.y.compress(bits, bit_index, &compression.y, flags);
        self.z.compress(bits, bit_index, &compression.z, flags);
    }

    fn get_args(_: &CompressedHeader<Self>) -> Self::CompressionArgs {}

    fn get_default_and_compression(values: &[Self], _: bool) -> (Self, Self::Compression) {
        let min = find_min_vector3(values.iter());
        let max = find_max_vector3(values.iter());

        // TODO: Is this the best default?
        (min, Vector3Compression::from_range(min, max))
    }
}

// Return the value that isn't NaN for min and max.
fn find_min_f32<'a, I: Iterator<Item = &'a f32>>(values: I) -> f32 {
    values.copied().reduce(f32::min).unwrap_or(0.0)
}

fn find_max_f32<'a, I: Iterator<Item = &'a f32>>(values: I) -> f32 {
    values.copied().reduce(f32::max).unwrap_or(0.0)
}

fn find_min_vector3<'a, I: Iterator<Item = &'a Vector3>>(values: I) -> Vector3 {
    values
        .copied()
        .reduce(Vector3::min)
        .unwrap_or(Vector3::ZERO)
}

fn find_max_vector3<'a, I: Iterator<Item = &'a Vector3>>(values: I) -> Vector3 {
    values
        .copied()
        .reduce(Vector3::max)
        .unwrap_or(Vector3::ZERO)
}

fn find_min_vector4<'a, I: Iterator<Item = &'a Vector4>>(values: I) -> Vector4 {
    values
        .copied()
        .reduce(Vector4::min)
        .unwrap_or(Vector4::ZERO)
}

fn find_max_vector4<'a, I: Iterator<Item = &'a Vector4>>(values: I) -> Vector4 {
    values
        .copied()
        .reduce(Vector4::max)
        .unwrap_or(Vector4::ZERO)
}

impl CompressedData for Vector4 {
    type Compression = Vector4Compression;
    type BitStore = u32;
    type CompressionArgs = ();

    fn decompress(
        stream: &mut BitReadStream<LittleEndian>,
        compression: &Self::Compression,
        default: &Self,
        _args: (),
    ) -> bitbuffer::Result<Self> {
        Ok(Vector4 {
            x: stream.decompress(&compression.x, &default.x, ())?,
            y: stream.decompress(&compression.y, &default.y, ())?,
            z: stream.decompress(&compression.z, &default.z, ())?,
            w: stream.decompress(&compression.w, &default.w, ())?,
        })
    }

    fn compress(
        &self,
        bits: &mut BitSlice<Lsb0, u8>,
        bit_index: &mut usize,
        compression: &Self::Compression,
        flags: CompressionFlags,
    ) {
        self.x.compress(bits, bit_index, &compression.x, flags);
        self.y.compress(bits, bit_index, &compression.y, flags);
        self.z.compress(bits, bit_index, &compression.z, flags);
        self.w.compress(bits, bit_index, &compression.w, flags);
    }

    fn get_args(_: &CompressedHeader<Self>) -> Self::CompressionArgs {}

    fn get_default_and_compression(values: &[Self], _: bool) -> (Self, Self::Compression) {
        let min = find_min_vector4(values.iter());
        let max = find_max_vector4(values.iter());

        // TODO: Is this the best default?
        (min, Vector4Compression::from_range(min, max))
    }
}

// TODO: Create a newtype for PatternIndex(u32)?
impl CompressedData for u32 {
    type Compression = U32Compression;
    type BitStore = u32;
    type CompressionArgs = ();

    fn decompress(
        stream: &mut BitReadStream<LittleEndian>,
        compression: &Self::Compression,
        default: &Self,
        _: Self::CompressionArgs,
    ) -> bitbuffer::Result<Self> {
        read_pattern_index_compressed(stream, compression, default)
    }

    fn compress(
        &self,
        bits: &mut BitSlice<Lsb0, u8>,
        bit_index: &mut usize,
        compression: &Self::Compression,
        _flags: CompressionFlags,
    ) {
        // TODO: This is just a guess.
        // TODO: Add a test case?
        let compressed_value = self - compression.min;
        bits[*bit_index..*bit_index + compression.bit_count as usize].store_le(compressed_value);
        *bit_index += compression.bit_count as usize;
    }

    fn get_args(_: &CompressedHeader<Self>) -> Self::CompressionArgs {}

    fn get_default_and_compression(values: &[Self], _: bool) -> (Self, Self::Compression) {
        (
            0, // TODO: Better default?
            U32Compression {
                min: values.iter().copied().min().unwrap_or(0),
                max: values.iter().copied().max().unwrap_or(0),
                bit_count: super::compression::DEFAULT_F32_BIT_COUNT, // TODO: How should this work for u32?
            },
        )
    }
}

impl CompressedData for f32 {
    type Compression = F32Compression;
    type BitStore = u32;
    type CompressionArgs = ();

    fn decompress(
        stream: &mut BitReadStream<LittleEndian>,
        compression: &Self::Compression,
        default: &Self,
        _args: Self::CompressionArgs,
    ) -> bitbuffer::Result<Self> {
        Ok(read_compressed_f32(stream, compression)?.unwrap_or(*default))
    }

    fn compress(
        &self,
        bits: &mut BitSlice<Lsb0, u8>,
        bit_index: &mut usize,
        compression: &Self::Compression,
        _flags: CompressionFlags,
    ) {
        if let Some(bit_count) = NonZeroU64::new(compression.bit_count as u64) {
            let compressed_value = compress_f32(*self, compression.min, compression.max, bit_count);
            bits[*bit_index..*bit_index + compression.bit_count as usize]
                .store_le(compressed_value);
            *bit_index += compression.bit_count as usize;
        }
    }

    fn get_args(_: &CompressedHeader<Self>) -> Self::CompressionArgs {}

    fn get_default_and_compression(values: &[Self], _: bool) -> (Self, Self::Compression) {
        let min = find_min_f32(values.iter());
        let max = find_max_f32(values.iter());
        (
            min, // TODO: f32 default for compression?
            F32Compression::from_range(min, max),
        )
    }
}

#[derive(Debug, BinRead, SsbhWrite, Default, PartialEq, Eq, Clone, Copy)]
pub struct Boolean(pub u8);

impl From<bool> for Boolean {
    fn from(v: bool) -> Self {
        Self::from(&v)
    }
}

impl From<&bool> for Boolean {
    fn from(v: &bool) -> Self {
        if *v {
            Self(1u8)
        } else {
            Self(0u8)
        }
    }
}

impl From<&Boolean> for bool {
    fn from(v: &Boolean) -> Self {
        v.0 != 0u8
    }
}

impl From<Boolean> for bool {
    fn from(v: Boolean) -> Self {
        Self::from(&v)
    }
}

impl CompressedData for Boolean {
    // There are 16 bytes for determining the compression, but all bytes are set to 0.
    type Compression = u128;
    type BitStore = u8;
    type CompressionArgs = usize;

    fn decompress(
        stream: &mut BitReadStream<LittleEndian>,
        _compression: &Self::Compression,
        _default: &Self,
        bits_per_entry: Self::CompressionArgs,
    ) -> bitbuffer::Result<Self> {
        // Boolean compression is based on bits per entry, which is usually set to 1 bit.
        // TODO: 0 bits uses the default?
        let value = stream.read_int::<u8>(bits_per_entry)?;
        Ok(Boolean(value))
    }

    fn compress(
        &self,
        bits: &mut BitSlice<Lsb0, u8>,
        bit_index: &mut usize,
        _: &Self::Compression,
        _: CompressionFlags,
    ) {
        *bits.get_mut(*bit_index).unwrap() = self.into();
        *bit_index += 1;
    }

    fn get_args(header: &CompressedHeader<Self>) -> Self::CompressionArgs {
        header.bits_per_entry as usize
    }

    fn get_default_and_compression(_: &[Self], _: bool) -> (Self, Self::Compression) {
        // TODO: Should booleans always default to false?
        (Boolean(0u8), 0)
    }
}

#[cfg(test)]
mod tests {
    use bitbuffer::BitReadBuffer;

    use super::*;

    #[test]
    fn bit_masks() {
        assert_eq!(0b1u64, bit_mask(NonZeroU64::new(1).unwrap()));
        assert_eq!(0b11u64, bit_mask(NonZeroU64::new(2).unwrap()));
        assert_eq!(0b111111111u64, bit_mask(NonZeroU64::new(9).unwrap()));
    }

    #[test]
    fn compress_float_8bit() {
        let bit_count = NonZeroU64::new(8).unwrap();
        for i in 0..=255u8 {
            assert_eq!(
                i as CompressedBits,
                compress_f32(i as f32 / u8::MAX as f32, 0.0, 1.0, bit_count)
            );
        }
    }

    #[test]
    fn decompress_float_8bit() {
        let bit_count = NonZeroU64::new(8).unwrap();
        for i in 0..=255u8 {
            assert_eq!(
                Some(i as f32 / u8::MAX as f32),
                decompress_f32(i as CompressedBits, 0.0, 1.0, bit_count)
            );
        }
    }

    #[test]
    fn decompress_float_14bit() {
        // stage/poke_unova/battle/motion/s13_a, D_lightning_B, CustomVector3
        assert_eq!(
            Some(1.254_003_3),
            decompress_f32(2350, 0.0, 8.74227, NonZeroU64::new(14).unwrap())
        );
        assert_eq!(
            Some(1.185_819_5),
            decompress_f32(2654, 0.0, 7.32, NonZeroU64::new(14).unwrap())
        );
        assert_eq!(
            Some(2.964_048_1),
            decompress_f32(2428, 0.0, 20.0, NonZeroU64::new(14).unwrap())
        );
        assert_eq!(
            Some(1.218_784_5),
            decompress_f32(2284, 0.0, 8.74227, NonZeroU64::new(14).unwrap())
        );
    }

    #[test]
    fn compress_float_14bit() {
        // stage/poke_unova/battle/motion/s13_a, D_lightning_B, CustomVector3
        assert_eq!(
            2350,
            compress_f32(1.254_003_3, 0.0, 8.74227, NonZeroU64::new(14).unwrap())
        );
        assert_eq!(
            2654,
            compress_f32(1.185_819_5, 0.0, 7.32, NonZeroU64::new(14).unwrap())
        );
        assert_eq!(
            2428,
            compress_f32(2.964_048_1, 0.0, 20.0, NonZeroU64::new(14).unwrap())
        );
        assert_eq!(
            2284,
            compress_f32(1.218_784_5, 0.0, 8.74227, NonZeroU64::new(14).unwrap())
        );
    }

    #[test]
    fn compress_decompress_float_24bit() {
        assert_eq!(
            bit_mask(NonZeroU64::new(24).unwrap()) as CompressedBits,
            compress_f32(1.0, -1.0, 1.0, NonZeroU64::new(24).unwrap())
        );

        assert_eq!(
            1.0,
            decompress_f32(
                bit_mask(NonZeroU64::new(24).unwrap()) as CompressedBits,
                -1.0,
                1.0,
                NonZeroU64::new(24).unwrap()
            )
            .unwrap()
        );
    }

    #[test]
    fn calculate_rotation_w_unit_quaternion_true() {
        let bit_buffer = BitReadBuffer::new(&[1u8], bitbuffer::LittleEndian);
        let mut bit_reader = BitReadStream::new(bit_buffer);
        assert_eq!(
            0.0,
            calculate_rotation_w(&mut bit_reader, Vector3::new(1.0, 0.0, 0.0))
        );
    }

    #[test]
    fn calculate_rotation_w_non_unit_quaternion_true() {
        let bit_buffer = BitReadBuffer::new(&[1u8], bitbuffer::LittleEndian);
        let mut bit_reader = BitReadStream::new(bit_buffer);

        // W isn't well defined in this case.
        // Just assume W is 0.0 when the square root would be negative.
        // TODO: There may be a better approach with better animation quality.
        assert_eq!(
            0.0,
            calculate_rotation_w(&mut bit_reader, Vector3::new(1.0, 1.0, 1.0))
        );
    }

    #[test]
    fn calculate_rotation_w_unit_quaternion_false() {
        let bit_buffer = BitReadBuffer::new(&[0u8], bitbuffer::LittleEndian);
        let mut bit_reader = BitReadStream::new(bit_buffer);
        assert_eq!(
            0.0,
            calculate_rotation_w(&mut bit_reader, Vector3::new(1.0, 0.0, 0.0))
        );
    }

    #[test]
    fn calculate_rotation_w_non_unit_quaternion_false() {
        let bit_buffer = BitReadBuffer::new(&[0u8], bitbuffer::LittleEndian);
        let mut bit_reader = BitReadStream::new(bit_buffer);

        // W isn't well defined in this case.
        // Just assume W is 0.0 when the square root would be negative.
        // TODO: There may be a better approach with better animation quality.
        assert_eq!(
            0.0,
            calculate_rotation_w(&mut bit_reader, Vector3::new(1.0, 1.0, 1.0))
        );
    }
}
