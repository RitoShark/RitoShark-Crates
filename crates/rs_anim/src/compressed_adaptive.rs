//! Adaptive CANM writer for frame-sampled animations.
//! Fitting uses midpoint tick samples; acceptance uses continuous frame interpolation,
//! both clock-cell edges and two independent subframe grids through the actual decoder.
use super::*;
use crate::animation_export::{AnimationExportMetrics, AnimationExportOptions};
use rs_math::Mat4;
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap};
use std::io::Cursor;

#[path = "compressed_curve.rs"]
mod curve;
use curve::*;
#[path = "compressed_adaptive_checks.rs"]
mod checks;
pub(crate) use checks::validate_dense;
use checks::*;

#[derive(Clone, Copy)]
struct Quality {
    local: [f32; 3],
    model: f32,
}
fn finite_error(kind: ChannelKind, a: Vec4, b: Vec4) -> f32 {
    if !a.is_finite() || !b.is_finite() {
        return f32::INFINITY;
    }
    let e = kind.error(a, b);
    if e.is_finite() { e } else { f32::INFINITY }
}
fn ratio(error: f32, tolerance: f32) -> f32 {
    if error == 0. {
        0.
    } else if tolerance == 0. {
        f32::INFINITY
    } else {
        error / tolerance
    }
}

struct Source<'a> {
    hashes: Vec<u32>,
    bounds: [Vec3; 4],
    duration: f32,
    fps: f32,
    frames: usize,
    dense: &'a Animation,
}
impl<'a> Source<'a> {
    fn new(dense: &'a Animation) -> Self {
        Self {
            hashes: dense.tracks.iter().map(|t| t.joint_hash).collect(),
            bounds: [Vec3::ZERO; 4],
            duration: (dense.frame_count() - 1) as f32 * (1. / dense.fps),
            fps: dense.fps,
            frames: dense.frame_count(),
            dense,
        }
    }
    fn unpack(&self, kind: ChannelKind, bytes: &[u8; 6]) -> Vec4 {
        match kind {
            ChannelKind::Rotation => decompress_quat(bytes).into(),
            ChannelKind::Translation => {
                decompress_vec3(self.bounds[0], self.bounds[1], bytes).extend(0.)
            }
            ChannelKind::Scale => decompress_vec3(self.bounds[2], self.bounds[3], bytes).extend(0.),
        }
    }
    fn pack(&self, kind: ChannelKind, value: Vec4) -> Key {
        let bytes = match kind {
            ChannelKind::Rotation => compress_quat(Quat::from_vec4(value).normalize()),
            ChannelKind::Translation => {
                pack_vector(value.truncate(), self.bounds[0], self.bounds[1])
            }
            ChannelKind::Scale => pack_vector(value.truncate(), self.bounds[2], self.bounds[3]),
        };
        Key { time: 0, bytes }
    }
    fn times(&self, steps: usize) -> Vec<u16> {
        let dt = 1. / (self.fps * steps as f32);
        (0..(self.frames - 1) * steps + 1)
            .map(|i| crate::compressed::compress_time(i as f32 * dt, self.duration))
            .collect()
    }
    fn emit(&self, channels: &[Channel]) -> Result<Vec<u8>> {
        serialize_channels(
            channels,
            &self.hashes,
            self.duration,
            self.fps,
            self.frames,
            self.bounds,
        )
    }
    fn sample_dense_seconds(&self, channel: usize, seconds: f32) -> Vec4 {
        sample_frame_channel(self.dense, channel, seconds)
    }
}

