/*!
The candidate property-name dictionary the resolver hashes against. Each entry point enumerates
every name a section of a given kind (system, group, field, fluid) might carry; the resolver
65599-hashes each candidate under the relevant section and matches it to the raw hashes stored in
the file. The combinators expand the families the format generates programmatically — `flex`
suffixes, per-channel `rand` variants, numbered placeholders — so the lists stay compact.
*/

const FIELD_VARS: u32 = 10;
const GPART_VARS: u32 = 50;
const MAT_VARS: u32 = 5;
const RAND_VARS: u32 = 10;
const COLOR_VARS: u32 = 25;
const ROT_VARS: u32 = 20;

fn flex(args: &[&str]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len() * 5);
    for name in args {
        out.push(format!("{name}_flex"));
        for j in 0..4 {
            out.push(format!("{name}_flex{j}"));
        }
    }
    out
}

fn color_with_mods(mods: &[&str], args: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    for arg in args {
        for j in 0..COLOR_VARS {
            out.push(format!("{arg}{j}"));
        }
        for m in mods {
            out.push(format!("{arg}{m}P"));
            for l in 0..COLOR_VARS {
                out.push(format!("{arg}{m}P{l}"));
            }
        }
    }
    out
}

fn rand_with_mods(mods: &[&str], args: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    for arg in args {
        for j in 0..RAND_VARS {
            out.push(format!("{arg}{j}"));
        }
        for m in mods {
            out.push(format!("{arg}{m}P"));
            for l in 0..RAND_VARS {
                out.push(format!("{arg}{m}P{l}"));
            }
        }
    }
    out
}

fn rand_with_mods_owned(mods: &[&str], args: Vec<String>) -> Vec<String> {
    let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    rand_with_mods(mods, &refs)
}

fn flex_float(args: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for name in args {
        out.push((*name).to_string());
        out.push(format!("{name}_flex"));
        for j in 0..4 {
            out.push(format!("{name}_flex{j}"));
        }
    }
    out
}

fn rand_color_amount(args: &[&str]) -> Vec<String> {
    color_with_mods(&["R", "G", "B", "A"], args)
}

fn rand_float(args: &[&str]) -> Vec<String> {
    rand_with_mods(&["X", ""], args)
}

fn rand_vec2(args: &[&str]) -> Vec<String> {
    rand_with_mods(&["X", "Y"], args)
}

fn rand_vec3(args: &[&str]) -> Vec<String> {
    rand_with_mods(&["X", "Y", "Z"], args)
}

fn rand_color(args: &[&str]) -> Vec<String> {
    rand_with_mods(&["R", "G", "B", "A"], args)
}

fn flex_rand_float(args: &[&str]) -> Vec<String> {
    rand_with_mods_owned(&["X", ""], flex(args))
}

fn flex_rand_vec2(args: &[&str]) -> Vec<String> {
    rand_with_mods_owned(&["X", "Y"], flex(args))
}

fn flex_rand_vec3(args: &[&str]) -> Vec<String> {
    rand_with_mods_owned(&["X", "Y", "Z"], flex(args))
}

const MATERIAL_NAMES: &[&str] = &[
    "MaterialOverrideTransMap",
    "MaterialOverrideTransSource",
    "p-trans-sample",
    "MaterialOverride%PLACEHOLDER%BlendMode",
    "MaterialOverride%PLACEHOLDER%GlossTexture",
    "MaterialOverride%PLACEHOLDER%EmissiveTexture",
    "MaterialOverride%PLACEHOLDER%FixedAlphaScrolling",
    "MaterialOverride%PLACEHOLDER%Priority",
    "MaterialOverride%PLACEHOLDER%RenderingMode",
    "MaterialOverride%PLACEHOLDER%SubMesh",
    "MaterialOverride%PLACEHOLDER%Texture",
    "MaterialOverride%PLACEHOLDER%UVScroll",
];

const PART_FLUID_NAMES: &[&str] = &["fluid-params"];
const PART_GROUP_NAMES: &[&str] = &["GroupPart%PLACEHOLDER%"];
const PART_FIELD_NAMES: &[&str] = &[
    "field-accel-%PLACEHOLDER%",
    "field-attract-%PLACEHOLDER%",
    "field-drag-%PLACEHOLDER%",
    "field-noise-%PLACEHOLDER%",
    "field-orbit-%PLACEHOLDER%",
];

