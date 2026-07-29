use std::io::{Read, Write};

use rs_math::{Quat, Rgba, Vec2, Vec3, Vec4};

use crate::Result;

/// Reading helpers layered over any [`std::io::Read`]; every integer is little-endian.
pub trait ReaderExt: Read {
    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_byte_array::<1>()?[0])
    }

    fn read_i8(&mut self) -> Result<i8> {
        Ok(self.read_byte_array::<1>()?[0] as i8)
    }

    fn read_u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.read_byte_array()?))
    }

    fn read_i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.read_byte_array()?))
    }

    fn read_u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.read_byte_array()?))
    }

    fn read_i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.read_byte_array()?))
    }

    fn read_u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.read_byte_array()?))
    }

    fn read_i64(&mut self) -> Result<i64> {
        Ok(i64::from_le_bytes(self.read_byte_array()?))
    }

    fn read_f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.read_byte_array()?))
    }

    fn read_f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.read_byte_array()?))
    }

    fn read_bool(&mut self) -> Result<bool> {
        Ok(self.read_u8()? != 0)
    }

    fn read_byte_array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut buf = [0u8; N];
        self.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn read_bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        const CHUNK: usize = 64 * 1024;
        let mut buf = Vec::new();
        let mut remaining = n;
        while remaining > 0 {
            let want = remaining.min(CHUNK);
            let start = buf.len();
            buf.resize(start + want, 0u8);
            self.read_exact(&mut buf[start..])?;
            remaining -= want;
        }
        Ok(buf)
    }

    fn read_string_u16(&mut self) -> Result<String> {
        let len = self.read_u16()? as usize;
        let bytes = self.read_bytes(len)?;
        Ok(String::from_utf8(bytes)?)
    }

    fn read_string_u32(&mut self) -> Result<String> {
        let len = self.read_u32()? as usize;
        let bytes = self.read_bytes(len)?;
        Ok(String::from_utf8(bytes)?)
    }

    fn read_cstring(&mut self) -> Result<String> {
        let mut bytes = Vec::new();
        loop {
            let b = self.read_u8()?;
            if b == 0 {
                break;
            }
            bytes.push(b);
        }
        Ok(String::from_utf8(bytes)?)
    }

    fn read_fixed_string<const N: usize>(&mut self) -> Result<String> {
        let buf = self.read_byte_array::<N>()?;
        let end = buf.iter().position(|&b| b == 0).unwrap_or(N);
        Ok(String::from_utf8(buf[..end].to_vec())?)
    }

    fn read_vec2(&mut self) -> Result<Vec2> {
        Ok(Vec2::new(self.read_f32()?, self.read_f32()?))
    }

    fn read_vec3(&mut self) -> Result<Vec3> {
        Ok(Vec3::new(
            self.read_f32()?,
            self.read_f32()?,
            self.read_f32()?,
        ))
    }

    fn read_vec4(&mut self) -> Result<Vec4> {
        Ok(Vec4::new(
            self.read_f32()?,
            self.read_f32()?,
            self.read_f32()?,
            self.read_f32()?,
        ))
    }

    fn read_quat(&mut self) -> Result<Quat> {
        Ok(Quat::from_array([
            self.read_f32()?,
            self.read_f32()?,
            self.read_f32()?,
            self.read_f32()?,
        ]))
    }

    fn read_mtx44(&mut self) -> Result<[f32; 16]> {
        let mut m = [0.0f32; 16];
        for slot in &mut m {
            *slot = self.read_f32()?;
        }
        Ok(m)
    }

    fn read_rgba(&mut self) -> Result<Rgba> {
        Ok(Rgba::new(
            self.read_u8()?,
            self.read_u8()?,
            self.read_u8()?,
            self.read_u8()?,
        ))
    }
}

impl<R: Read + ?Sized> ReaderExt for R {}

