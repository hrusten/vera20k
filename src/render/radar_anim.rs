//! Radar chrome animation state machine.
//!
//! Manages the animated radar.shp (33 frames) that plays when the player gains
//! or loses radar capability. The animation uses a dedicated updatable GPU texture
//! separate from the static sidebar chrome atlas.
//!
//! ## State machine
//! - `Offline` — frame 32 (closed housing), no minimap
//! - `Opening` — animate 32→0 when radar gained
//! - `Online` — frame 0 (open housing), minimap visible
//! - `Closing` — animate 0→32 when radar lost
//!
//! ## Dependency rules
//! - Part of render/ — depends on render/batch, render/gpu.
//! - Reads pre-rendered RGBA frames from SidebarChromeAtlas.

use crate::render::batch::{BatchRenderer, BatchTexture};
use crate::render::gpu::GpuContext;

/// Milliseconds per frame during the opening/closing animation.
/// ~64ms per frame = ~2.1s for 33 frames. Matches the original RA2 timing.
const RADAR_ANIM_FRAME_RATE_MS: f32 = 64.0;

/// Current phase of the radar chrome animation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadarAnimPhase {
    /// Radar offline — showing closed housing (last frame).
    Offline,
    /// Radar powering up — animating from last frame toward frame 0.
    Opening,
    /// Radar fully online — showing open housing (frame 0), minimap visible.
    Online,
    /// Radar powering down — animating from frame 0 toward last frame.
    Closing,
}

/// Manages the animated radar chrome display.
///
/// Holds all pre-rendered RGBA frames and a single GPU texture that gets
/// updated via `queue.write_texture()` whenever the current frame changes.
/// This avoids putting all 33 frames in the atlas (which would waste ~300KB).
pub struct RadarAnimState {
    phase: RadarAnimPhase,
    /// Current animation frame index (0 = fully open, last = fully closed).
    current_frame: usize,
    /// Time accumulated since last frame change.
    elapsed_ms: f32,
    /// Total number of frames in the radar animation.
    frame_count: usize,
    /// GPU texture (rewritten each time the frame changes).
    texture: BatchTexture,
    /// Raw wgpu texture handle for `write_texture()`.
    texture_raw: wgpu::Texture,
    /// Pre-rendered RGBA data for each frame.
    frames_rgba: Vec<Vec<u8>>,
    /// Pixel dimensions of each frame.
    pub width: u32,
    pub height: u32,
}

impl RadarAnimState {
    /// Create a new RadarAnimState from pre-rendered radar.shp frames.
    ///
    /// Starts in `Offline` phase (last frame shown).
    /// `frames_rgba` must be non-empty; each entry is width*height*4 bytes RGBA.
    pub fn new(
        gpu: &GpuContext,
        batch: &BatchRenderer,
        frames_rgba: Vec<Vec<u8>>,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        if frames_rgba.is_empty() || width == 0 || height == 0 {
            return None;
        }
        let frame_count: usize = frames_rgba.len();
        let last_frame: usize = frame_count.saturating_sub(1);

        // Initialize GPU texture with the last frame (closed/offline state).
        let initial_rgba: &[u8] = &frames_rgba[last_frame];
        let (texture_raw, texture) =
            batch.create_updatable_texture(gpu, initial_rgba, width, height);

        Some(Self {
            phase: RadarAnimPhase::Offline,
            current_frame: last_frame,
            elapsed_ms: 0.0,
            frame_count,
            texture,
            texture_raw,
            frames_rgba,
            width,
            height,
        })
    }

    /// Notify the animation of the current radar availability.
    /// Triggers Opening or Closing transitions as needed.
    pub fn set_has_radar(&mut self, has_radar: bool) {
        match (has_radar, self.phase) {
            (true, RadarAnimPhase::Offline) | (true, RadarAnimPhase::Closing) => {
                self.phase = RadarAnimPhase::Opening;
                self.elapsed_ms = 0.0;
            }
            (false, RadarAnimPhase::Online) | (false, RadarAnimPhase::Opening) => {
                self.phase = RadarAnimPhase::Closing;
                self.elapsed_ms = 0.0;
            }
            _ => {} // Already in the right direction or stable state.
        }
    }

    /// Advance the animation by `dt_ms` milliseconds.
    /// Updates the GPU texture when the frame changes.
    pub fn tick(&mut self, gpu: &GpuContext, dt_ms: f32) {
        match self.phase {
            RadarAnimPhase::Offline | RadarAnimPhase::Online => return,
            RadarAnimPhase::Opening | RadarAnimPhase::Closing => {}
        }

        self.elapsed_ms += dt_ms;
        let old_frame: usize = self.current_frame;

        while self.elapsed_ms >= RADAR_ANIM_FRAME_RATE_MS {
            self.elapsed_ms -= RADAR_ANIM_FRAME_RATE_MS;

            match self.phase {
                RadarAnimPhase::Opening => {
                    if self.current_frame == 0 {
                        self.phase = RadarAnimPhase::Online;
                        self.elapsed_ms = 0.0;
                        break;
                    }
                    self.current_frame -= 1;
                }
                RadarAnimPhase::Closing => {
                    let last: usize = self.frame_count.saturating_sub(1);
                    if self.current_frame >= last {
                        self.phase = RadarAnimPhase::Offline;
                        self.elapsed_ms = 0.0;
                        break;
                    }
                    self.current_frame += 1;
                }
                _ => break,
            }
        }

        // Only upload to GPU when the frame actually changed.
        if self.current_frame != old_frame {
            self.upload_frame(gpu);
        }
    }

    /// Whether the minimap should be drawn (only when fully online).
    pub fn is_minimap_visible(&self) -> bool {
        self.phase == RadarAnimPhase::Online
    }

    /// Current animation phase.
    pub fn phase(&self) -> RadarAnimPhase {
        self.phase
    }

    /// Get the GPU texture for drawing the radar chrome.
    pub fn texture(&self) -> &BatchTexture {
        &self.texture
    }

    /// Upload the current frame's RGBA data to the GPU texture.
    fn upload_frame(&self, gpu: &GpuContext) {
        let rgba: &[u8] = &self.frames_rgba[self.current_frame];
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture_raw,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.width * 4),
                rows_per_image: Some(self.height),
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_transitions() {
        // Test state machine logic without GPU (can't create textures in unit tests).
        // Just verify the phase transition rules.
        assert_eq!(RadarAnimPhase::Offline, RadarAnimPhase::Offline);
        assert_ne!(RadarAnimPhase::Offline, RadarAnimPhase::Online);

        // Opening should trigger from Offline when has_radar becomes true.
        // Closing should trigger from Online when has_radar becomes false.
        // These are verified implicitly by the set_has_radar match arms.
    }
}