const SYSTEM_NAMES: &[&str] = &[
    "AudioFlexValueParameterName",
    "AudioParameterFlexID",
    "build-up-time",
    "group-vis",
    "group-scale-cap",
    "GroupPart%PLACEHOLDER%",
    "GroupPart%PLACEHOLDER%Type",
    "GroupPart%PLACEHOLDER%Importance",
    "Override-Offset%PLACEHOLDER%",
    "Override-Rotation%PLACEHOLDER%",
    "Override-Scale%PLACEHOLDER%",
    "KeepOrientationAfterSpellCast",
    "PersistThruDeath",
    "PersistThruRevive",
    "SelfIllumination",
    "SimulateEveryFrame",
    "SimulateOncePerFrame",
    "SimulateWhileOffScreen",
    "SoundEndsOnEmitterEnd",
    "SoundOnCreate",
    "SoundPersistent",
    "SoundsPlayWhileOffScreen",
    "VoiceOverOnCreate",
    "VoiceOverPersistent",
];

const GROUP_NAMES: &[&str] = &[
    "ExcludeAttachmentType",
    "KeywordsExcluded",
    "KeywordsIncluded",
    "KeywordsRequired",
    "Particle-ScaleAlongMovementVector",
    "SoundOnCreate",
    "SoundPersistent",
    "VoiceOverOnCreate",
    "VoiceOverPersistent",
    "dont-scroll-alpha-UV",
    "e-active",
    "e-alpharef",
    "e-beam-segments",
    "e-censor-policy",
    "e-disabled",
    "e-life",
    "e-life-scale",
    "e-linger",
    "e-local-orient",
    "e-period",
    "e-shape-name",
    "e-shape-scale",
    "e-shape-use-normal-for-birth",
    "e-soft-in-depth",
    "e-soft-out-depth",
    "e-soft-in-depth-delta",
    "e-soft-out-depth-delta",
    "e-timeoffset",
    "e-trail-cutoff",
    "e-trail-smoothing",
    "e-uvscroll",
    "e-uvscroll-mult",
    "flag-brighter-in-fow",
    "flag-disable-z",
    "flag-disable-y",
    "flag-groundlayer",
    "flag-ground-layer",
    "flag-force-animated-mesh-z-write",
    "flag-projected",
    "p-alphaslicerange",
    "p-animation",
    "p-backfaceon",
    "p-beammode",
    "p-bindtoemitter",
    "p-coloroffset",
    "p-colorscale",
    "p-colortype",
    "p-distortion-mode",
    "p-distortion-power",
    "p-falloff-texture",
    "p-fixedorbit",
    "p-fixedorbittype",
    "p-flexoffset",
    "p-flexscale",
    "p-followterrain",
    "p-frameRate",
    "p-frameRate-mult",
    "p-fresnel",
    "p-life-scale",
    "p-life-scale-offset",
    "p-life-scale-symX",
    "p-life-scale-symY",
    "p-life-scale-symZ",
    "p-linger",
    "p-local-orient",
    "p-lockedtoemitter",
    "p-mesh",
    "p-meshtex",
    "p-meshtex-mult",
    "p-normal-map",
    "p-numframes",
    "p-numframes-mult",
    "p-offsetbyheight",
    "p-offsetbyradius",
    "p-orientation",
    "p-projection-fading",
    "p-projection-y-range",
    "p-randomstartframe",
    "p-randomstartframe-mult",
    "p-reflection-fresnel",
    "p-reflection-map",
    "p-reflection-opacity-direct",
    "p-reflection-opacity-glancing",
    "p-rgba",
    "p-scalebias",
    "p-scalebyheight",
    "p-scalebyradius",
    "p-scaleupfromorigin",
    "p-shadow",
    "p-simpleorient",
    "p-skeleton",
    "p-skin",
    "p-startframe",
    "p-startframe-mult",
    "p-texdiv",
    "p-texdiv-mult",
    "p-texture",
    "p-texture-mode",
    "p-texture-mult",
    "p-texture-mult-mode",
    "p-texture-pixelate",
    "p-trailmode",
    "p-type",
    "p-uvmode",
    "p-uvparallax-scale",
    "p-uvscroll-alpha-mult",
    "p-uvscroll-no-alpha",
    "p-uvscroll-rgb",
    "p-uvscroll-rgb-clamp",
    "p-uvscroll-rgb-clamp-mult",
    "p-vec-velocity-minscale",
    "p-vec-velocity-scale",
    "p-vecalign",
    "p-xquadrot-on",
    "pass",
    "rendermode",
    "single-particle",
    "submesh-list",
    "teamcolor-correction",
    "uniformscale",
    "ChildParticleName",
    "ChildSpawnAtBone",
    "ChildEmitOnDeath",
    "p-childProb",
    "ChildParticleName%PLACEHOLDER%",
    "ChildSpawnAtBone%PLACEHOLDER%",
    "ChildEmitOnDeath%PLACEHOLDER%",
    "p-rgbaX%PLACEHOLDER%",
    "e-rgbaX%PLACEHOLDER%",
];