/// Writing helpers layered over any [`std::io::Write`]; every integer is little-endian.
pub trait WriterExt: Write {
    fn write_u8(&mut self, v: u8) -> Result<()> {
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_i8(&mut self, v: i8) -> Result<()> {
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_u16(&mut self, v: u16) -> Result<()> {
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_i16(&mut self, v: i16) -> Result<()> {
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_u32(&mut self, v: u32) -> Result<()> {
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_i32(&mut self, v: i32) -> Result<()> {
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_u64(&mut self, v: u64) -> Result<()> {
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_i64(&mut self, v: i64) -> Result<()> {
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_f32(&mut self, v: f32) -> Result<()> {
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_f64(&mut self, v: f64) -> Result<()> {
        self.write_bytes(&v.to_le_bytes())
    }

    fn write_bool(&mut self, v: bool) -> Result<()> {
        self.write_u8(v as u8)
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        self.write_all(bytes)?;
        Ok(())
    }

    fn write_string_u16(&mut self, s: &str) -> Result<()> {
        self.write_u16(s.len() as u16)?;
        self.write_bytes(s.as_bytes())
    }

    fn write_string_u32(&mut self, s: &str) -> Result<()> {
        self.write_u32(s.len() as u32)?;
        self.write_bytes(s.as_bytes())
    }

    fn write_cstring(&mut self, s: &str) -> Result<()> {
        self.write_bytes(s.as_bytes())?;
        self.write_u8(0)
    }

    fn write_vec2(&mut self, v: Vec2) -> Result<()> {
        let [x, y] = v.to_array();
        self.write_f32(x)?;
        self.write_f32(y)
    }

    fn write_vec3(&mut self, v: Vec3) -> Result<()> {
        let [x, y, z] = v.to_array();
        self.write_f32(x)?;
        self.write_f32(y)?;
        self.write_f32(z)
    }

    fn write_vec4(&mut self, v: Vec4) -> Result<()> {
        let [x, y, z, w] = v.to_array();
        self.write_f32(x)?;
        self.write_f32(y)?;
        self.write_f32(z)?;
        self.write_f32(w)
    }

    fn write_quat(&mut self, q: Quat) -> Result<()> {
        let [x, y, z, w] = q.to_array();
        self.write_f32(x)?;
        self.write_f32(y)?;
        self.write_f32(z)?;
        self.write_f32(w)
    }

    fn write_mtx44(&mut self, m: &[f32; 16]) -> Result<()> {
        for &v in m {
            self.write_f32(v)?;
        }
        Ok(())
    }

    fn write_rgba(&mut self, c: Rgba) -> Result<()> {
        self.write_u8(c.r)?;
        self.write_u8(c.g)?;
        self.write_u8(c.b)?;
        self.write_u8(c.a)
    }
}

impl<W: Write + ?Sized> WriterExt for W {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn round_trip_u32() {
        let mut c = Cursor::new(Vec::new());
        c.write_u32(0xDEAD_BEEF).unwrap();
        c.set_position(0);
        assert_eq!(c.read_u32().unwrap(), 0xDEAD_BEEF);
    }

    #[test]
    fn round_trip_f32() {
        let mut c = Cursor::new(Vec::new());
        c.write_f32(1.5).unwrap();
        c.set_position(0);
        assert_eq!(c.read_f32().unwrap(), 1.5);
    }

    #[test]
    fn round_trip_string_u16() {
        let mut c = Cursor::new(Vec::new());
        c.write_string_u16("ritoshark").unwrap();
        c.set_position(0);
        assert_eq!(c.read_string_u16().unwrap(), "ritoshark");
    }

    #[test]
    fn round_trip_vec3() {
        let v = Vec3::new(1.0, -2.0, 3.5);
        let mut c = Cursor::new(Vec::new());
        c.write_vec3(v).unwrap();
        c.set_position(0);
        assert_eq!(c.read_vec3().unwrap(), v);
    }

    #[test]
    fn read_bytes_hostile_size_errors_without_aborting() {
        let mut c = Cursor::new(vec![0u8; 10]);
        let err = c.read_bytes(0xFFFF_FF00).unwrap_err();
        let crate::Error::Io(io_err) = err else {
            panic!("expected Error::Io, got {err:?}");
        };
        assert_eq!(io_err.kind(), std::io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn read_bytes_hostile_size_never_allocates_past_stream_end() {
        struct CountingReader<'a> {
            data: &'a [u8],
            pos: usize,
            max_buf_seen: usize,
        }

        impl Read for CountingReader<'_> {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                self.max_buf_seen = self.max_buf_seen.max(out.len());
                let remaining = &self.data[self.pos..];
                let n = remaining.len().min(out.len());
                out[..n].copy_from_slice(&remaining[..n]);
                self.pos += n;
                Ok(n)
            }
        }

        let mut r = CountingReader {
            data: &[0u8; 10],
            pos: 0,
            max_buf_seen: 0,
        };
        let result = r.read_bytes(usize::MAX - 1);
        assert!(result.is_err());
        assert!(
            r.max_buf_seen <= 64 * 1024,
            "single read request exceeded the chunk cap: {}",
            r.max_buf_seen
        );
    }

    #[test]
    fn read_bytes_exact_available_length() {
        let data = vec![1u8, 2, 3, 4, 5];
        let mut c = Cursor::new(data.clone());
        assert_eq!(c.read_bytes(5).unwrap(), data);
    }

    #[test]
    fn read_bytes_zero() {
        let mut c = Cursor::new(vec![1u8, 2, 3]);
        assert_eq!(c.read_bytes(0).unwrap(), Vec::<u8>::new());
        assert_eq!(c.read_bytes(3).unwrap(), vec![1u8, 2, 3]);
    }

    #[test]
    fn read_bytes_partial_available_errors() {
        let mut c = Cursor::new(vec![1u8, 2, 3]);
        assert!(c.read_bytes(10).is_err());
    }

    #[test]
    fn read_string_u16_hostile_length_errors() {
        let mut data = Vec::new();
        data.write_u16(0xFFFF).unwrap();
        let mut c = Cursor::new(data);
        assert!(c.read_string_u16().is_err());
    }
}
