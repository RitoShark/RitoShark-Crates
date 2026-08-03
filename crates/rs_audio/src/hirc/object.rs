use super::read::{Reader, skip_base_params, skip_initial_params};
use crate::error::Result;

/// A sound: the leaf of the hierarchy, naming the `.wem` that actually plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    pub id: u32,
    /// The embedded or streamed `.wem` id this sound plays.
    pub source_id: u32,
    pub streamed: bool,
}

/// An event action: what an event does, and to which object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub id: u32,
    pub scope: u8,
    pub action_type: u8,
    /// The hierarchy object this action drives; zero for actions that set switches or states.
    pub target_id: u32,
    pub switch_group_id: u32,
    pub switch_state_id: u32,
    pub state_group_id: u32,
    pub target_state_id: u32,
}

/// An event: the entry point the game triggers by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub id: u32,
    pub action_ids: Vec<u32>,
}

/// Any object whose useful content is a list of child objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    pub id: u32,
    pub parent_id: u32,
    pub children: Vec<u32>,
}

/// A switch container: children selected at runtime by a switch or state group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchContainer {
    pub id: u32,
    pub parent_id: u32,
    pub group_type: u8,
    pub group_id: u32,
    pub children: Vec<u32>,
}

/// A music track: names its `.wem` sources directly rather than through child objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicTrack {
    pub id: u32,
    pub parent_id: u32,
    pub source_ids: Vec<u32>,
    pub switch_group_id: u32,
    pub switch_ids: Vec<u32>,
}

/// A music switch: children plus a decision tree naming further audio objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicSwitch {
    pub id: u32,
    pub parent_id: u32,
    pub children: Vec<u32>,
    pub decision_audio_ids: Vec<u32>,
}