const FLUID_NAMES: &[&str] = &[
    "f-accel",
    "f-buoyancy",
    "f-denseforce",
    "f-diffusion",
    "f-dissipation",
    "f-life",
    "f-initdensity",
    "f-movement-x",
    "f-movement-y",
    "f-viscosity",
    "f-startkick",
    "f-rate",
    "f-rendersize",
    "f-jetdir%PLACEHOLDER%",
    "f-jetdirdiff%PLACEHOLDER%",
    "f-jetpos%PLACEHOLDER%",
    "f-jetspeed%PLACEHOLDER%",
];

const FIELD_NAMES_BASE: &[&str] = &["f-localspace", "f-axisfrac"];

/// Expand `%PLACEHOLDER%` tokens with `start..end`; items without the token pass through verbatim.
fn expand(items: &[&str], start: u32, end: u32) -> Vec<String> {
    let mut out = Vec::new();
    for item in items {
        if item.contains("%PLACEHOLDER%") {
            for k in start..end {
                out.push(item.replace("%PLACEHOLDER%", &k.to_string()));
            }
        } else {
            out.push((*item).to_string());
        }
    }
    out
}

pub(crate) fn part_group_names() -> Vec<String> {
    expand(PART_GROUP_NAMES, 0, GPART_VARS)
}

pub(crate) fn part_field_names() -> Vec<String> {
    expand(PART_FIELD_NAMES, 1, FIELD_VARS)
}

pub(crate) fn part_fluid_names() -> Vec<String> {
    PART_FLUID_NAMES.iter().map(|s| (*s).to_string()).collect()
}

pub(crate) fn system_names() -> Vec<String> {
    let mut out = expand(SYSTEM_NAMES, 0, GPART_VARS);
    out.extend(expand(MATERIAL_NAMES, 0, MAT_VARS));
    out
}

pub(crate) fn group_names() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    out.extend(expand(GROUP_NAMES, 0, 10));
    out.extend(expand(MATERIAL_NAMES, 0, MAT_VARS));
    out.extend(rand_color_amount(&["e-rgba", "p-xrgba"]));
    out.extend(flex_float(&["p-scale", "p-scaleEmitOffset"]));
    out.extend(flex_rand_float(&["e-rate", "p-life", "p-rotvel"]));
    out.extend(flex_rand_vec2(&["e-uvoffset"]));
    out.extend(flex_rand_vec3(&["p-offset", "p-postoffset", "p-vel"]));
    out.extend(rand_color(&[
        "e-censor-modulate",
        "p-fresnel-color",
        "p-reflection-fresnel-color",
    ]));
    out.extend(rand_float(&[
        "e-color-modulate",
        "e-framerate",
        "p-bindtoemitter",
        "p-life",
        "p-quadrot",
        "p-rotvel",
        "p-scale",
        "p-xquadrot",
        "p-xscale",
        "e-rate",
    ]));
    out.extend(rand_vec2(&[
        "e-ratebyvel",
        "e-uvoffset",
        "e-uvoffset-mult",
        "p-uvscroll-rgb",
        "p-uvscroll-rgb-mult",
    ]));
    out.extend(rand_vec3(&[
        "Emitter-BirthRotationalAcceleration",
        "Particle-Acceleration",
        "Particle-Drag",
        "Particle-Velocity",
        "e-tilesize",
        "p-accel",
        "p-drag",
        "p-offset",
        "p-orbitvel",
        "p-postoffset",
        "p-quadrot",
        "p-rotvel",
        "p-scale",
        "p-vel",
        "p-worldaccel",
        "p-xquadrot",
        "p-xrgba-beam-bind-distance",
        "p-xscale",
    ]));
    let rotation_names = expand(&["e-rotation%PLACEHOLDER%"], 0, ROT_VARS);
    let rotation_refs: Vec<&str> = rotation_names.iter().map(|s| s.as_str()).collect();
    out.extend(rand_float(&rotation_refs));
    out.extend(expand(&["e-rotation%PLACEHOLDER%-axis"], 0, ROT_VARS));
    out.extend(expand(PART_FIELD_NAMES, 1, FIELD_VARS));
    out.extend(PART_FLUID_NAMES.iter().map(|s| (*s).to_string()));
    out
}

pub(crate) fn field_names() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    out.extend(FIELD_NAMES_BASE.iter().map(|s| (*s).to_string()));
    out.extend(rand_float(&[
        "f-accel",
        "f-drag",
        "f-freq",
        "f-frequency",
        "f-period",
        "f-radius",
        "f-veldelta",
    ]));
    out.extend(rand_vec3(&[
        "f-accel",
        "f-direction",
        "f-pos",
        "f-axisfrac",
    ]));
    out
}

pub(crate) fn fluid_names() -> Vec<String> {
    expand(FLUID_NAMES, 0, 4)
}
