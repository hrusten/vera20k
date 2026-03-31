//! Dedicated bridge-body atlas for the zdepth bridge pass.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::assets::asset_manager::AssetManager;
use crate::assets::pal_file::Palette;
use crate::assets::shp_file::ShpFile;
use crate::map::overlay::OverlayEntry;
use crate::map::overlay_types::{OverlayTypeFlags, OverlayTypeRegistry};
use crate::render::batch::{BatchRenderer, BatchTexture};
use crate::render::gpu::GpuContext;
use crate::render::overlay_atlas::{OverlaySpriteEntry, OverlaySpriteKey};
use crate::rules::art_data::{self, ArtRegistry};
use crate::rules::ini_parser::IniFile;
use wgpu::util::DeviceExt;

const SPRITE_PADDING: u32 = 1;

pub struct BridgeAtlas {
    pub texture: BatchTexture,
    pub depth_texture_view: wgpu::TextureView,
    pub zdepth_bind_group: wgpu::BindGroup,
    entries: HashMap<OverlaySpriteKey, OverlaySpriteEntry>,
}

impl BridgeAtlas {
    pub fn get(&self, key: &OverlaySpriteKey) -> Option<&OverlaySpriteEntry> {
        self.entries.get(key)
    }
}

struct RenderedBridge {
    key: OverlaySpriteKey,
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    offset_x: f32,
    offset_y: f32,
}

pub fn is_high_bridge_body_name(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "BRIDGE1" | "BRIDGEB1" | "BRIDGE2" | "BRIDGEB2"
    )
}

pub fn build_bridge_atlas(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    overlays: &[OverlayEntry],
    overlay_names: &BTreeMap<u8, String>,
    asset_manager: &AssetManager,
    theater_palette: &Palette,
    unit_palette: &Palette,
    theater_ext: &str,
    theater_name: &str,
    overlay_registry: &OverlayTypeRegistry,
    rules_ini: &IniFile,
    art_registry: &ArtRegistry,
) -> Option<BridgeAtlas> {
    let mut needed: HashSet<OverlaySpriteKey> = HashSet::new();
    for entry in overlays {
        let Some(name) = overlay_names.get(&entry.overlay_id) else {
            continue;
        };
        if !is_high_bridge_body_name(name) {
            continue;
        }
        for frame in 0u8..18u8 {
            needed.insert(OverlaySpriteKey {
                name: name.clone(),
                frame,
            });
        }
    }
    if needed.is_empty() {
        return None;
    }

    let mut rendered: Vec<RenderedBridge> = Vec::with_capacity(needed.len());
    for key in &needed {
        let flags: OverlayTypeFlags = overlay_registry
            .flags_by_name(&key.name)
            .cloned()
            .unwrap_or_default();
        let palette: &Palette = if flags.wall {
            unit_palette
        } else {
            theater_palette
        };
        if let Some(sprite) = render_bridge_sprite(
            asset_manager,
            palette,
            key,
            theater_ext,
            theater_name,
            rules_ini,
            art_registry,
            &flags,
        ) {
            rendered.push(sprite);
        }
    }
    if rendered.is_empty() {
        return None;
    }

    Some(pack_bridge_sprites(gpu, batch, &rendered))
}

fn render_bridge_sprite(
    asset_manager: &AssetManager,
    palette: &Palette,
    key: &OverlaySpriteKey,
    theater_ext: &str,
    theater_name: &str,
    rules_ini: &IniFile,
    art_registry: &ArtRegistry,
    flags: &OverlayTypeFlags,
) -> Option<RenderedBridge> {
    let image_id: String = art_registry.resolve_overlay_image_id(&key.name, rules_ini);
    let mut candidates: Vec<String> = art_data::overlay_shp_candidates(
        Some(art_registry),
        &key.name,
        &image_id,
        theater_ext,
        theater_name,
    );
    if let Some(alias) = decrement_numeric_suffix(&key.name) {
        candidates.push(format!("{}.{}", alias, theater_ext));
        candidates.push(format!("{}.shp", alias));
        candidates.push(format!("{}.{}", alias.to_ascii_lowercase(), theater_ext));
        candidates.push(format!("{}.shp", alias.to_ascii_lowercase()));
    }

    let shp: ShpFile = candidates.iter().find_map(|name| {
        let data = asset_manager.get_ref(name)?;
        let shp = ShpFile::from_bytes(data).ok()?;
        let has_drawable = shp
            .frames
            .iter()
            .any(|fr| fr.frame_width > 0 && fr.frame_height > 0);
        has_drawable.then_some(shp)
    })?;

    let max_normal_frame: usize = if flags.bridge_deck {
        shp.frames.len() / 2
    } else {
        shp.frames.len()
    };
    let requested_idx: usize = (key.frame as usize).min(max_normal_frame.saturating_sub(1));
    let mut frame_idx = requested_idx;
    if !shp
        .frames
        .get(frame_idx)
        .is_some_and(|fr| fr.frame_width > 0 && fr.frame_height > 0)
    {
        frame_idx = shp
            .frames
            .iter()
            .take(max_normal_frame)
            .enumerate()
            .find(|(_, fr)| fr.frame_width > 0 && fr.frame_height > 0)
            .map(|(idx, _)| idx)?;
    }

    let frame = &shp.frames[frame_idx];
    let frame_rgba: Vec<u8> = shp.frame_to_rgba(frame_idx, palette).ok()?;
    let full_w: u32 = shp.width as u32;
    let full_h: u32 = shp.height as u32;
    let mut full_rgba: Vec<u8> = vec![0u8; (full_w * full_h * 4) as usize];

    let fw: u32 = frame.frame_width as u32;
    let fh: u32 = frame.frame_height as u32;
    let fx: u32 = frame.frame_x as u32;
    let fy: u32 = frame.frame_y as u32;
    for y in 0..fh {
        let dst_y: u32 = fy + y;
        if dst_y >= full_h {
            break;
        }
        let src_off: usize = (y * fw * 4) as usize;
        let dst_off: usize = ((dst_y * full_w + fx) * 4) as usize;
        let copy_w: u32 = fw.min(full_w.saturating_sub(fx));
        let bytes: usize = (copy_w * 4) as usize;
        if src_off + bytes <= frame_rgba.len() && dst_off + bytes <= full_rgba.len() {
            full_rgba[dst_off..dst_off + bytes]
                .copy_from_slice(&frame_rgba[src_off..src_off + bytes]);
        }
    }

    Some(RenderedBridge {
        key: key.clone(),
        rgba: full_rgba,
        width: full_w,
        height: full_h,
        offset_x: -(full_w as f32) / 2.0,
        offset_y: -(full_h as f32) / 2.0 + flags.y_draw_offset(),
    })
}

