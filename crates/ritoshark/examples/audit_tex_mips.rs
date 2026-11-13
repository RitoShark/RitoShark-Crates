use std::{collections::BTreeMap, fs::File, io::BufReader};

use ritoshark::{
    prelude::*,
    tex::{Error, Texture, TextureHeader},
    wad::Wad,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let paths: Vec<_> = std::env::args_os().skip(1).collect();
    if paths.is_empty() {
        return Err("usage: audit_tex_mips <archive.wad.client> ...".into());
    }
    let mut failures = 0;
    for path in paths {
        let mut reader = BufReader::new(File::open(&path)?);
        let wad = Wad::from_reader_toc(&mut reader)?;
        let (mut passed, mut malformed, mut unreadable, mut large) = (0, 0, 0, 0);
        let mut unsupported = BTreeMap::<u8, usize>::new();
        for chunk in &wad.chunks {
            if chunk.uncompressed_size > 64 * 1024 * 1024 {
                large += 1;
                continue;
            }
            let bytes = match wad.chunk_data_from(&mut reader, chunk) {
                Ok(bytes) => bytes,
                Err(error) => {
                    unreadable += 1;
                    eprintln!("{:016x}: unreadable chunk: {error}", chunk.path_hash);
                    continue;
                }
            };
            if !bytes.starts_with(b"TEX\0") {
                continue;
            }
            match Texture::from_bytes(&bytes) {
                Ok(texture) if texture.to_bytes()? == bytes => passed += 1,
                Err(Error::UnsupportedFormat(_)) if bytes.len() >= 12 => {
                    *unsupported.entry(bytes[9]).or_default() += 1;
                    let header = TextureHeader::from_bytes(&bytes)?;
                    eprintln!(
                        "{:016x}: unsupported TEX format={} dimensions={}x{} depth={} resource={} flags={:#04x} payload_bytes={}",
                        chunk.path_hash,
                        bytes[9],
                        header.width,
                        header.height,
                        bytes[8],
                        bytes[10],
                        bytes[11],
                        bytes.len() - 12
                    );
                }
                result => {
                    malformed += 1;
                    match result {
                        Err(error) => eprintln!("{:016x}: {error}", chunk.path_hash),
                        Ok(_) => eprintln!("{:016x}: byte round-trip mismatch", chunk.path_hash),
                    }
                }
            }
        }
        println!(
            "{}: passed={passed} malformed={malformed} unsupported={unsupported:?} unreadable_chunks={unreadable} skipped_large_chunks={large}",
            std::path::Path::new(&path).display()
        );
        failures += malformed + unreadable;
    }
    if failures != 0 {
        return Err(format!("audit found {failures} failures").into());
    }
    Ok(())
}
