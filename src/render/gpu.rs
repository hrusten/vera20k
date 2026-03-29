//! wgpu device, queue, and surface initialization.
//!
//! GpuContext is created once during app startup and lives for the entire session.
//! It holds all the wgpu state needed for rendering. Other render modules
//! (sprite.rs, terrain.rs, etc.) borrow device/queue from here.
//!
//! ## Why pollster::block_on?
//! wgpu's initialization is async (request_adapter, request_device), but winit's
//! ApplicationHandler::resumed() is sync. pollster::block_on bridges this gap.
//! This is the standard pattern for desktop wgpu apps.
//!
//! ## Why Arc<Window>?
//! wgpu::Surface requires the window to live as long as the surface ('static lifetime).
//! Wrapping Window in Arc satisfies this because Arc provides 'static ownership.
//!
//! ## Dependency rules
//! - gpu.rs is part of render/ — see render/mod.rs for dependency rules.

use std::sync::Arc;

use anyhow::{Context, Result};
use winit::window::Window;

/// Holds all wgpu state needed for rendering.
///
/// Created once during app initialization when the window becomes available.
/// Other render modules borrow `device` and `queue` from this struct.
pub struct GpuContext {
    /// The GPU surface we render to (tied to the window).
    pub surface: wgpu::Surface<'static>,
    /// The logical GPU device — used to create buffers, textures, pipelines.
    pub device: wgpu::Device,
    /// The command queue — used to submit render commands to the GPU.
    pub queue: wgpu::Queue,
    /// Current surface configuration (format, size, present mode).
    pub config: wgpu::SurfaceConfiguration,
    /// The texture format the surface uses (needed when creating pipelines).
    pub surface_format: wgpu::TextureFormat,
}

impl GpuContext {
    /// Initialize wgpu with the given window.
    ///
    /// This blocks on async wgpu calls using pollster. Safe to call from
    /// winit's sync ApplicationHandler::resumed().
    pub fn new(window: Arc<Window>) -> Result<Self> {
        pollster::block_on(Self::new_async(window))
    }

    /// Async initialization — called by new() via pollster::block_on().
    async fn new_async(window: Arc<Window>) -> Result<Self> {
        // Create wgpu instance with primary backends (Vulkan on Windows/Linux, Metal on Mac).
        let instance: wgpu::Instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        // Create the surface from our window. The surface is what we draw pixels to.
        let surface: wgpu::Surface<'static> = instance
            .create_surface(window.clone())
            .context("Failed to create wgpu surface from window")?;

        // Request a GPU adapter (physical device) that can render to our surface.
        // HighPerformance prefers discrete GPUs over integrated ones.
        let adapter: wgpu::Adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("No suitable GPU adapter found — is a GPU available?")?;

        log::info!("Using GPU adapter: {}", adapter.get_info().name);

        // Request a logical device and command queue from the adapter.
        // We don't need any special features or limits for now.
        let (device, queue): (wgpu::Device, wgpu::Queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("RA2 Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .context("Failed to create wgpu device")?;

        // Pick the best texture format for the surface.
        // This is usually Bgra8UnormSrgb on Windows, Bgra8Unorm on some other platforms.
        let surface_caps: wgpu::SurfaceCapabilities = surface.get_capabilities(&adapter);
        let surface_format: wgpu::TextureFormat = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        // Configure the surface with our chosen format and the window's current size.
        let window_size: winit::dpi::PhysicalSize<u32> = window.inner_size();
        let config: wgpu::SurfaceConfiguration = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            format: surface_format,
            width: window_size.width.max(1), // wgpu panics on 0-sized surfaces
            height: window_size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        log::info!(
            "GPU initialized: {}x{}, format={:?}",
            config.width,
            config.height,
            surface_format
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            surface_format,
        })
    }

    /// Handle window resize — reconfigure the surface with new dimensions.
    ///
    /// Called from ApplicationHandler::window_event when WindowEvent::Resized fires.
    /// Ignores zero-sized dimensions (happens during window minimize on some platforms).
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return; // Minimized window — skip reconfigure
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        log::debug!("Surface resized to {}x{}", width, height);
    }

    /// Create a depth texture matching the current surface dimensions.
    ///
    /// Must be recreated whenever the window is resized (surface dimensions change).
    /// Uses Depth32Float format matching the batch pipeline's depth_stencil state.
    pub fn create_depth_texture(&self) -> wgpu::TextureView {
        let texture: wgpu::Texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width: self.config.width.max(1),
                height: self.config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        texture.create_view(&Default::default())
    }

    /// Render a single frame with the given clear color (RGB, 0.0–1.0 range).
    ///
    /// This is the simplest possible render pass — just fills the screen with one color.
    /// Used during early development to verify the GPU pipeline works.
    /// Later replaced by actual terrain/sprite/UI rendering.
    pub fn render_clear(&self, r: f64, g: f64, b: f64) -> Result<()> {
        // Get the next framebuffer texture from the surface.
        let output: wgpu::SurfaceTexture = self
            .surface
            .get_current_texture()
            .context("Failed to get surface texture — surface may be lost")?;

        // Create a view into the texture for the render pass.
        let view: wgpu::TextureView = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Encode a render pass that clears the screen to our color.
        let mut encoder: wgpu::CommandEncoder =
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Clear Encoder"),
                });

        {
            // The render pass is scoped — it ends when _pass is dropped.
            let _pass: wgpu::RenderPass<'_> =
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Clear Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                        // depth_slice is for 3D texture array layers — None for normal 2D rendering.
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
        }

        // Submit the encoded commands to the GPU and present the frame.
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}
