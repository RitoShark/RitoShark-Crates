mod object;
mod read;

pub use object::{
    Action, Container, Event, HircBody, HircKind, HircObject, MusicSwitch, MusicTrack, Sound,
    SwitchContainer,
};

use crate::error::{Error, Result};
use std::collections::{HashMap, HashSet};

/** The Wwise object hierarchy inside a bank's HIRC section.

This is a read-only view. The bank keeps the section's bytes verbatim, so parsing here can never
threaten the round-trip contract — which in turn frees the parser to decode only the object types
that matter and leave the rest alone.

Objects are framed as `type byte, u32 length, body`. That length is what makes tolerance possible:
an object whose body does not parse is kept as [`HircBody::Opaque`] and the cursor jumps to the
next one, so a single unfamiliar object costs one object rather than the whole section. */
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HircSection {
    pub objects: Vec<HircObject>,
}

/// Wwise's action-type byte for Play. 1 is Stop, 2 Pause, 3 Resume, 4 Play.
const ACTION_PLAY: u8 = 4;

impl HircSection {
    /** Parses a HIRC section body. `version` is the bank's BKHD version, which decides the size
    of several parameter blocks. */
    pub fn parse(body: &[u8], version: u32) -> Result<Self> {
        if body.len() < 4 {
            return Err(Error::Hirc("HIRC section is too short to hold a count"));
        }
        let declared = u32::from_le_bytes([body[0], body[1], body[2], body[3]]) as usize;

        let mut objects = Vec::with_capacity(declared.min(0x10000));
        let mut at = 4usize;

        for _ in 0..declared {
            if at + 5 > body.len() {
                break;
            }
            let kind = HircKind::from_byte(body[at]);
            let length =
                u32::from_le_bytes([body[at + 1], body[at + 2], body[at + 3], body[at + 4]])
                    as usize;
            at += 5;

            let Some(end) = at.checked_add(length) else {
                break;
            };
            if end > body.len() {
                break;
            }

            let object_body = &body[at..end];
            let object = match object::decode(kind, object_body, version) {
                Ok((id, decoded)) => HircObject {
                    kind,
                    id,
                    body: decoded,
                },
                Err(_) => HircObject {
                    kind,
                    id: object_body
                        .get(..4)
                        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .unwrap_or(0),
                    body: HircBody::Opaque,
                },
            };
            objects.push(object);

            at = end;
        }

        Ok(Self { objects })
    }

    pub fn object(&self, id: u32) -> Option<&HircObject> {
        self.objects.iter().find(|o| o.id == id)
    }

