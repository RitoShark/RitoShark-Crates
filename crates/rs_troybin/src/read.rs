use std::io::{Read, Seek};

use rs_io::{Parse, ReaderExt};

use crate::error::{Error, Result};
use crate::troybin::{Bucket, BucketValues, Troybin, TroybinBody, TroybinV1, TroybinV2, V1Entry};

impl Parse for Troybin {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let version = reader.read_u8()?;
        let body = match version {
            1 => TroybinBody::V1(read_v1(reader)?),
            2 => TroybinBody::V2(read_v2(reader)?),
            other => return Err(Error::UnsupportedVersion(other)),
        };
        ensure_consumed(reader)?;
        Ok(Troybin { version, body })
    }
}

fn read_v1<R: Read>(r: &mut R) -> Result<TroybinV1> {
    let header = r.read_array::<3>()?;
    let entry_count = r.read_u32()? as usize;
    let data_count = r.read_u32()? as usize;

    let mut entries = Vec::with_capacity(entry_count);
    for _ in 0..entry_count {
        let hash = r.read_u32()?;
        let offset = r.read_u32()?;
        entries.push(V1Entry { hash, offset });
    }
    let data = r.read_bytes(data_count)?;
    Ok(TroybinV1 {
        header,
        entries,
        data,
    })
}

fn read_v2<R: Read>(r: &mut R) -> Result<TroybinV2> {
    let strings_length = r.read_u16()?;
    let mut flags = r.read_u16()?;
    let flags_zero_prefix = flags == 0;
    if flags_zero_prefix {
        flags = r.read_u16()?;
    }

    let mut buckets = Vec::new();
    for bit in 0u8..16 {
        if flags & (1 << bit) == 0 {
            continue;
        }
        buckets.push(read_bucket(r, bit, strings_length as usize)?);
    }

    Ok(TroybinV2 {
        strings_length,
        flags_zero_prefix,
        buckets,
    })
}

fn read_bucket<R: Read>(r: &mut R, bit: u8, strings_length: usize) -> Result<Bucket> {
    let count = r.read_u16()? as usize;
    let mut hashes = Vec::with_capacity(count);
    for _ in 0..count {
        hashes.push(r.read_u32()?);
    }

    let values = match bit {
        0 | 13 => BucketValues::I32(read_scalars(r, count, ReaderExt::read_i32)?),
        1 => BucketValues::F32(read_scalars(r, count, ReaderExt::read_f32)?),
        2 | 4 => BucketValues::U8(read_scalars(r, count, ReaderExt::read_u8)?),
        3 => BucketValues::I16(read_scalars(r, count, ReaderExt::read_i16)?),
        5 => BucketValues::Bool(r.read_bytes(count.div_ceil(8))?),
        6 => BucketValues::U8x3(read_u8_arrays::<3, R>(r, count)?),
        7 => BucketValues::F32x3(read_f32_arrays::<3, R>(r, count)?),
        8 => BucketValues::U8x2(read_u8_arrays::<2, R>(r, count)?),
        9 => BucketValues::F32x2(read_f32_arrays::<2, R>(r, count)?),
        10 => BucketValues::U8x4(read_u8_arrays::<4, R>(r, count)?),
        11 => BucketValues::F32x4(read_f32_arrays::<4, R>(r, count)?),
        12 => {
            let offsets = read_scalars(r, count, ReaderExt::read_u16)?;
            let blob = r.read_bytes(strings_length)?;
            BucketValues::Strings { offsets, blob }
        }
        other => return Err(Error::UnsupportedBucket(other)),
    };

    Ok(Bucket {
        flag_bit: bit,
        hashes,
        values,
    })
}

fn read_scalars<R: Read, T>(
    r: &mut R,
    count: usize,
    read: impl Fn(&mut R) -> rs_io::Result<T>,
) -> Result<Vec<T>> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(read(r)?);
    }
    Ok(out)
}

fn read_u8_arrays<const N: usize, R: Read>(r: &mut R, count: usize) -> Result<Vec<[u8; N]>> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(r.read_array::<N>()?);
    }
    Ok(out)
}

fn read_f32_arrays<const N: usize, R: Read>(r: &mut R, count: usize) -> Result<Vec<[f32; N]>> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut a = [0f32; N];
        for slot in &mut a {
            *slot = r.read_f32()?;
        }
        out.push(a);
    }
    Ok(out)
}

fn ensure_consumed<R: Read>(r: &mut R) -> Result<()> {
    let mut rest = Vec::new();
    r.read_to_end(&mut rest)?;
    if rest.is_empty() {
        Ok(())
    } else {
        Err(Error::TrailingBytes(rest.len()))
    }
}
