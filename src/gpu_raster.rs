//! GPU palette lookup for the symbol map. Simulation stays on the CPU.

use iced::mouse;
use iced::wgpu;
use iced::widget::shader::{self, Viewport};
use iced::widget::Shader;
use iced::{Rectangle, Size};

use crate::machine::MAX_SYMBOLS;
use crate::palette::Rgb;

/// How the symbol map is turned into colours for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RasterMode {
    #[default]
    Gpu,
    Cpu,
}

impl RasterMode {
    pub const ALL: [RasterMode; 2] = [Self::Gpu, Self::Cpu];
}

impl std::fmt::Display for RasterMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Gpu => "GPU",
            Self::Cpu => "CPU",
        })
    }
}

/// Pack the i32 tape into bytes for an R8Uint texture upload.
pub fn pack_map(map: &[i32]) -> Vec<u8> {
    map.iter().map(|&sy| sy as u8).collect()
}

/// CPU reference for the GPU fragment shader (contain / nearest / palette LUT).
#[allow(clippy::too_many_arguments)]
pub fn gpu_lookup_rgba(
    map: &[i32],
    width: usize,
    height: usize,
    colors: &[Rgb; MAX_SYMBOLS],
    widget_w: f32,
    widget_h: f32,
    x: f32,
    y: f32,
) -> [u8; 4] {
    if width == 0 || height == 0 || widget_w <= 0.0 || widget_h <= 0.0 {
        return [0, 0, 0, 255];
    }
    let map_w = width as f32;
    let map_h = height as f32;
    let scale = (widget_w / map_w).min(widget_h / map_h);
    let drawn_w = map_w * scale;
    let drawn_h = map_h * scale;
    let origin_x = (widget_w - drawn_w) * 0.5;
    let origin_y = (widget_h - drawn_h) * 0.5;
    let local_x = x - origin_x;
    let local_y = y - origin_y;
    if local_x < 0.0 || local_y < 0.0 || local_x >= drawn_w || local_y >= drawn_h {
        return [0, 0, 0, 255];
    }
    let cx = (local_x / scale).floor() as usize;
    let cy = (local_y / scale).floor() as usize;
    if cx >= width || cy >= height {
        return [0, 0, 0, 255];
    }
    let sy = map[cy * width + cx] as usize;
    let c = colors[sy.min(MAX_SYMBOLS - 1)];
    [c[0], c[1], c[2], 255]
}

/// Shader widget that colorizes `map` with `colors` (contain + nearest).
pub fn map_shader<'a, Message>(
    map: &'a [i32],
    width: u32,
    height: u32,
    colors: &'a [Rgb; MAX_SYMBOLS],
) -> Shader<Message, RasterProgram<'a>> {
    iced::widget::shader(RasterProgram {
        map,
        width,
        height,
        colors: *colors,
    })
    .width(iced::Length::Fill)
    .height(iced::Length::Fill)
}

pub struct RasterProgram<'a> {
    map: &'a [i32],
    width: u32,
    height: u32,
    colors: [Rgb; MAX_SYMBOLS],
}

impl<Message> shader::Program<Message> for RasterProgram<'_> {
    type State = ();
    type Primitive = RasterPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        bounds: Rectangle,
    ) -> Self::Primitive {
        let _ = bounds;
        RasterPrimitive {
            packed: pack_map(self.map),
            width: self.width.max(1),
            height: self.height.max(1),
            colors: self.colors,
        }
    }
}

#[derive(Debug)]
pub struct RasterPrimitive {
    packed: Vec<u8>,
    width: u32,
    height: u32,
    colors: [Rgb; MAX_SYMBOLS],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    palette: [[f32; 4]; 8],
    map_size: [f32; 2],
    widget_size: [f32; 2],
    letterbox: [f32; 4],
}

const UNIFORM_SIZE: u64 = 256;

pub struct RasterPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    uniform_buf: wgpu::Buffer,
    map_texture: wgpu::Texture,
    map_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    tex_size: (u32, u32),
}

impl RasterPipeline {
    fn create_map_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("turing map"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    fn bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform_buf: &wgpu::Buffer,
        map_view: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("turing map bind group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(map_view),
                },
            ],
        })
    }

    fn resize_map(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.tex_size == (width, height) {
            return;
        }
        self.map_texture = Self::create_map_texture(device, width, height);
        self.map_view = self
            .map_texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = Self::bind_group(
            device,
            &self.bind_group_layout,
            &self.uniform_buf,
            &self.map_view,
        );
        self.tex_size = (width, height);
    }

    fn upload_map(&self, queue: &wgpu::Queue, packed: &[u8], width: u32, height: u32) {
        let bpr = padded_bytes_per_row(width);
        let data;
        let bytes: &[u8] = if bpr == width {
            packed
        } else {
            data = pad_rows(packed, width, height, bpr);
            &data
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.map_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bpr),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn upload_uniforms(&self, queue: &wgpu::Queue, uniforms: &Uniforms) {
        let bytes = uniforms_bytes(uniforms);
        queue.write_buffer(&self.uniform_buf, 0, &bytes);
    }
}

