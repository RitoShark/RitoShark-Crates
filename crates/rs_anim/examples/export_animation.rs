//! cargo run -p rs_anim --release --example export_animation -- input.anm output.anm canm [rig.skl]
use rs_anim::{Animation, AnimationExportOptions, AnimationFormat, Skeleton};
use rs_io::Parse;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&args.len()) {
        return Err("usage: input.anm output.anm canm|v5|v4 [rig.skl]".into());
    }
    let format = match args[2].as_str() {
        "canm" => AnimationFormat::Canm,
        "v5" => AnimationFormat::V5,
        "v4" => AnimationFormat::V4,
        _ => return Err("unknown animation format".into()),
    };
    let animation = Animation::from_path(&args[0])?;
    let skeleton = args.get(3).map(Skeleton::from_path).transpose()?;
    let out = animation.export_to_path(
        &args[1],
        &AnimationExportOptions::new(format),
        skeleton.as_ref(),
    )?;
    println!(
        "Requested {}, wrote {} ({} bytes)",
        out.requested_format,
        out.format,
        out.bytes.len()
    );
    for fallback in &out.fallbacks {
        println!("{}: {}", fallback.format, fallback.reason);
    }
    println!("Checked errors: {:?}", out.metrics);
    Ok(())
}
