# Exporting frame-sampled animations

Use the new export API when the application needs explicit format selection,
automatic fallback, and a report of the actual output.

```rust,no_run
use rs_anim::{Animation, AnimationExportOptions, AnimationFormat, Skeleton};
use rs_io::Parse;

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let animation = Animation::from_path("input.anm")?;
let skeleton = Skeleton::from_path("model.skl")?;
let options = AnimationExportOptions::new(AnimationFormat::Canm);
let result = animation.export_to_path("output.anm", &options, Some(&skeleton))?;
println!("Requested {}, wrote {}", result.requested_format, result.format);
for fallback in &result.fallbacks {
    println!("{}: {}", fallback.format, fallback.reason);
}
# Ok(())
# }
```

| Requested format | Attempt order |
| --- | --- |
| `AnimationFormat::Canm` | CANM v1 → ANMD v5 → ANMD v4 |
| `AnimationFormat::V5` | ANMD v5 → ANMD v4 |
| `AnimationFormat::V4` | ANMD v4 |

CANM v1 is a different container from ANMD v1/v5. Selection never means choosing
whichever successful format happens to be smallest. The first format that can be
encoded and pass validation wins. No Aventurine UI is included in this API change.

The input must contain finite transforms, valid nonzero quaternions, unique joint
hashes and equally sized, nonempty tracks sampled from time zero at the stated fps.
The current samples are always encoded, even if the animation retains old source
bytes. The animation is not mutated; ordinary `Serialize::to_bytes` retains its
existing byte-exact behavior.

The default tolerances are 2 degrees rotation, 0.5 translation per component,
0.005 scale per component, and 1 model-space unit. Change the public options fields
to request another quality. All formats, including fallbacks, must pass the same
tolerances. Exact-zero tolerances can fail even for v4 because reading and
interpolation involve float32 operations.

Model-space checks use the optional skeleton. Unanimated joints retain their rest
transforms; animation tracks missing from that skeleton are preserved and checked
locally. `metrics.model_space` is `None` when no skeleton was supplied. It measures
joint origins/unit-axis probes, not the maximum displacement of mesh vertices.

The adaptive CANM fitter checks 96 subintervals/frame and both sides of every clock
cell; finished bytes are checked through playback at 96 and 97 subintervals/frame
and clock-cell edges. Dense fallbacks are read back and checked at 96 and 97
subintervals/frame. These checks are not a continuous mathematical error bound.
The library streams validation poses and keeps one 1-MiB tick cache for fitting.
CANM fitting accepts up to 16,384 tracks and 32,768 input frames; exceeding that
budget tries the dense formats. V4 and v5 still have their own palette/file limits.

`export_to_path` finishes encoding, writes and flushes a temporary file beside the
destination, then atomically replaces it using `rs_io::write_atomic`. Failures before
replacement preserve the destination. A write-time I/O failure is returned, not
retried with another animation format. Replacement uses new-file metadata and does
not guarantee parent-directory durability across power loss.

For Python, the `rs_python` crate exposes the same policy:

```python
import ritoshark as rs

animation = rs.Anm.from_path("input.anm")
skeleton = rs.Skl.from_path("model.skl")
result = animation.export("canm", skeleton=skeleton)
print(result.requested_format, result.format, result.fallbacks, result.metrics)
result.to_path("output.anm")
# result.bytes contains the validated bytes.
```

The older `to_compressed_bytes`, `to_v5_bytes` and normal save methods remain
available with their prior strict behavior and defaults. See
[CANM authoring findings](canm-authoring-findings.md) for the tested input model,
known limits and remaining compression opportunities.