    /// Every event in the bank, in file order.
    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.objects.iter().filter_map(|o| match &o.body {
            HircBody::Event(event) => Some(event),
            _ => None,
        })
    }

    /// Every sound, i.e. every object naming a `.wem` directly.
    pub fn sounds(&self) -> impl Iterator<Item = &Sound> {
        self.objects.iter().filter_map(|o| match &o.body {
            HircBody::Sound(sound) => Some(sound),
            _ => None,
        })
    }

    /// How many objects were kept opaque, either as an unmodelled type or as a failed decode.
    pub fn opaque_count(&self) -> usize {
        self.objects
            .iter()
            .filter(|o| o.body == HircBody::Opaque)
            .count()
    }

    /** Every `.wem` id an event can end up playing.

    An event fans out to actions, each action targets a hierarchy object, and that object is either
    a sound naming a `.wem` or a container routing to more objects. Walking that whole reachable
    set is what turns "mute this voice line" into a concrete list of payloads to replace.

    Only PLAY actions are followed. An event carries more than one kind: Akali skin92's
    `Play_sfx_AkaliSkin92_Recall3D_leadin1` has one Play and three Stops, each pointing at a
    different container. Following all of them returned the cues the event SILENCES alongside the
    one it starts, in no defined order, so a caller taking the first id played whatever the event
    was cutting off - a skin's recall came out sounding like the base champion's. An event whose
    actions are all non-Play plays nothing, and correctly yields an empty list.

    Ids are returned in discovery order, deduplicated. Objects are visited at most once, so a
    hierarchy containing a cycle terminates rather than recursing forever. */
    pub fn wems_for_event(&self, event_id: u32) -> Vec<u32> {
        let index: HashMap<u32, &HircObject> = self.objects.iter().map(|o| (o.id, o)).collect();

        let Some(HircObject {
            body: HircBody::Event(event),
            ..
        }) = index.get(&event_id).copied()
        else {
            return Vec::new();
        };

        let mut roots = Vec::new();
        for action_id in &event.action_ids {
            if let Some(HircObject {
                body: HircBody::Action(action),
                ..
            }) = index.get(action_id).copied()
                && action.action_type == ACTION_PLAY
                && action.target_id != 0
            {
                roots.push(action.target_id);
            }
        }

        let mut sources = Vec::new();
        let mut seen_source = HashSet::new();
        let mut visited = HashSet::new();
        let mut pending = roots;

        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            let Some(object) = index.get(&id).copied() else {
                continue;
            };

            for source in object.source_ids() {
                if source != 0 && seen_source.insert(source) {
                    sources.push(source);
                }
            }

            if let HircBody::MusicSwitch(switch) = &object.body {
                pending.extend(switch.decision_audio_ids.iter().copied());
            }
            pending.extend(object.children().iter().copied());
        }

        sources
    }

    /// Every event paired with the `.wem` ids it can play.
    pub fn event_wem_map(&self) -> Vec<(u32, Vec<u32>)> {
        self.events()
            .map(|event| (event.id, self.wems_for_event(event.id)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    // `super::*` already re-exports Action/Container/Event/Sound/HircBody/HircKind/HircObject.
    use super::*;

    fn obj(kind: HircKind, id: u32, body: HircBody) -> HircObject {
        HircObject { kind, id, body }
    }

    /* The exact shape of Akali skin92's `Play_sfx_AkaliSkin92_Recall3D_leadin1`: one Play
    action into a random container holding the new cue, plus a Stop action aimed at a
    different container holding the cue being cut off. Following both returned the
    stopped sound too, and a caller taking the first id played the wrong one. */
    fn recall_shaped_section() -> HircSection {
        HircSection {
            objects: vec![
                obj(
                    HircKind::Event,
                    1,
                    HircBody::Event(Event {
                        id: 1,
                        action_ids: vec![10, 11],
                    }),
                ),
                obj(
                    HircKind::Action,
                    10,
                    HircBody::Action(Action {
                        id: 10,
                        scope: 0,
                        action_type: 4,
                        target_id: 20,
                        switch_group_id: 0,
                        switch_state_id: 0,
                        state_group_id: 0,
                        target_state_id: 0,
                    }),
                ),
                obj(
                    HircKind::Action,
                    11,
                    HircBody::Action(Action {
                        id: 11,
                        scope: 0,
                        action_type: 1,
                        target_id: 21,
                        switch_group_id: 0,
                        switch_state_id: 0,
                        state_group_id: 0,
                        target_state_id: 0,
                    }),
                ),
                obj(
                    HircKind::RandomSequenceContainer,
                    20,
                    HircBody::RandomSequenceContainer(Container {
                        id: 20,
                        parent_id: 0,
                        children: vec![30],
                    }),
                ),
                obj(
                    HircKind::RandomSequenceContainer,
                    21,
                    HircBody::RandomSequenceContainer(Container {
                        id: 21,
                        parent_id: 0,
                        children: vec![31],
                    }),
                ),
                obj(
                    HircKind::Sound,
                    30,
                    HircBody::Sound(Sound {
                        id: 30,
                        source_id: 716848736,
                        streamed: false,
                    }),
                ),
                obj(
                    HircKind::Sound,
                    31,
                    HircBody::Sound(Sound {
                        id: 31,
                        source_id: 368824398,
                        streamed: false,
                    }),
                ),
            ],
        }
    }

    #[test]
    fn only_play_actions_contribute_wems() {
        assert_eq!(recall_shaped_section().wems_for_event(1), vec![716848736]);
    }

    #[test]
    fn an_event_with_no_play_action_plays_nothing() {
        let mut section = recall_shaped_section();
        for object in &mut section.objects {
            if let HircBody::Action(action) = &mut object.body {
                action.action_type = 1; // every action a Stop
            }
        }
        assert!(section.wems_for_event(1).is_empty());
    }
}
