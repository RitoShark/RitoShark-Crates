/** Captures the exact on-disk form of an animation so the writer can reproduce the original bytes
verbatim, for every container the reader accepts (uncompressed `r3d2anmd` v3/v4/v5 and compressed
`r3d2canm`). The decoded [`crate::Animation`] keeps human-editable tracks, but several lossy steps
happen on read — the quaternion palette is normalized, the v5 palette ordering is not recoverable
from decoded poses, and compressed keyframes are dequantized and resampled — so the source bytes are
retained alongside to guarantee a byte-exact round-trip. Calling [`crate::Animation::make_editable`]
drops this, after which the writer rebuilds the file from the decoded tracks (emitting v4). */
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawAnim {
    pub bytes: Vec<u8>,
}

pub(crate) fn check_range<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    offset: u64,
    count: usize,
    stride: usize,
) -> crate::Result<()> {
    let here = reader.stream_position().map_err(rs_io::Error::from)?;
    let length = reader
        .seek(std::io::SeekFrom::End(0))
        .map_err(rs_io::Error::from)?;
    reader
        .seek(std::io::SeekFrom::Start(here))
        .map_err(rs_io::Error::from)?;
    let end = (count as u64)
        .checked_mul(stride as u64)
        .and_then(|size| offset.checked_add(size));
    if end.is_none_or(|end| end > length) {
        return Err(crate::Error::InvalidData("section extends beyond the file"));
    }
    Ok(())
}
