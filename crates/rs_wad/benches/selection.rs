use std::hint::black_box;
use std::time::{Duration, Instant};

use rs_wad::{Wad, WadChunk, WadCompression};

fn archive(count: usize) -> Wad {
    let mut wad = Wad {
        version: (3, 4),
        header_trailer: vec![0; 264],
        chunks: (0..count)
            .map(|i| WadChunk {
                path_hash: i as u64 + 1,
                data_offset: 0,
                compressed_size: 128,
                uncompressed_size: 128,
                compression: WadCompression::None,
                is_duplicated: false,
                subchunk_count: 0,
                subchunk_start: 0,
                checksum: 0,
            })
            .collect(),
        data: vec![0x5a; count * 128],
    };
    let start = wad.data_start();
    for (i, chunk) in wad.chunks.iter_mut().enumerate() {
        chunk.data_offset = u32::try_from(start + (i * 128) as u64).unwrap();
    }
    wad
}

fn measure(name: &str, chunks: usize, duration: Duration, mut run: impl FnMut()) {
    run();
    let start = Instant::now();
    let mut iterations = 0;
    loop {
        run();
        iterations += 1;
        if start.elapsed() >= duration {
            break;
        }
    }
    let micros = start.elapsed().as_secs_f64() * 1_000_000.0 / f64::from(iterations);
    println!("{name},{chunks},{iterations},{micros:.3}");
}

fn main() {
    let smoke = std::env::args().any(|arg| arg == "--test");
    let duration = if smoke {
        Duration::ZERO
    } else {
        Duration::from_millis(250)
    };
    println!("operation,chunks,iterations,microseconds_per_iteration");
    for count in [1_000, 10_000, 50_000] {
        let wad = archive(count);
        let requests: Vec<_> = (0..256)
            .map(|i| (i * (count - 1) / 255 + 1) as u64)
            .collect();
        let index = wad.chunk_index();
        assert_eq!(index.len(), count);
        assert_eq!(
            wad.extract_selected(requests.iter().copied())
                .unwrap()
                .len(),
            256
        );
        let all = wad.extract_all().unwrap();
        assert_eq!(all.len(), count);
        assert!(all.values().all(|data| data == &[0x5a; 128]));
        drop(all);

        measure("linear_lookup_256", count, duration, || {
            for hash in &requests {
                black_box(wad.chunk_by_hash(black_box(*hash)));
            }
        });
        measure("indexed_lookup_256", count, duration, || {
            for hash in &requests {
                black_box(index.get(black_box(hash)));
            }
        });
        measure("build_index", count, duration, || {
            black_box(wad.chunk_index());
        });
        measure("extract_selected_256", count, duration, || {
            black_box(
                wad.extract_selected(black_box(requests.iter().copied()))
                    .unwrap(),
            );
        });
        measure("extract_all", count, duration, || {
            black_box(wad.extract_all().unwrap());
        });
    }
}
