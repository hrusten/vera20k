//! Game sound events — the bridge between sim and audio.
//!
//! The simulation produces GameSoundEvents when things happen (weapon fired,
//! unit selected, entity destroyed). The app layer collects these events each
//! tick and feeds them to the SfxPlayer for playback.
//!
//! Events carry the sound ID (from rules.ini / sound.ini) rather than a
//! filename — the SfxPlayer resolves IDs to files via SoundRegistry.
//!
//! ## Design
//! Events are plain data — no audio library handles, no asset references. This keeps
//! sim/ free from audio dependencies. The event queue is a simple Vec that
//! gets drained each frame.
//!
//! ## Dependency rules
//! - Part of audio/ — but contains no rodio code, only data types.
//! - sim/ may reference this module to push events (acceptable because
//!   it's pure data with zero audio-library dependencies).

/// A sound event produced by the game simulation or UI.
#[derive(Debug, Clone)]
pub enum GameSoundEvent {
    /// A weapon fired — play the weapon's Report= sound.
    WeaponFired {
        /// sound.ini ID from the weapon's Report= field.
        sound_id: String,
        /// Screen position of the sound source (for spatial audio).
        /// If None, plays at full volume (non-positional).
        screen_pos: Option<(f32, f32)>,
    },

    /// A unit was selected by the player — play VoiceSelect.
    UnitSelected {
        /// sound.ini ID from the unit's VoiceSelect= field.
        sound_id: String,
    },

    /// A unit was ordered to move — play VoiceMove.
    UnitMoveOrder {
        /// sound.ini ID from the unit's VoiceMove= field.
        sound_id: String,
    },

    /// A unit was ordered to attack — play VoiceAttack.
    UnitAttackOrder {
        /// sound.ini ID from the unit's VoiceAttack= field.
        sound_id: String,
    },

    /// An entity was destroyed — play DieSound.
    EntityDestroyed {
        /// sound.ini ID from the entity's DieSound= field.
        sound_id: String,
        /// Screen position of the sound source (for spatial audio).
        screen_pos: Option<(f32, f32)>,
    },

    /// A building finished construction — play the EVA "Construction complete" or similar.
    BuildingReady {
        /// sound.ini ID for the completion announcement.
        sound_id: String,
    },

    /// A unit finished training — play the EVA "Unit ready" or similar.
    UnitReady {
        /// sound.ini ID for the unit-ready announcement.
        sound_id: String,
    },

    /// Generic UI sound (button click, error beep, etc.).
    UiSound {
        /// sound.ini ID for the UI sound.
        sound_id: String,
    },
}

impl GameSoundEvent {
    /// Get the sound ID for this event.
    pub fn sound_id(&self) -> &str {
        match self {
            Self::WeaponFired { sound_id, .. }
            | Self::UnitSelected { sound_id }
            | Self::UnitMoveOrder { sound_id }
            | Self::UnitAttackOrder { sound_id }
            | Self::EntityDestroyed { sound_id, .. }
            | Self::BuildingReady { sound_id }
            | Self::UnitReady { sound_id }
            | Self::UiSound { sound_id } => sound_id,
        }
    }

    /// Get the screen position for spatial audio, if this event has one.
    pub fn screen_pos(&self) -> Option<(f32, f32)> {
        match self {
            Self::WeaponFired { screen_pos, .. } => *screen_pos,
            Self::EntityDestroyed { screen_pos, .. } => *screen_pos,
            _ => None,
        }
    }
}

/// Collects sound events during a simulation tick for later playback.
///
/// Drained by the app layer each frame after sim ticking.
#[derive(Debug, Default)]
pub struct SoundEventQueue {
    events: Vec<GameSoundEvent>,
}

impl SoundEventQueue {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    /// Push a sound event into the queue.
    pub fn push(&mut self, event: GameSoundEvent) {
        self.events.push(event);
    }

    /// Drain all pending events for playback.
    pub fn drain(&mut self) -> Vec<GameSoundEvent> {
        std::mem::take(&mut self.events)
    }

    /// Whether there are pending events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sound_id_accessor() {
        let evt: GameSoundEvent = GameSoundEvent::WeaponFired {
            sound_id: "VGCannon1".to_string(),
            screen_pos: None,
        };
        assert_eq!(evt.sound_id(), "VGCannon1");
    }

    #[test]
    fn test_queue_drain() {
        let mut queue: SoundEventQueue = SoundEventQueue::new();
        assert!(queue.is_empty());
        queue.push(GameSoundEvent::UiSound {
            sound_id: "click".to_string(),
        });
        queue.push(GameSoundEvent::UiSound {
            sound_id: "beep".to_string(),
        });
        assert!(!queue.is_empty());
        let events: Vec<GameSoundEvent> = queue.drain();
        assert_eq!(events.len(), 2);
        assert!(queue.is_empty());
    }
}