struct Oracle<'a> {
    dense: &'a Animation,
    duration: f32,
    cache: RefCell<Option<(usize, Vec<Vec4>)>>,
}
impl<'a> Oracle<'a> {
    fn new(src: &Source<'a>) -> Self {
        Self {
            dense: src.dense,
            duration: src.duration,
            cache: RefCell::new(None),
        }
    }
    fn prepare_channel(&self, c: usize) {
        let mut cache = self.cache.borrow_mut();
        if let Some((channel, values)) = cache.as_mut() {
            *channel = c;
            values.fill(Vec4::NAN);
        } else {
            *cache = Some((c, vec![Vec4::NAN; 65536]));
        }
    }
    fn sample(&self, c: usize, t: u16) -> Vec4 {
        let mut cache = self.cache.borrow_mut();
        if let Some((channel, values)) = cache.as_ref()
            && *channel == c
            && values[t as usize].is_finite()
        {
            return values[t as usize];
        }
        let seconds = if t == 0 {
            0.
        } else if t == 65535 {
            self.duration
        } else {
            ((t as f64 + 0.5) / 65535. * self.duration as f64) as f32
        };
        let value = sample_frame_channel(self.dense, c, seconds);
        if let Some((channel, values)) = cache.as_mut()
            && *channel == c
        {
            values[t as usize] = value;
        }
        value
    }
    fn reference(&self, src: &Source, c: usize, _tick: u16, seconds: f32) -> Vec4 {
        src.sample_dense_seconds(c, seconds)
    }
    fn bounds(&self) -> [Vec3; 4] {
        let mut bounds = [
            Vec3::splat(f32::INFINITY),
            Vec3::splat(f32::NEG_INFINITY),
            Vec3::splat(f32::INFINITY),
            Vec3::splat(f32::NEG_INFINITY),
        ];
        // Match the reviewed writer's midpoint samples without storing them for every bone.
        for t in 0..=65535u32 {
            let seconds = if t == 0 {
                0.
            } else if t == 65535 {
                self.duration
            } else {
                ((t as f64 + 0.5) / 65535. * self.duration as f64) as f32
            };
            for j in 0..self.dense.tracks.len() {
                let p = sample_frame_channel(self.dense, j * 3 + 1, seconds).truncate();
                let s = sample_frame_channel(self.dense, j * 3 + 2, seconds).truncate();
                bounds[0] = bounds[0].min(p);
                bounds[1] = bounds[1].max(p);
                bounds[2] = bounds[2].min(s);
                bounds[3] = bounds[3].max(s);
            }
        }
        bounds
    }
}
fn sample_frame_channel(dense: &Animation, channel: usize, seconds: f32) -> Vec4 {
    let t = &dense.tracks[channel / 3];
    let x = (seconds * dense.fps).min((t.frames.len() - 1) as f32);
    let i = x.floor() as usize;
    let u = x - i as f32;
    let a = t.frames[i];
    let b = t.frames[(i + 1).min(t.frames.len() - 1)];
    match channel % 3 {
        0 => a
            .rotation
            .normalize()
            .slerp(b.rotation.normalize(), u)
            .normalize()
            .into(),
        1 => a.translation.lerp(b.translation, u).extend(0.),
        _ => a.scale.lerp(b.scale, u).extend(0.),
    }
}
fn window(t: u16, frames: usize) -> usize {
    ((t as usize * (frames - 1)) / 65535).min(frames - 1)
}
fn kind(c: usize) -> ChannelKind {
    match c % 3 {
        0 => ChannelKind::Rotation,
        1 => ChannelKind::Translation,
        _ => ChannelKind::Scale,
    }
}
fn packed(src: &Source, oracle: &Oracle, c: usize, time: u16) -> Key {
    let mut key = src.pack(kind(c), oracle.sample(c, time));
    key.time = time;
    key
}
fn value_at(keys: &[Key], values: &[Vec4], kind: ChannelKind, t: u16) -> Vec4 {
    let i = keys.partition_point(|k| k.time <= t).saturating_sub(1);
    let ids = [
        i.saturating_sub(1),
        i,
        (i + 1).min(keys.len() - 1),
        (i + 2).min(keys.len() - 1),
    ];
    kind.sample(t, ids.map(|i| keys[i].time), ids.map(|i| values[i]))
}
fn make_curve(src: &Source, oracle: &Oracle, c: usize, keys: Vec<Key>, audit: &[u16]) -> Curve {
    let n = keys.len();
    Curve {
        kind: kind(c),
        values: keys.iter().map(|k| src.unpack(kind(c), &k.bytes)).collect(),
        keys,
        expected: audit.iter().map(|&t| oracle.sample(c, t)).collect(),
        prev: (0..n).map(|i| i.saturating_sub(1)).collect(),
        next: (0..n).map(|i| (i + 1).min(n - 1)).collect(),
        active: vec![true; n],
        versions: vec![0; n],
        alive: n,
    }
}
fn removal_ratio(
    curve: &mut Curve,
    node: usize,
    amount: usize,
    audit: &[u16],
    limits: &[f32],
) -> f32 {
    let last = if amount == 1 { node } else { curve.next[node] };
    let a = curve.prev[node];
    let b = curve.next[last];
    let start = curve.prev[a];
    let end = curve.next[b];
    curve.next[a] = b;
    curve.prev[b] = a;
    let lo = audit.partition_point(|&t| t < curve.keys[start].time);
    let hi = audit.partition_point(|&t| t <= curve.keys[end].time);
    let mut key = start;
    let mut worst = 0f32;
    for (s, &time) in audit.iter().enumerate().take(hi).skip(lo) {
        while curve.next[key] != key && curve.keys[curve.next[key]].time <= time {
            key = curve.next[key];
        }
        let ids = [
            curve.prev[key],
            key,
            curve.next[key],
            curve.next[curve.next[key]],
        ];
        let actual = curve.kind.sample(
            time,
            ids.map(|i| curve.keys[i].time),
            ids.map(|i| curve.values[i]),
        );
        worst = worst.max(
            finite_error(curve.kind, curve.expected[s], actual)
                / limits[window(time, limits.len())].max(f32::MIN_POSITIVE),
        );
        if worst > 1. {
            break;
        }
    }
    curve.next[a] = node;
    curve.prev[b] = last;
    worst
}
fn simplify(mut curve: Curve, audit: &[u16], limits: &[f32]) -> Vec<Key> {
    let mut heap = BinaryHeap::new();
    let enqueue = |curve: &mut Curve, heap: &mut BinaryHeap<_>, i: usize| {
        heap.push((
            Reverse(ordered_cost(curve.cost(i, audit))),
            Reverse(i),
            curve.versions[i],
            1usize,
        ));
        if curve.can_remove_pair(i) {
            heap.push((
                Reverse(ordered_cost(curve.pair_cost(i, audit))),
                Reverse(i),
                curve.versions[i],
                2,
            ));
        }
    };
    for i in 1..curve.keys.len().saturating_sub(1) {
        enqueue(&mut curve, &mut heap, i);
    }
    while let Some((_, Reverse(node), version, amount)) = heap.pop() {
        if !curve.active[node]
            || curve.versions[node] != version
            || (amount == 2 && !curve.can_remove_pair(node))
        {
            continue;
        }
        if removal_ratio(&mut curve, node, amount, audit, limits) > 1. {
            continue;
        }
        let second = curve.next[node];
        let mut changed = curve.remove(node);
        if amount == 2 {
            changed.extend(curve.remove(second));
            changed.sort_unstable();
            changed.dedup();
            changed.retain(|&n| curve.active[n]);
        }
        for n in changed {
            enqueue(&mut curve, &mut heap, n);
        }
    }
    curve
        .keys
        .iter()
        .zip(curve.active)
        .filter_map(|(&k, a)| a.then_some(k))
        .collect()
}
fn polish(
    src: &Source,
    oracle: &Oracle,
    c: usize,
    keys: &mut [Key],
    audit: &[u16],
    limits: &[f32],
    only_violations: bool,
) -> usize {
    if keys.len() < 3 {
        return 0;
    }
    let mut values: Vec<_> = keys.iter().map(|k| src.unpack(kind(c), &k.bytes)).collect();
    let mut groups = Vec::new();
    let mut i = 0;
    while i < keys.len() {
        let start = i;
        i += 1;
        while i < keys.len() && keys[i].time == keys[start].time {
            i += 1;
        }
        groups.push((start, i));
    }
    let mut moved = 0;
    for (start, end) in groups {
        if start == 0 || end == keys.len() {
            continue;
        }
        let lo = keys[start - 1].time;
        let hi = keys[end].time;
        if hi - lo < 2 {
            continue;
        }
        let begin = keys[start.saturating_sub(2)].time;
        let finish = keys[(end + 1).min(keys.len() - 1)].time;
        let aa = audit.partition_point(|&t| t < begin);
        let bb = audit.partition_point(|&t| t <= finish);
        let score = |keys: &[Key], values: &[Vec4]| {
            let mut worst = 0f32;
            let mut sum = 0f64;
            for &t in &audit[aa..bb] {
                let e = finite_error(
                    kind(c),
                    oracle.sample(c, t),
                    value_at(keys, values, kind(c), t),
                ) / limits[window(t, limits.len())].max(f32::MIN_POSITIVE);
                worst = worst.max(e);
                sum += (e as f64).powi(2);
            }
            (worst, sum)
        };
        let old = keys[start];
        let mut best_key = old;
        let mut best = score(keys, &values);
        if only_violations && best.0 <= 1.0 {
            continue;
        }
        let mut choices = vec![old.time.saturating_sub(1), old.time.saturating_add(1)];
        for part in 1..8 {
            choices.push((lo as u32 + (hi - lo) as u32 * part / 8) as u16);
        }
        choices.sort_unstable();
        choices.dedup();
        for time in choices {
            if time <= lo || time >= hi || time == old.time {
                continue;
            }
            let candidate = packed(src, oracle, c, time);
            keys[start..end].fill(candidate);
            values[start..end].fill(src.unpack(kind(c), &candidate.bytes));
            let s = score(keys, &values);
            if s.0 < best.0 - 1e-5 || (s.0 <= best.0 + 1e-5 && s.1 < best.1) {
                best = s;
                best_key = candidate;
            }
        }
        keys[start..end].fill(best_key);
        values[start..end].fill(src.unpack(kind(c), &best_key.bytes));
        moved += usize::from(best_key.time != old.time);
    }
    moved
}
fn fit_curve(
    src: &Source,
    oracle: &Oracle,
    c: usize,
    audit: &[u16],
    limits: &[f32],
    previous: Option<&[Key]>,
) -> (Channel, usize, usize) {
    oracle.prepare_channel(c);
    let fit = src.times(2);
    let samples: Vec<_> = fit.iter().map(|&t| packed(src, oracle, c, t)).collect();
    let constant = audit.iter().all(|&t| {
        finite_error(
            kind(c),
            oracle.sample(c, t),
            src.unpack(kind(c), &samples[0].bytes),
        ) <= limits[window(t, limits.len())]
    });
    if constant {
        return (
            Channel {
                joint: (c / 3) as u16,
                kind: kind(c),
                keys: vec![samples[0]],
            },
            0,
            0,
        );
    }
    let last = samples.len() - 1;
    let mut keys: Vec<_> = if let Some(previous) = previous {
        let mut v = previous.to_vec();
        if v.len() > 1 {
            v.pop();
        }
        v
    } else {
        samples
            .into_iter()
            .enumerate()
            .flat_map(|(i, k)| std::iter::repeat_n(k, if i == 0 || i == last { 1 } else { 2 }))
            .collect()
    };
    let mut added = 0;
    let mut moved = 0;
    // A previously constant channel has no terminal knot. Once refinement
    // makes it dynamic, restore both endpoint anchors before simplifying.
    // The runtime uses a moving four-key window and must never extrapolate
    // beyond an early last knot merely because a clamp-only estimator does.
    if keys.first().unwrap().time != 0 {
        keys.insert(0, packed(src, oracle, c, 0));
    }
    if keys.last().unwrap().time != 65535 {
        keys.push(packed(src, oracle, c, 65535));
    }
    for round in 0..4 {
        keys = simplify(make_curve(src, oracle, c, keys, audit), audit, limits);
        moved += polish(src, oracle, c, &mut keys, audit, limits, previous.is_some());
        keys = simplify(make_curve(src, oracle, c, keys, audit), audit, limits);
        let values: Vec<_> = keys.iter().map(|k| src.unpack(kind(c), &k.bytes)).collect();
        let mut bad: BTreeMap<usize, (f32, u16)> = BTreeMap::new();
        for &t in audit {
            let e = finite_error(
                kind(c),
                oracle.sample(c, t),
                value_at(&keys, &values, kind(c), t),
            ) / limits[window(t, limits.len())].max(f32::MIN_POSITIVE);
            if e > 1. {
                let interval = keys.partition_point(|k| k.time <= t).saturating_sub(1);
                let item = bad.entry(interval).or_insert((0., t));
                if e > item.0 {
                    *item = (e, t);
                }
            }
        }
        if bad.is_empty() || round == 3 {
            break;
        }
        let mut additions = Vec::new();
        for (_, (_, t)) in bad {
            if keys.iter().any(|k| k.time == t) {
                continue;
            }
            let key = packed(src, oracle, c, t);
            additions.extend([key, key]);
        }
        if additions.is_empty() {
            break;
        }
        added += additions.len();
        keys.extend(additions);
        keys.sort_by_key(|k| k.time);
    }
    keys.push(*keys.last().unwrap());
    (
        Channel {
            joint: (c / 3) as u16,
            kind: kind(c),
            keys,
        },
        moved,
        added,
    )
}
struct Rig {
    order: Vec<usize>,
    tracks: Vec<Option<usize>>,
}
impl Rig {
    fn new(src: &Source, skl: &Skeleton) -> Self {
        let map: HashMap<_, _> = src
            .hashes
            .iter()
            .enumerate()
            .map(|(i, &h)| (h, i))
            .collect();
        let tracks = skl
            .joints
            .iter()
            .map(|j| map.get(&j.hash).copied())
            .collect();
        let mut order = Vec::new();
        let mut seen = vec![false; skl.joints.len()];
        while order.len() < seen.len() {
            let n = order.len();
            for (i, j) in skl.joints.iter().enumerate() {
                if !seen[i] && (j.parent_id < 0 || seen[j.parent_id as usize]) {
                    seen[i] = true;
                    order.push(i);
                }
            }
            assert!(order.len() > n);
        }
        Self { order, tracks }
    }
}
fn matrix(values: [Vec4; 3]) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        values[2].truncate(),
        Quat::from_vec4(values[0]).normalize(),
        values[1].truncate(),
    )
}
fn initial_limits(
    src: &Source,
    oracle: &Oracle,
    skl: &Skeleton,
    rig: &Rig,
    q: Quality,
) -> Vec<Vec<f32>> {
    let mut weights = vec![vec![0f32; src.frames]; src.hashes.len() * 3];
    let mut world = vec![Mat4::IDENTITY; skl.joints.len()];
    let mut local = world.clone();
    let mut scales = vec![Vec3::ONE; skl.joints.len()];
    for tick in src.times(2) {
        let w = window(tick, src.frames);
        for &j in &rig.order {
            let joint = &skl.joints[j];
            local[j] = if let Some(i) = rig.tracks[j] {
                let v = std::array::from_fn(|c| oracle.sample(i * 3 + c, tick));
                scales[j] = v[2].truncate();
                matrix(v)
            } else {
                scales[j] = joint.local_scale;
                joint.local_transform()
            };
            world[j] = if joint.parent_id < 0 {
                local[j]
            } else {
                world[joint.parent_id as usize] * local[j]
            };
        }
        for child in 0..skl.joints.len() {
            for mut p in [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z] {
                let mut next = Some(child);
                while let Some(j) = next {
                    let parent = skl.joints[j].parent_id;
                    let m = if parent < 0 {
                        Mat4::IDENTITY
                    } else {
                        world[parent as usize]
                    };
                    let norm = (m.x_axis.truncate().length_squared()
                        + m.y_axis.truncate().length_squared()
                        + m.z_axis.truncate().length_squared())
                    .sqrt();
                    if let Some(i) = rig.tracks[j] {
                        let a = [norm * (scales[j] * p).length(), norm, norm * p.length()];
                        for c in 0..3 {
                            for weight in &mut weights[i * 3 + c]
                                [w.saturating_sub(1)..=(w + 1).min(src.frames - 1)]
                            {
                                *weight = weight.max(a[c]);
                            }
                        }
                    }
                    p = local[j].transform_point3(p);
                    next = (parent >= 0).then_some(parent as usize);
                }
            }
        }
    }
    weights
        .iter()
        .enumerate()
        .map(|(c, values)| {
            values
                .iter()
                .map(|&w| q.local[c % 3].min(q.model / w.max(1e-6)) * 0.8)
                .collect()
        })
        .collect()
}
#[derive(Clone, Copy)]
struct Hot {
    joint: usize,
    tick: u16,
    seconds: f32,
    point: Vec3,
    error: f32,
}
struct Scan {
    local: [f32; 3],
    probe: f32,
    updates: BTreeMap<(usize, usize), f32>,
}
#[allow(clippy::too_many_arguments)]
fn scan(
    src: &Source,
    oracle: &Oracle,
    skl: &Skeleton,
    rig: &Rig,
    channels: &[Channel],
    resolution: usize,
    limits: &[Vec<f32>],
    q: Quality,
) -> Scan {
    let values: Vec<Vec<_>> = channels
        .iter()
        .map(|c| {
            c.keys
                .iter()
                .map(|k| src.unpack(c.kind, &k.bytes))
                .collect()
        })
        .collect();
    let eval = |c: usize, t: u16| value_at(&channels[c].keys, &values[c], kind(c), t);
    let mut s = Scan {
        local: [0.; 3],
        probe: 0.,
        updates: BTreeMap::new(),
    };
    let mut hotspots: BTreeMap<(usize, usize), Hot> = BTreeMap::new();
    let mut actual = vec![[Vec4::ZERO; 3]; src.hashes.len()];
    let mut reference = actual.clone();
    let mut worlds = [
        vec![Mat4::IDENTITY; skl.joints.len()],
        vec![Mat4::IDENTITY; skl.joints.len()],
    ];
    let mut tighten = |c: usize, t: u16, e: f32| {
        let w = window(t, src.frames);
        let new = (e * 0.55).max(1e-7);
        for (ww, &limit) in limits[c]
            .iter()
            .enumerate()
            .take((w + 1).min(src.frames - 1) + 1)
            .skip(w.saturating_sub(1))
        {
            let bound = (limit * 0.65).min(new);
            s.updates
                .entry((c, ww))
                .and_modify(|v| *v = v.min(bound))
                .or_insert(bound);
        }
    };
    let rate = src.fps * resolution as f32;
    let mut samples: Vec<_> = (0..(src.duration * rate).round() as usize + 1)
        .map(|i| {
            let seconds = i as f32 * (1. / rate);
            (
                crate::compressed::compress_time(seconds, src.duration),
                seconds,
            )
        })
        .collect();
    if resolution >= 96 {
        // The fitting and validation grids need not land in the same part of
        // a clock cell. Check both ends of every cell, retaining the true
        // floating-point compress_time mapping near boundaries.
        for tick in 0..65535u32 {
            for fraction in [0.001, 0.999] {
                let seconds = ((tick as f64 + fraction) / 65535. * src.duration as f64) as f32;
                samples.push((
                    crate::compressed::compress_time(seconds, src.duration),
                    seconds,
                ));
            }
        }
    }
    for (t, seconds) in samples {
        for i in 0..src.hashes.len() {
            for c in 0..3 {
                reference[i][c] = oracle.reference(src, i * 3 + c, t, seconds);
                actual[i][c] = eval(i * 3 + c, t);
                let e = finite_error(kind(c), reference[i][c], actual[i][c]);
                s.local[c] = s.local[c].max(e);
                if e > q.local[c] * 0.98 {
                    tighten(i * 3 + c, t, e);
                }
            }
        }
        for &j in &rig.order {
            let joint = &skl.joints[j];
            for (side, v) in [&reference, &actual].iter().enumerate() {
                let local = rig.tracks[j].map_or_else(|| joint.local_transform(), |i| matrix(v[i]));
                worlds[side][j] = if joint.parent_id < 0 {
                    local
                } else {
                    worlds[side][joint.parent_id as usize] * local
                };
            }
            for p in [Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z] {
                let e = worlds[0][j]
                    .transform_point3(p)
                    .distance(worlds[1][j].transform_point3(p));
                let e = if e.is_finite() { e } else { f32::INFINITY };
                s.probe = s.probe.max(e);
                if e > q.model * 0.98 {
                    let hot = Hot {
                        joint: j,
                        tick: t,
                        seconds,
                        point: p,
                        error: e,
                    };
                    hotspots
                        .entry((window(t, src.frames), j))
                        .and_modify(|h| {
                            if e > h.error {
                                *h = hot
                            }
                        })
                        .or_insert(hot);
                }
            }
        }
    }
    let mut hot: Vec<_> = hotspots.into_values().collect();
    hot.sort_by(|a, b| b.error.total_cmp(&a.error));
    let mut per_window = BTreeMap::<usize, usize>::new();
    for h in hot
        .into_iter()
        .filter(|h| {
            let n = per_window.entry(window(h.tick, src.frames)).or_default();
            *n += 1;
            *n <= 4
        })
        .take(1024)
    {
        let mut chain = vec![h.joint];
        while skl.joints[*chain.last().unwrap()].parent_id >= 0 {
            chain.push(skl.joints[*chain.last().unwrap()].parent_id as usize);
        }
        chain.reverse();
        let actual: Vec<[Vec4; 3]> = chain
            .iter()
            .map(|&j| {
                rig.tracks[j].map_or([Vec4::ZERO; 3], |i| {
                    std::array::from_fn(|c| eval(i * 3 + c, h.tick))
                })
            })
            .collect();
        let reference: Vec<[Vec4; 3]> = chain
            .iter()
            .map(|&j| {
                rig.tracks[j].map_or([Vec4::ZERO; 3], |i| {
                    std::array::from_fn(|c| oracle.reference(src, i * 3 + c, h.tick, h.seconds))
                })
            })
            .collect();
        let product = |fix: Option<(usize, usize)>, all: bool| {
            let mut global = Mat4::IDENTITY;
            for (index, &j) in chain.iter().enumerate() {
                let local = if rig.tracks[j].is_some() {
                    let mut v = if all { reference[index] } else { actual[index] };
                    if let Some((a, c)) = fix
                        && a == index
                    {
                        v[c] = reference[index][c];
                    }
                    matrix(v)
                } else {
                    skl.joints[j].local_transform()
                };
                global *= local;
            }
            global.transform_point3(h.point)
        };
        let target = product(None, true);
        let mut best = 0f32;
        let mut gains = Vec::new();
        for (index, &j) in chain.iter().enumerate() {
            if let Some(i) = rig.tracks[j] {
                for c in 0..3 {
                    let gain = h.error - product(Some((index, c)), false).distance(target);
                    if gain > 1e-6 {
                        best = best.max(gain);
                        gains.push((i * 3 + c, index, c, gain));
                    }
                }
            }
        }
        for (c, index, k, gain) in gains {
            if gain >= best * 0.5 {
                tighten(
                    c,
                    h.tick,
                    finite_error(kind(c), reference[index][k], actual[index][k]),
                );
            }
        }
    }
    s
}
fn fit(src: &Source, oracle: &Oracle, skl: &Skeleton, q: Quality) -> Result<(Vec<u8>, f32)> {
    let rig = Rig::new(src, skl);
    let mut limits = initial_limits(src, oracle, skl, &rig, q);
    let mut audit = src.times(32);
    let mut resolution = 32;
    audit.sort_unstable();
    audit.dedup();
    let mut channels = Vec::new();
    for (c, limit) in limits.iter().enumerate() {
        let (ch, _, _) = fit_curve(src, oracle, c, &audit, limit, None);
        channels.push(ch);
    }
    let mut best: Option<(Vec<u8>, f32)> = None;
    for pass in 0..24 {
        // Do not wait for the coarse grid to pass before looking for narrow
        // events between its samples. Keep early coarse candidates diagnostic.
        if pass == 3 {
            let mut finer = src.times(96);
            finer.sort_unstable();
            finer.dedup();
            if finer.len() > audit.len() {
                audit = finer;
                resolution = 96;
                best = None;
            }
        }
        let check = scan(src, oracle, skl, &rig, &channels, resolution, &limits, q);
        let fitness = check
            .local
            .iter()
            .zip(q.local)
            .map(|(e, l)| ratio(*e, l))
            .fold(ratio(check.probe, q.model), f32::max);
        let bytes = src.emit(&channels)?;
        let replace = best.as_ref().is_none_or(|(b, f)| {
            if fitness <= 1. && *f <= 1. {
                bytes.len() < b.len()
            } else {
                fitness < *f
            }
        });
        if replace {
            best = Some((bytes, fitness));
        }
        if fitness <= 0.98 || check.updates.is_empty() {
            let mut finer = src.times(96);
            finer.sort_unstable();
            finer.dedup();
            if finer.len() > audit.len() && pass < 23 {
                audit = finer;
                resolution = 96;
                best = None;
                continue;
            }
            break;
        }
        let mut dirty = BTreeSet::new();
        for ((c, w), v) in check.updates {
            if v < limits[c][w] * 0.99 {
                limits[c][w] = v;
                dirty.insert(c);
            }
        }
        if dirty.is_empty() {
            break;
        }
        for c in dirty {
            let (ch, _, _) = fit_curve(src, oracle, c, &audit, &limits[c], Some(&channels[c].keys));
            channels[c] = ch;
        }
    }
    best.ok_or(Error::InvalidData("CANM fitting produced no candidate"))
}

