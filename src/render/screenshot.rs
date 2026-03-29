//! GPU screenshot capture — renders to an offscreen texture and saves as PNG.
//!
//! Used for development debugging: captures what the GPU actually renders,
//! bypassing any display/compositor issues. Called once on the first frame
//! with terrain data, then disabled via atomic flag.
//!
//! ## Dependency rules
//! - Part of render/ — depends on render/gpu, render/batch.

use crate::render::batch::{BatchRenderer, BatchTexture};
use crate::render::gpu::GpuContext;

/// Capture a screenshot by rendering to an offscreen texture and saving to PNG.
///
/// Creates a separate render pass on an offscreen texture, copies the result
/// to a CPU-readable buffer, converts BGRA→RGBA, and saves as PNG.
/// The `clear_color` should match the main render pass for consistency.
pub fn capture_screenshot(
    gpu: &GpuContext,
    batch: &BatchRenderer,
    atlas_texture: Option<&BatchTexture>,
    clear_color: wgpu::Color,
) {
    let w: u32 = gpu.config.width;
    let h: u32 = gpu.config.height;

    let offscreen: wgpu::Texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Screenshot"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu.surface_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let off_view: wgpu::TextureView = offscreen.create_view(&Default::default());

    let mut enc: wgpu::CommandEncoder =
        gpu.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Screenshot"),
            });
    {
        let mut pass: wgpu::RenderPass<'_> = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Screenshot Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &off_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        if let Some(tex) = atlas_texture {
            batch.draw_batch(&mut pass, tex);
        }
    }

    // Row alignment: wgpu requires rows to be aligned to 256 bytes.
    let bytes_per_row: u32 = (w * 4 + 255) & !255;
    let readback: wgpu::Buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Screenshot Buf"),
        size: (bytes_per_row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &offscreen,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit(std::iter::once(enc.finish()));

    // Map the buffer and read pixels back to CPU.
    let (tx, rx) = std::sync::mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = gpu.device.poll(wgpu::PollType::wait_indefinitely());

    if let Ok(Ok(())) = rx.recv() {
        let data = readback.slice(..).get_mapped_range();
        // Convert BGRA (surface format) → RGBA (PNG format).
        let mut rgba: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
        for row in 0..h {
            for col in 0..w {
                let i: usize = (row * bytes_per_row) as usize + (col * 4) as usize;
                if i + 3 < data.len() {
                    // BGRA → RGBA: swap blue and red channels.
                    rgba.extend_from_slice(&[data[i + 2], data[i + 1], data[i], data[i + 3]]);
                }
            }
        }
        if let Some(img) = image::RgbaImage::from_raw(w, h, rgba) {
            let _ = img.save("debug_screenshot.png");
            log::info!("Saved debug_screenshot.png ({}x{})", w, h);
        }
    }
}
