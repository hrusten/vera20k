//! egui rendering integration — bridges egui, winit, and wgpu.
//!
//! Owns the egui Context, the egui-winit input translator (State),
//! and the egui-wgpu Renderer that draws egui output to GPU textures.
//!
//! ## Dependency rules
//! - Part of render/ — depends on render/gpu for GpuContext.
//! - Does NOT depend on ui/, sim/, or any game logic.
//! - App.rs creates this and passes it to UI draw functions.

use winit::window::Window;

use crate::render::gpu::GpuContext;

/// All egui state needed for input handling and rendering.
///
/// Created once in App::initialize() alongside the GpuContext.
/// Holds the egui_winit::State (input translator) and egui_wgpu::Renderer
/// (GPU draw pipeline).
pub struct EguiIntegration {
    /// The egui context — shared across all egui frames.
    /// Holds style, fonts, memory, and the current frame's UI state.
    pub ctx: egui::Context,

    /// egui-winit input state. Translates winit WindowEvents into egui RawInput.
    /// Must be fed every event via on_window_event().
    state: egui_winit::State,

    /// egui-wgpu renderer. Manages GPU buffers and textures for egui output.
    /// Does NOT use a depth buffer — egui renders flat UI elements.
    renderer: egui_wgpu::Renderer,
}

impl EguiIntegration {
    /// Create a new EguiIntegration.
    ///
    /// Called once during app initialization after the GPU context is ready.
    /// `window` is needed for egui-winit to query scale factor and set up input.
    pub fn new(gpu: &GpuContext, window: &Window) -> Self {
        let ctx: egui::Context = egui::Context::default();

        // egui-winit State handles all input translation:
        // mouse, keyboard, touch, clipboard, IME, etc.
        let state: egui_winit::State = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            window,
            Some(window.scale_factor() as f32),
            None,
            None,
        );

        // egui-wgpu Renderer compiles the egui shader and sets up GPU pipelines.
        // No depth format — egui draws flat, no MSAA — egui anti-aliases via feathering.
        let renderer: egui_wgpu::Renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.surface_format,
            egui_wgpu::RendererOptions {
                depth_stencil_format: None,
                msaa_samples: 1,
                dithering: true,
                ..Default::default()
            },
        );

        // Load Verdana as the proportional font for a cleaner UI look.
        load_system_font(&ctx);

        Self {
            ctx,
            state,
            renderer,
        }
    }

    /// Feed a winit event to egui. Returns whether egui consumed it.
    ///
    /// Call this in window_event() BEFORE handling game-specific input.
    /// If `consumed` is true, the event was used by egui (e.g., mouse click
    /// on a button) and should NOT be passed to game input handling.
    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &winit::event::WindowEvent,
    ) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    /// Begin an egui frame. Call once per frame before running UI code.
    ///
    /// Collects accumulated input from on_window_event() calls and
    /// starts the egui frame. After this, call egui UI functions
    /// (panels, windows, etc.), then call end_frame_and_render().
    pub fn begin_frame(&mut self, window: &Window) {
        let raw_input: egui::RawInput = self.state.take_egui_input(window);
        self.ctx.begin_pass(raw_input);
    }

    /// End the egui frame, tessellate, and render to the given surface view.
    ///
    /// This:
    /// 1. Ends the egui pass and collects paint jobs.
    /// 2. Handles platform output (cursor changes, clipboard, etc.).
    /// 3. Updates egui GPU buffers with new vertices/textures.
    /// 4. Creates a render pass WITHOUT depth and draws egui on top.
    ///
    /// Must be called AFTER any game render pass so egui draws on top.
    /// Uses LoadOp::Load to preserve whatever was drawn before.
    pub fn end_frame_and_render(
        &mut self,
        gpu: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        window: &Window,
        has_software_cursor: bool,
    ) {
        let full_output: egui::FullOutput = self.ctx.end_pass();

        // Handle platform output (clipboard, open URL, IME, etc.).
        let mut platform_output = full_output.platform_output;
        if has_software_cursor {
            platform_output.cursor_icon = egui::CursorIcon::None;
        }
        self.state.handle_platform_output(window, platform_output);
        // egui-winit caches cursor_icon and skips set_cursor_visible(false) when
        // the icon hasn't changed between frames. Force it hidden unconditionally
        // so the OS cursor never flashes on clicks or other non-move events.
        if has_software_cursor {
            window.set_cursor_visible(false);
        }

        // Tessellate egui shapes into GPU-ready triangles.
        let paint_jobs: Vec<egui::epaint::ClippedPrimitive> = self
            .ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        let screen_descriptor: egui_wgpu::ScreenDescriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [gpu.config.width, gpu.config.height],
            pixels_per_point: full_output.pixels_per_point,
        };

        // Upload texture deltas (font atlas changes, user textures).
        for (id, image_delta) in &full_output.textures_delta.set {
            self.renderer
                .update_texture(&gpu.device, &gpu.queue, *id, image_delta);
        }

        // Upload vertex/index buffers. Returns extra command buffers for
        // operations that couldn't go through the encoder (rare).
        let extra_cmd_bufs: Vec<wgpu::CommandBuffer> = self.renderer.update_buffers(
            &gpu.device,
            &gpu.queue,
            encoder,
            &paint_jobs,
            &screen_descriptor,
        );
        // Submit extra command buffers immediately if any exist. These contain
        // preparatory work (texture uploads) the render pass will reference.
        if !extra_cmd_bufs.is_empty() {
            gpu.queue.submit(extra_cmd_bufs);
        }

        // Render egui in its own pass — NO depth buffer, LoadOp::Load preserves game content.
        {
            let render_pass: wgpu::RenderPass<'_> =
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            // forget_lifetime() is required because Renderer::render expects
            // RenderPass<'static>. Safe — wgpu keeps resources alive internally.
            let mut render_pass: wgpu::RenderPass<'static> = render_pass.forget_lifetime();
            self.renderer
                .render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }

        // Free any textures that egui no longer needs.
        for id in &full_output.textures_delta.free {
            self.renderer.free_texture(id);
        }
    }
}

/// Load Verdana (or Calibri fallback) as the default proportional font.
/// Falls back silently to egui's built-in font if neither is found.
fn load_system_font(ctx: &egui::Context) {
    let font_path = if std::path::Path::new("C:/Windows/Fonts/verdana.ttf").exists() {
        "C:/Windows/Fonts/verdana.ttf"
    } else if std::path::Path::new("C:/Windows/Fonts/calibri.ttf").exists() {
        "C:/Windows/Fonts/calibri.ttf"
    } else {
        log::info!("No Verdana/Calibri found — using egui default font");
        return;
    };

    let Ok(font_bytes) = std::fs::read(font_path) else {
        log::warn!("Failed to read font file: {}", font_path);
        return;
    };
    log::info!(
        "Loaded system font: {} ({} bytes)",
        font_path,
        font_bytes.len()
    );

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "sidebar_font".to_string(),
        egui::FontData::from_owned(font_bytes).into(),
    );
    // Insert at the front of the Proportional family so it takes priority.
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "sidebar_font".to_string());
    ctx.set_fonts(fonts);
}