fn serialize_channels(
    channels: &[Channel],
    hashes: &[u32],
    duration: f32,
    output_fps: f32,
    count: usize,
    bounds: [Vec3; 4],
) -> Result<Vec<u8>> {
    let [tmin, tmax, smin, smax] = bounds;
    // Each channel contributes its initial window, followed by keys sorted on their activation
    // time. This allows the single global read cursor to serve every channel without stalling.
    let mut entries = Vec::new();
    let mut seeds = Vec::with_capacity(channels.len());
    let mut pending = Vec::new();
    for (channel_id, channel) in channels.iter().enumerate() {
        let start = entries.len();
        let prologue = channel.keys.len().min(3);
        for key in &channel.keys[..prologue] {
            entries.push(Entry {
                key: *key,
                joint_and_kind: channel.joint | ((channel.kind as u16) << 14),
            });
        }
        seeds.push([
            start,
            start,
            start + (prologue - 1).min(1),
            start + prologue - 1,
        ]);
        for i in prologue..channel.keys.len() {
            pending.push((channel.keys[i - 2].time, channel_id, i));
        }
    }
    let prologue_count = entries.len();
    pending.sort_unstable();
    for (_, channel_id, i) in pending {
        let channel = &channels[channel_id];
        entries.push(Entry {
            key: channel.keys[i],
            joint_and_kind: channel.joint | ((channel.kind as u16) << 14),
        });
    }
    let wide = entries.len() > 65536;
    // Shipped CANMs use approximately one uniformly timed seek snapshot per thousand keys.
    // Do not allocate more snapshots than distinct validation samples for very wide rigs.
    let cache_count = entries.len().div_ceil(1000).min(count);
    let frames_offset = 128u64;
    let cache_offset = frames_offset + entries.len() as u64 * 10;
    let hash_offset =
        cache_offset + cache_count as u64 * hashes.len() as u64 * if wide { 48 } else { 24 };
    let size = hash_offset + hashes.len() as u64 * 4;
    if size > i32::MAX as u64 || entries.len() > i32::MAX as usize {
        return Err(Error::Unsupported(
            "compressed animation exceeds signed 32-bit section limits",
        ));
    }
    let mut bytes = Vec::with_capacity(size as usize);
    bytes.write_all(b"r3d2canm").map_err(rs_io::Error::from)?;
    bytes.write_u32(1)?;
    bytes.write_u32((size - 12) as u32)?;
    bytes.write_all(b"canm").map_err(rs_io::Error::from)?;
    bytes.write_u32(7)?; // Cubic interpolation with keyframe-time parametrization.
    bytes.write_i32(hashes.len() as i32)?;
    bytes.write_i32(entries.len() as i32)?;
    bytes.write_i32(cache_count as i32)?;
    bytes.write_f32(duration)?;
    bytes.write_f32(output_fps)?;
    // Conventional encoder metadata found in shipped files. Actual accuracy is controlled by
    // CompressionOptions and verified against decoded samples, not these informational fields.
    for metric in [2.0, 10.0, 2.0, 10.0, 0.01, 0.2] {
        bytes.write_f32(metric)?;
    }
    for bounds in [tmin, tmax, smin, smax] {
        bytes.write_vec3(bounds)?;
    }
    for offset in [frames_offset, cache_offset, hash_offset] {
        bytes.write_i32((offset - 12) as i32)?;
    }
    for entry in &entries {
        bytes.write_u16(entry.key.time)?;
        bytes.write_u16(entry.joint_and_kind)?;
        bytes
            .write_all(&entry.key.bytes)
            .map_err(rs_io::Error::from)?;
    }
    let mut cursor = prologue_count;
    for cache in 0..cache_count {
        // Stay one tick before the boundary so f32 rounding cannot seed a future window.
        let time = ((65535u64 * cache as u64 / cache_count as u64) as u16).saturating_sub(1);
        while cursor < entries.len() {
            let tag = entries[cursor].joint_and_kind;
            let channel = (tag & 0x3fff) as usize * 3 + (tag >> 14) as usize;
            let hot = &mut seeds[channel];
            if entries[hot[2]].key.time > time {
                break;
            }
            *hot = [hot[1], hot[2], hot[3], cursor];
            cursor += 1;
        }
        for seed in &seeds {
            for &index in seed {
                if wide {
                    bytes.write_u32(index as u32)?;
                } else {
                    bytes.write_u16(index as u16)?;
                }
            }
        }
    }
    for &hash in hashes {
        bytes.write_u32(hash)?;
    }
    debug_assert_eq!(bytes.len(), size as usize);
    Ok(bytes)
}

