//! Catmull-Rom bicubic upscale post-process pass.
//!
//! Renders the game to an intermediate lower-resolution texture, then upscales
//! to the swapchain using an optimized 5-tap Catmull-Rom bicubic filter.
//! This produces sharper pixel-art scaling than bilinear without the blockiness
//! of nearest-neighbor.
//!
//! Depends on: `render/gpu.rs` (GpuContext)

use super::gpu::GpuContext;

const SHADER_SRC: &str = include_str!("upscale_catmull_rom.wgsl");

/// GPU resources for the Catmull-Rom upscale post-process pass.
#[allow(dead_code)] // Fields kept alive for GPU resource ownership.
pub struct UpscalePass {
    color_texture: wgpu::Texture,
    depth_texture: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    params_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    bgl: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    src_width: u32,
    src_height: u32,
}

impl UpscalePass {
    /// Create an upscale pass targeting a source resolution of `src_width x src_height`.
    pub fn new(gpu: &GpuContext, src_width: u32, src_height: u32) -> Self {
        let color_texture = create_color_texture(gpu, src_width, src_height);
        let depth_texture = create_depth_texture(gpu, src_width, src_height);
        let color_view = color_texture.create_view(&Default::default());
        let depth_view = depth_texture.create_view(&Default::default());

        let params_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Upscale Params"),
            size: 16, // vec2f + 8 bytes padding (uniform alignment)
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        gpu.queue.write_buffer(
            &params_buffer,
            0,
            bytemuck::cast_slice(&[src_width as f32, src_height as f32, 0.0f32, 0.0f32]),
        );

        let bgl = create_bgl(gpu);
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Upscale Sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let bind_group = create_bind_group(gpu, &bgl, &color_texture, &sampler, &params_buffer);
        let pipeline = create_pipeline(gpu, &bgl);

        Self {
            color_texture,
            depth_texture,
            color_view,
            depth_view,
            params_buffer,
            bind_group,
            bgl,
            pipeline,
            sampler,
            src_width,
            src_height,
        }
    }

    /// Cached view for the intermediate color render target.
    pub fn color_view(&self) -> &wgpu::TextureView {
        &self.color_view
    }

    /// Cached view for the intermediate depth buffer.
    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth_view
    }

    /// Source render width.
    pub fn src_width(&self) -> u32 {
        self.src_width
    }

    /// Source render height.
    pub fn src_height(&self) -> u32 {
        self.src_height
    }

    /// Draw the upscale pass: sample the intermediate texture and write to `target_view`.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, target_view: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Upscale Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..6, 0..1);
    }
}

// ---------------------------------------------------------------------------
// Resource creation helpers
// ---------------------------------------------------------------------------

fn create_color_texture(gpu: &GpuContext, width: u32, height: u32) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Upscale Color RT"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu.surface_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn create_depth_texture(gpu: &GpuContext, width: u32, height: u32) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Upscale Depth RT"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

fn create_bgl(gpu: &GpuContext) -> wgpu::BindGroupLayout {
    gpu.device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Upscale BGL"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        })
}

fn create_bind_group(
    gpu: &GpuContext,
    bgl: &wgpu::BindGroupLayout,
    color_texture: &wgpu::Texture,
    sampler: &wgpu::Sampler,
    params_buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let view = color_texture.create_view(&Default::default());
    gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Upscale BG"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params_buffer.as_entire_binding(),
            },
        ],
    })
}

fn create_pipeline(gpu: &GpuContext, bgl: &wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
    let shader = gpu
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Upscale Catmull-Rom Shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER_SRC.into()),
        });

    let layout = gpu
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Upscale Pipeline Layout"),
            bind_group_layouts: &[bgl],
            push_constant_ranges: &[],
        });

    gpu.device
        .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Upscale Catmull-Rom Pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.surface_format,
                    blend: None, // opaque overwrite
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None, // own render pass, no depth
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        })
}
