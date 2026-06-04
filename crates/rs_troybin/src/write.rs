use std::io::Write;

use rs_io::{Serialize, WriterExt};

use crate::error::{Error, Result};
use crate::troybin::{Bucket, BucketValues, Troybin, TroybinBody, TroybinV1, TroybinV2};

impl Serialize for Troybin {
    type Error = Error;

    fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        writer.write_u8(self.version)?;
        match &self.body {
            TroybinBody::V1(body) => write_v1(writer, body),
            TroybinBody::V2(body) => write_v2(writer, body),
        }
    }
}

fn write_v1<W: Write>(w: &mut W, body: &TroybinV1) -> Result<()> {
    w.write_bytes(&body.header)?;
    w.write_u32(body.entries.len() as u32)?;
    w.write_u32(body.data.len() as u32)?;
    for entry in &body.entries {
        w.write_u32(entry.hash)?;
        w.write_u32(entry.offset)?;
    }
    w.write_bytes(&body.data)?;
    Ok(())
}

fn write_v2<W: Write>(w: &mut W, body: &TroybinV2) -> Result<()> {
    w.write_u16(body.strings_length)?;

    let mut flags = 0u16;
    for bucket in &body.buckets {
        flags |= 1 << bucket.flag_bit;
    }
    if body.flags_zero_prefix {
        w.write_u16(0)?;
    }
    w.write_u16(flags)?;

    for bucket in &body.buckets {
        write_bucket(w, bucket)?;
    }
    Ok(())
}

fn write_bucket<W: Write>(w: &mut W, bucket: &Bucket) -> Result<()> {
    w.write_u16(bucket.hashes.len() as u16)?;
    for &hash in &bucket.hashes {
        w.write_u32(hash)?;
    }

    match &bucket.values {
        BucketValues::I32(v) => v.iter().try_for_each(|&x| w.write_i32(x))?,
        BucketValues::F32(v) => v.iter().try_for_each(|&x| w.write_f32(x))?,
        BucketValues::U8(v) => w.write_bytes(v)?,
        BucketValues::I16(v) => v.iter().try_for_each(|&x| w.write_i16(x))?,
        BucketValues::U16(v) => v.iter().try_for_each(|&x| w.write_u16(x))?,
        BucketValues::Bool(bytes) => w.write_bytes(bytes)?,
        BucketValues::U8x3(v) => v.iter().try_for_each(|a| w.write_bytes(a))?,
        BucketValues::U8x2(v) => v.iter().try_for_each(|a| w.write_bytes(a))?,
        BucketValues::U8x4(v) => v.iter().try_for_each(|a| w.write_bytes(a))?,
        BucketValues::F32x3(v) => write_f32_arrays(w, v)?,
        BucketValues::F32x2(v) => write_f32_arrays(w, v)?,
        BucketValues::F32x4(v) => write_f32_arrays(w, v)?,
        BucketValues::Strings { offsets, blob } => {
            offsets.iter().try_for_each(|&o| w.write_u16(o))?;
            w.write_bytes(blob)?;
        }
    }
    Ok(())
}

fn write_f32_arrays<W: Write, const N: usize>(w: &mut W, arrays: &[[f32; N]]) -> Result<()> {
    for arr in arrays {
        for &x in arr {
            w.write_f32(x)?;
        }
    }
    Ok(())
}