pub(crate) fn encode(
    source: &Animation,
    options: &AnimationExportOptions,
    skeleton: Option<&Skeleton>,
) -> Result<(Vec<u8>, AnimationExportMetrics)> {
    if source.tracks.len() > 16384 {
        return Err(Error::Unsupported("CANM supports at most 16384 tracks"));
    }
    // Initial half-frame candidates must fit the 16-bit clock without collisions.
    if source.frame_count() > 32768 {
        return Err(Error::Unsupported(
            "adaptive CANM supports at most 32768 source frames",
        ));
    }
    let empty = Skeleton::new();
    let skl = skeleton.unwrap_or(&empty);
    let mut src = Source::new(source);
    let q = check_options(options, skeleton.is_some());
    check_clock(&src, skl, q)?;
    let oracle = Oracle::new(&src);
    src.bounds = oracle.bounds();
    align_neutral_scale(&mut src.bounds);
    if src.bounds.iter().any(|b| !b.is_finite())
        || !(src.bounds[1] - src.bounds[0]).is_finite()
        || !(src.bounds[3] - src.bounds[2]).is_finite()
    {
        return Err(Error::Unsupported(
            "CANM vector quantization range is not representable",
        ));
    }
    let bytes = if src.frames == 1 {
        let channels: Vec<_> = (0..src.hashes.len() * 3)
            .map(|c| Channel {
                joint: (c / 3) as u16,
                kind: kind(c),
                keys: vec![packed(&src, &oracle, c, 0); 3],
            })
            .collect();
        src.emit(&channels)?
    } else {
        let (bytes, fitness) = fit(&src, &oracle, skl, q)?;
        if !fitness.is_finite() || fitness > 1. {
            return Err(Error::CompressionFit {
                error_ratio: fitness,
            });
        }
        bytes
    };
    drop(oracle);
    let metrics = validate_canm(&src, &bytes, options, skl, skeleton.is_some())?;
    Ok((bytes, metrics))
}
// Keep the neutral scale exactly representable. A tiny repeated scale bias
// compounds down a hierarchy and can move distant props more than a key fit.
pub(super) fn align_neutral_scale(bounds: &mut [Vec3; 4]) {
    for (axis, lo) in bounds[2].to_array().into_iter().enumerate() {
        let hi = bounds[3][axis];
        if !(lo < 1. && hi > 1.) {
            continue;
        }
        let center = (((1. - lo as f64) / (hi as f64 - lo as f64)) * 65535.).round() as i32;
        let mut best: Option<(f32, f32)> = None;
        for n in (center - 4).max(1)..=(center + 4).min(65534) {
            let step =
                ((1. - lo as f64) / n as f64).max((hi as f64 - 1.) / (65535 - n) as f64) as f32;
            for offset in 0..16 {
                let step = f32::from_bits(step.to_bits() + offset);
                let min = 1. - step * n as f32;
                let max = 1. + step * (65535 - n) as f32;
                if min > lo || max < hi {
                    continue;
                }
                let encoded = pack_vector(Vec3::ONE, Vec3::splat(min), Vec3::splat(max));
                let code = u16::from_le_bytes([encoded[0], encoded[1]]);
                let decoded = (max - min) / 65535. * code as f32 + min;
                if decoded == 1. && best.is_none_or(|(a, b)| max - min < b - a) {
                    best = Some((min, max));
                }
            }
        }
        if let Some((min, max)) = best {
            bounds[2][axis] = min;
            bounds[3][axis] = max;
        }
    }
}

#[cfg(test)]
#[path = "compressed_adaptive_tests.rs"]
mod tests;