/** The decoded content of one HIRC object.

Only the types on the path from an event to a `.wem` are decoded. Everything else — bus routing,
RTPCs, state groups, effects, attenuations — stays [`HircBody::Opaque`], as does any object whose
body does not parse cleanly. That keeps an unfamiliar bank version degrading gracefully instead of
failing outright or, worse, producing values read from the wrong offsets. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HircBody {
    Sound(Sound),
    Action(Action),
    Event(Event),
    RandomSequenceContainer(Container),
    SwitchContainer(SwitchContainer),
    ActorMixer(Container),
    MusicSegment(Container),
    MusicTrack(MusicTrack),
    MusicSwitch(MusicSwitch),
    MusicPlaylist(Container),
    Opaque,
}

/// The object type byte that opens every HIRC object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HircKind {
    Settings,
    Sound,
    Action,
    Event,
    RandomSequenceContainer,
    SwitchContainer,
    ActorMixer,
    Bus,
    LayerContainer,
    MusicSegment,
    MusicTrack,
    MusicSwitch,
    MusicPlaylist,
    Attenuation,
    DialogueEvent,
    MotionBus,
    MotionFx,
    Effect,
    AuxiliaryBus,
    Other(u8),
}

impl HircKind {
    pub(super) fn from_byte(byte: u8) -> Self {
        match byte {
            1 => Self::Settings,
            2 => Self::Sound,
            3 => Self::Action,
            4 => Self::Event,
            5 => Self::RandomSequenceContainer,
            6 => Self::SwitchContainer,
            7 => Self::ActorMixer,
            8 => Self::Bus,
            9 => Self::LayerContainer,
            10 => Self::MusicSegment,
            11 => Self::MusicTrack,
            12 => Self::MusicSwitch,
            13 => Self::MusicPlaylist,
            14 => Self::Attenuation,
            15 => Self::DialogueEvent,
            16 => Self::MotionBus,
            17 => Self::MotionFx,
            18 => Self::Effect,
            20 => Self::AuxiliaryBus,
            other => Self::Other(other),
        }
    }
}

/// One object in the hierarchy: its type, its id, and whatever we could decode of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HircObject {
    pub kind: HircKind,
    pub id: u32,
    pub body: HircBody,
}

impl HircObject {
    /// The child objects this one routes to, empty for leaves and opaque objects.
    pub fn children(&self) -> &[u32] {
        match &self.body {
            HircBody::RandomSequenceContainer(c)
            | HircBody::ActorMixer(c)
            | HircBody::MusicSegment(c)
            | HircBody::MusicPlaylist(c) => &c.children,
            HircBody::SwitchContainer(c) => &c.children,
            HircBody::MusicSwitch(c) => &c.children,
            _ => &[],
        }
    }

    /// The `.wem` ids this object plays directly, empty unless it is a sound or a music track.
    pub fn source_ids(&self) -> Vec<u32> {
        match &self.body {
            HircBody::Sound(sound) => vec![sound.source_id],
            HircBody::MusicTrack(track) => track.source_ids.clone(),
            _ => Vec::new(),
        }
    }
}

pub(super) fn decode(kind: HircKind, body: &[u8], version: u32) -> Result<(u32, HircBody)> {
    let mut reader = Reader::new(body);
    let id = reader.u32()?;

    let decoded = match kind {
        HircKind::Sound => HircBody::Sound(read_sound(id, &mut reader, version)?),
        HircKind::Action => HircBody::Action(read_action(id, &mut reader)?),
        HircKind::Event => HircBody::Event(read_event(id, &mut reader, version)?),
        HircKind::RandomSequenceContainer => {
            HircBody::RandomSequenceContainer(read_random_container(id, &mut reader, version)?)
        }
        HircKind::SwitchContainer => {
            HircBody::SwitchContainer(read_switch_container(id, &mut reader, version)?)
        }
        HircKind::ActorMixer => HircBody::ActorMixer(read_actor_mixer(id, &mut reader, version)?),
        HircKind::MusicSegment => {
            HircBody::MusicSegment(read_music_container(id, &mut reader, version)?)
        }
        HircKind::MusicPlaylist => {
            HircBody::MusicPlaylist(read_music_container(id, &mut reader, version)?)
        }
        HircKind::MusicTrack => HircBody::MusicTrack(read_music_track(id, &mut reader, version)?),
        HircKind::MusicSwitch => {
            HircBody::MusicSwitch(read_music_switch(id, &mut reader, version)?)
        }
        _ => HircBody::Opaque,
    };

    Ok((id, decoded))
}

fn read_sound(id: u32, reader: &mut Reader, version: u32) -> Result<Sound> {
    reader.skip(4)?;
    let streamed = reader.u8()? != 0;
    if version <= 0x59 {
        reader.skip(3)?;
    }
    if version <= 0x70 {
        reader.skip(4)?;
    }
    Ok(Sound {
        id,
        source_id: reader.u32()?,
        streamed,
    })
}

fn read_action(id: u32, reader: &mut Reader) -> Result<Action> {
    let scope = reader.u8()?;
    let action_type = reader.u8()?;

    let mut action = Action {
        id,
        scope,
        action_type,
        target_id: 0,
        switch_group_id: 0,
        switch_state_id: 0,
        state_group_id: 0,
        target_state_id: 0,
    };

    match action_type {
        /* SetSwitch and SetState name a group and a value rather than an audio object. */
        25 => {
            reader.skip(5)?;
            skip_initial_params(reader)?;
            action.switch_group_id = reader.u32()?;
            action.switch_state_id = reader.u32()?;
        }
        18 => {
            reader.skip(5)?;
            skip_initial_params(reader)?;
            action.state_group_id = reader.u32()?;
            action.target_state_id = reader.u32()?;
        }
        _ => action.target_id = reader.u32()?,
    }

    Ok(action)
}

fn read_event(id: u32, reader: &mut Reader, version: u32) -> Result<Event> {
    let count = u32::from(reader.u8()?);
    if version == 0x58 {
        reader.skip(3)?;
    }
    Ok(Event {
        id,
        action_ids: reader.ids(count)?,
    })
}

fn read_random_container(id: u32, reader: &mut Reader, version: u32) -> Result<Container> {
    let parent_id = skip_base_params(reader, version)?;
    reader.skip(24)?;
    let count = reader.u32()?;
    Ok(Container {
        id,
        parent_id,
        children: reader.ids(count)?,
    })
}