impl shader::Pipeline for RasterPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("turing map shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "shaders/map.wgsl"
            ))),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("turing map bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Uint,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("turing map pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("turing map pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("turing map uniforms"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let map_texture = Self::create_map_texture(device, 1, 1);
        let map_view = map_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = Self::bind_group(device, &bind_group_layout, &uniform_buf, &map_view);

        Self {
            pipeline,
            bind_group_layout,
            uniform_buf,
            map_texture,
            map_view,
            bind_group,
            tex_size: (1, 1),
        }
    }
}

impl shader::Primitive for RasterPrimitive {
    type Pipeline = RasterPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let width = self.width;
        let height = self.height;
        pipeline.resize_map(device, width, height);
        pipeline.upload_map(queue, &self.packed, width, height);

        let scale = viewport.scale_factor();
        let size = Size::new(
            (bounds.width * scale).max(1.0),
            (bounds.height * scale).max(1.0),
        );
        let uniforms = Uniforms {
            palette: palette_to_vec4(&self.colors),
            map_size: [width as f32, height as f32],
            widget_size: [size.width, size.height],
            letterbox: [0.0, 0.0, 0.0, 1.0],
        };
        pipeline.upload_uniforms(queue, &uniforms);
    }

    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        if clip_bounds.width == 0 || clip_bounds.height == 0 {
            return;
        }

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("turing map pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_viewport(
            clip_bounds.x as f32,
            clip_bounds.y as f32,
            clip_bounds.width as f32,
            clip_bounds.height as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn palette_to_vec4(colors: &[Rgb; MAX_SYMBOLS]) -> [[f32; 4]; 8] {
    let mut out = [[0.0f32; 4]; 8];
    for (i, c) in colors.iter().enumerate() {
        out[i] = [
            f32::from(c[0]) / 255.0,
            f32::from(c[1]) / 255.0,
            f32::from(c[2]) / 255.0,
            1.0,
        ];
    }
    out
}

fn uniforms_bytes(u: &Uniforms) -> [u8; std::mem::size_of::<Uniforms>()] {
    unsafe { std::mem::transmute_copy(u) }
}

fn padded_bytes_per_row(width: u32) -> u32 {
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let unpadded = width;
    unpadded.div_ceil(align) * align
}

fn pad_rows(packed: &[u8], width: u32, height: u32, bpr: u32) -> Vec<u8> {
    let mut out = vec![0u8; (bpr * height) as usize];
    let width = width as usize;
    let bpr = bpr as usize;
    for y in 0..height as usize {
        let src = y * width;
        let dst = y * bpr;
        out[dst..dst + width].copy_from_slice(&packed[src..src + width]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_map_truncates_to_u8() {
        assert_eq!(pack_map(&[0, 3, 7]), vec![0, 3, 7]);
    }

    #[test]
    fn lookup_center_cell_uses_palette() {
        let map = [0, 1, 2, 3];
        let mut colors = [[0u8; 3]; MAX_SYMBOLS];
        colors[1] = [10, 20, 30];
        let rgba = gpu_lookup_rgba(&map, 2, 2, &colors, 4.0, 4.0, 2.5, 0.5);
        assert_eq!(rgba, [10, 20, 30, 255]);
    }

    #[test]
    fn lookup_letterbox_is_black() {
        let map = [1];
        let mut colors = [[0u8; 3]; MAX_SYMBOLS];
        colors[1] = [255, 0, 0];
        let rgba = gpu_lookup_rgba(&map, 1, 1, &colors, 4.0, 2.0, 0.2, 1.0);
        assert_eq!(rgba, [0, 0, 0, 255]);
    }

    #[test]
    fn row_padding_aligns_to_256() {
        assert_eq!(padded_bytes_per_row(512), 512);
        assert_eq!(padded_bytes_per_row(1470), 1536);
    }

    #[test]
    fn gpu_lookup_matches_cpu_rgba_when_widget_matches_map() {
        let map = vec![0, 1, 2, 3, 4, 5, 6, 7];
        let colors = crate::palette::Palette::classic().colors;
        let pixels = crate::palette::rgba_from_map(&map, &colors);
        let (width, height) = (4usize, 2usize);
        for y in 0..height {
            for x in 0..width {
                let rgba = gpu_lookup_rgba(
                    &map,
                    width,
                    height,
                    &colors,
                    width as f32,
                    height as f32,
                    x as f32 + 0.5,
                    y as f32 + 0.5,
                );
                let o = (y * width + x) * 4;
                assert_eq!(
                    rgba,
                    [pixels[o], pixels[o + 1], pixels[o + 2], pixels[o + 3]],
                    "cell ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn uniforms_are_tightly_packed() {
        assert_eq!(std::mem::size_of::<Uniforms>(), 160);
    }
}