fn pack_bridge_sprites(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    sprites: &[RenderedBridge],
) -> BridgeAtlas {
    let mut indices: Vec<usize> = (0..sprites.len()).collect();
    indices.sort_by(|&a, &b| sprites[b].height.cmp(&sprites[a].height));

    let total_area: u64 = sprites
        .iter()
        .map(|s| {
            (s.width as u64 + SPRITE_PADDING as u64) * (s.height as u64 + SPRITE_PADDING as u64)
        })
        .sum();
    let estimated_side: u32 = (total_area as f64).sqrt().ceil() as u32;
    let max_texture_dim: u32 = gpu.device.limits().max_texture_dimension_2d;
    let mut atlas_width: u32 = estimated_side.clamp(64, max_texture_dim);

    let placements: Vec<(usize, u32, u32)>;
    let atlas_height: u32;
    loop {
        let mut trial: Vec<(usize, u32, u32)> = Vec::with_capacity(sprites.len());
        let mut cx: u32 = 0;
        let mut cy: u32 = 0;
        let mut shelf_h: u32 = 0;
        for &idx in &indices {
            let w: u32 = sprites[idx].width;
            let h: u32 = sprites[idx].height;
            if cx + w > atlas_width {
                cy += shelf_h + SPRITE_PADDING;
                cx = 0;
                shelf_h = 0;
            }
            trial.push((idx, cx, cy));
            cx += w + SPRITE_PADDING;
            shelf_h = shelf_h.max(h);
        }
        let trial_height: u32 = trial
            .iter()
            .map(|&(idx, _, py)| py + sprites[idx].height)
            .max()
            .unwrap_or(1);
        if trial_height <= max_texture_dim {
            placements = trial;
            atlas_height = trial_height;
            break;
        }
        if atlas_width >= max_texture_dim {
            placements = trial;
            atlas_height = trial_height.min(max_texture_dim);
            break;
        }
        atlas_width = (atlas_width.saturating_mul(2)).min(max_texture_dim);
    }

    let mut rgba: Vec<u8> = vec![0u8; (atlas_width * atlas_height * 4) as usize];
    let mut entries: HashMap<OverlaySpriteKey, OverlaySpriteEntry> =
        HashMap::with_capacity(placements.len());
    let aw: f32 = atlas_width as f32;
    let ah: f32 = atlas_height as f32;

    for &(idx, px, py) in &placements {
        let spr: &RenderedBridge = &sprites[idx];
        let w: u32 = spr.width;
        let h: u32 = spr.height;
        for y in 0..h {
            let src_start: usize = (y * w * 4) as usize;
            let src_end: usize = src_start + (w * 4) as usize;
            let dst_start: usize = (((py + y) * atlas_width + px) * 4) as usize;
            let dst_end: usize = dst_start + (w * 4) as usize;
            if src_end <= spr.rgba.len() && dst_end <= rgba.len() {
                rgba[dst_start..dst_end].copy_from_slice(&spr.rgba[src_start..src_end]);
            }
        }
        entries.insert(
            spr.key.clone(),
            OverlaySpriteEntry {
                uv_origin: [px as f32 / aw, py as f32 / ah],
                uv_size: [w as f32 / aw, h as f32 / ah],
                pixel_size: [w as f32, h as f32],
                offset_x: spr.offset_x,
                offset_y: spr.offset_y,
            },
        );
    }

    let texture: BatchTexture = batch.create_texture(gpu, &rgba, atlas_width, atlas_height);
    let depth_texture_view = create_r8_texture(
        gpu,
        &vec![0u8; (atlas_width * atlas_height) as usize],
        atlas_width,
        atlas_height,
    );
    let zdepth_bind_group = batch.create_zdepth_bind_group(gpu, &texture.view, &depth_texture_view);

    BridgeAtlas {
        texture,
        depth_texture_view,
        zdepth_bind_group,
        entries,
    }
}

fn create_r8_texture(gpu: &GpuContext, data: &[u8], width: u32, height: u32) -> wgpu::TextureView {
    let texture = gpu.device.create_texture_with_data(
        &gpu.queue,
        &wgpu::TextureDescriptor {
            label: Some("Bridge Depth Atlas R8"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        data,
    );
    texture.create_view(&Default::default())
}

fn decrement_numeric_suffix(name: &str) -> Option<String> {
    let split: usize = name.rfind(|c: char| !c.is_ascii_digit())?;
    if split + 1 >= name.len() {
        return None;
    }
    let (prefix, digits) = name.split_at(split + 1);
    let width: usize = digits.len();
    let n: u32 = digits.parse().ok()?;
    if n == 0 {
        return None;
    }
    Some(format!("{}{:0width$}", prefix, n - 1, width = width))
}