fn read_switch_container(id: u32, reader: &mut Reader, version: u32) -> Result<SwitchContainer> {
    let parent_id = skip_base_params(reader, version)?;
    let group_type = reader.u8()?;
    if version <= 0x59 {
        reader.skip(3)?;
    }
    let group_id = reader.u32()?;
    reader.skip(5)?;
    let count = reader.u32()?;
    Ok(SwitchContainer {
        id,
        parent_id,
        group_type,
        group_id,
        children: reader.ids(count)?,
    })
}

fn read_actor_mixer(id: u32, reader: &mut Reader, version: u32) -> Result<Container> {
    let parent_id = skip_base_params(reader, version)?;
    let count = reader.u32()?;
    Ok(Container {
        id,
        parent_id,
        children: reader.ids(count)?,
    })
}

fn read_music_container(id: u32, reader: &mut Reader, version: u32) -> Result<Container> {
    reader.skip(1)?;
    let parent_id = skip_base_params(reader, version)?;
    let count = reader.u32()?;
    Ok(Container {
        id,
        parent_id,
        children: reader.ids(count)?,
    })
}

fn read_music_track(id: u32, reader: &mut Reader, version: u32) -> Result<MusicTrack> {
    reader.skip(1)?;
    let sources = reader.u32()?;
    reader.skip_scaled(sources, 14)?;

    let clips = reader.u32()?;
    let mut source_ids = Vec::with_capacity(clips as usize);
    for _ in 0..clips {
        let _track_index = reader.u32()?;
        source_ids.push(reader.u32()?);
        reader.skip(4 + 32)?;
    }

    let _subtrack_count = reader.u32()?;
    let automations = reader.u32()?;
    for _ in 0..automations {
        reader.skip(8)?;
        let points = reader.u32()?;
        reader.skip_scaled(points, 12)?;
    }

    let parent_id = skip_base_params(reader, version)?;

    let track_type = reader.u8()?;
    let mut switch_group_id = 0;
    let mut switch_ids = Vec::new();
    if track_type == 0x3 {
        reader.skip(1)?;
        switch_group_id = reader.u32()?;
        reader.skip(4)?;
        let count = reader.u32()?;
        switch_ids = reader.ids(count)?;
    }

    Ok(MusicTrack {
        id,
        parent_id,
        source_ids,
        switch_group_id,
        switch_ids,
    })
}

fn read_music_switch(id: u32, reader: &mut Reader, version: u32) -> Result<MusicSwitch> {
    reader.skip(1)?;
    let parent_id = skip_base_params(reader, version)?;

    let count = reader.u32()?;
    let children = reader.ids(count)?;

    reader.skip(23)?;
    let stingers = reader.u32()?;
    reader.skip_scaled(stingers, 24)?;

    let rules = reader.u32()?;
    for _ in 0..rules {
        let sources = reader.u32()?;
        reader.skip_scaled(sources, 4)?;
        let destinations = reader.u32()?;
        reader.skip_scaled(destinations, 4)?;
        reader.skip(if version <= 0x84 { 45 } else { 47 })?;
        if reader.u8()? != 0 {
            reader.skip(30)?;
        }
    }

    reader.skip(1)?;
    let arguments = reader.u32()?;
    reader.skip_scaled(arguments, 4)?;
    reader.skip_scaled(arguments, 1)?;

    /* The decision tree is a flat array of twelve-byte nodes: key, audio object, then flags. */
    let tree_bytes = reader.u32()?;
    reader.skip(1)?;
    let nodes = tree_bytes / 12;
    if (nodes as usize).saturating_mul(12) > reader.remaining() {
        return Err(crate::error::Error::Hirc(
            "decision tree is larger than the object body",
        ));
    }

    let mut decision_audio_ids = Vec::with_capacity(nodes as usize);
    for _ in 0..nodes {
        let _key = reader.u32()?;
        decision_audio_ids.push(reader.u32()?);
        reader.skip(4)?;
    }

    Ok(MusicSwitch {
        id,
        parent_id,
        children,
        decision_audio_ids,
    })
}
