use std::hint::black_box;
use std::time::Instant;

use rs_audio::Bnk;
use rs_io::Serialize;

fn main() {
    let smoke = std::env::args().any(|arg| arg == "--test");
    let iterations = if smoke { 1 } else { 10 };
    println!("entries,payload_bytes,iterations,sequential_us,batch_us");
    for count in [32u32, 128, 512] {
        let mut original = Bnk::new();
        for id in 0..count {
            original.insert_wem(id, vec![1; 4096]).unwrap();
        }
        let replacements: Vec<_> = (0..count).map(|id| (id, vec![2; 4096])).collect();
        let mut sequential = original.clone();
        for (id, data) in replacements.clone() {
            sequential.replace_wem(id, data).unwrap();
        }
        let mut batch = original.clone();
        batch.replace_wems(replacements.clone()).unwrap();
        assert_eq!(sequential.to_bytes().unwrap(), batch.to_bytes().unwrap());
        let mut totals = [0.0; 2];
        for (method, total) in totals.iter_mut().enumerate() {
            for _ in 0..iterations {
                let mut bank = original.clone();
                let updates = replacements.clone();
                let start = Instant::now();
                if method == 0 {
                    for (id, bytes) in updates {
                        bank.replace_wem(id, bytes).unwrap();
                    }
                } else {
                    bank.replace_wems(updates).unwrap();
                }
                *total += start.elapsed().as_secs_f64();
                black_box(bank);
            }
        }
        println!(
            "{count},4096,{iterations},{:.3},{:.3}",
            totals[0] * 1e6 / iterations as f64,
            totals[1] * 1e6 / iterations as f64
        );
    }
}
