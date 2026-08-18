//! GPU display of the baked RGBA canvas. Simulation stays on the CPU.

use iced::mouse;
use iced::wgpu;
use iced::widget::shader::{self, Viewport};
use iced::widget::Shader;
use iced::Rectangle;
use serde::{Deserialize, Serialize};

/// How the canvas is shown on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

/// Precomputed contain-fit letterbox layout, shared between CPU lookup and GPU uniforms.
struct ContainLayout {
    origin_x: f32,
    origin_y: f32,
    inv_scale: f32,
    drawn_w: f32,
    drawn_h: f32,
}

fn contain_layout(
    map_w: f32,
    map_h: f32,
    widget_w: f32,
    widget_h: f32,
) -> Option<ContainLayout> {
    if map_w <= 0.0 || map_h <= 0.0 || widget_w <= 0.0 || widget_h <= 0.0 {
        return None;
    }
    let scale = (widget_w / map_w).min(widget_h / map_h);
    if scale <= 0.0 {
        return None;
    }
    let drawn_w = map_w * scale;
    let drawn_h = map_h * scale;
    Some(ContainLayout {
        origin_x: (widget_w - drawn_w) * 0.5,
        origin_y: (widget_h - drawn_h) * 0.5,
        inv_scale: 1.0 / scale,
        drawn_w,
        drawn_h,
    })
}

/// Map a widget-local point onto a tape cell using contain + letterbox.
pub fn map_cell_at(
    map_width: usize,
    map_height: usize,
    widget_w: f32,
    widget_h: f32,
    x: f32,
    y: f32,
) -> Option<(i32, i32)> {
    let cl = contain_layout(map_width as f32, map_height as f32, widget_w, widget_h)?;
    let local_x = x - cl.origin_x;
    let local_y = y - cl.origin_y;
    if local_x < 0.0 || local_y < 0.0 || local_x >= cl.drawn_w || local_y >= cl.drawn_h {
        return None;
    }
    let cx = (local_x * cl.inv_scale).floor() as i32;
    let cy = (local_y * cl.inv_scale).floor() as i32;
    if cx < 0 || cy < 0 || (cx as usize) >= map_width || (cy as usize) >= map_height {
        return None;
    }
    Some((cx, cy))
}

/// CPU reference for the GPU fragment shader (contain / nearest / baked RGBA).
pub fn gpu_lookup_rgba(
    canvas: &[u8],
    width: usize,
    height: usize,
    widget_w: f32,
    widget_h: f32,
    x: f32,
    y: f32,
) -> [u8; 4] {
    let Some((cx, cy)) = map_cell_at(width, height, widget_w, widget_h, x, y) else {
        return [0, 0, 0, 255];
    };
    let o = (cy as usize * width + cx as usize) * 4;
    [canvas[o], canvas[o + 1], canvas[o + 2], canvas[o + 3]]
}

/// Shader widget that displays the baked `canvas` (contain + nearest).
pub fn canvas_shader<'a, Message>(
    canvas: &'a [u8],
    width: u32,
    height: u32,
) -> Shader<Message, RasterProgram<'a>> {
    iced::widget::shader(RasterProgram {
        canvas,
        width,
        height,
    })
    .width(iced::Length::Fill)
    .height(iced::Length::Fill)
}

pub struct RasterProgram<'a> {
    canvas: &'a [u8],
    width: u32,
    height: u32,
}

impl<Message> shader::Program<Message> for RasterProgram<'_> {
    type State = ();
    type Primitive = RasterPrimitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        RasterPrimitive {
            canvas: self.canvas,
            width: self.width.max(1),
            height: self.height.max(1),
        }
    }
}

/// Borrows the canvas slice from `RasterProgram` instead of cloning it.
/// The lifetime is erased via a raw pointer; the borrow is valid because
/// `prepare` runs synchronously before `RasterProgram` is dropped.
pub struct RasterPrimitive {
    canvas: *const [u8],
    width: u32,
    height: u32,
}

unsafe impl Send for RasterPrimitive {}
unsafe impl Sync for RasterPrimitive {}

impl std::fmt::Debug for RasterPrimitive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RasterPrimitive")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

/// Uniforms with precomputed letterbox layout so the shader only does
/// a bounds check + `textureLoad`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Uniforms {
    /// xy = origin of the drawn rect in physical pixels.
    origin: [f32; 4],
    /// x = 1/scale (widget px → cell), yz = drawn rect size in physical pixels.
    inv_scale: [f32; 4],
    /// xy = map dimensions in cells.
    map_dims: [f32; 4],
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
    /// Reusable row-padding scratch buffer, resized only when canvas size changes.
    padded: Vec<u8>,
}

impl RasterPipeline {
    fn create_map_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("turing canvas"),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
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
            label: Some("turing canvas bind group"),
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
        let bpr = padded_bytes_per_row(width);
        if bpr != width * 4 {
            self.padded.resize((bpr * height) as usize, 0);
        } else {
            self.padded.clear();
        }
    }

    fn upload_map(&mut self, queue: &wgpu::Queue, canvas: &[u8], width: u32, height: u32) {
        let bpr = padded_bytes_per_row(width);
        let bytes: &[u8] = if bpr == width * 4 {
            canvas
        } else {
            pad_rows_into(canvas, width, height, bpr, &mut self.padded);
            &self.padded
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
            label: Some("turing canvas shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "shaders/map.wgsl"
            ))),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("turing canvas bgl"),
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("turing canvas pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("turing canvas pipeline"),
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
            label: Some("turing canvas uniforms"),
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
            padded: Vec::new(),
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
        let canvas = unsafe { &*self.canvas };
        pipeline.resize_map(device, width, height);
        pipeline.upload_map(queue, canvas, width, height);

        let scale = viewport.scale_factor();
        let widget_w = (bounds.width * scale).max(1.0);
        let widget_h = (bounds.height * scale).max(1.0);

        let uniforms = if let Some(cl) = contain_layout(
            width as f32,
            height as f32,
            widget_w,
            widget_h,
        ) {
            Uniforms {
                origin: [cl.origin_x, cl.origin_y, 0.0, 0.0],
                inv_scale: [cl.inv_scale, cl.drawn_w, cl.drawn_h, 0.0],
                map_dims: [width as f32, height as f32, widget_w, widget_h],
                letterbox: [0.0, 0.0, 0.0, 1.0],
            }
        } else {
            Uniforms {
                origin: [0.0; 4],
                inv_scale: [0.0; 4],
                map_dims: [0.0, 0.0, widget_w, widget_h],
                letterbox: [0.0, 0.0, 0.0, 1.0],
            }
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
            label: Some("turing canvas pass"),
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

fn uniforms_bytes(u: &Uniforms) -> [u8; UNIFORM_SIZE as usize] {
    let mut bytes = [0u8; UNIFORM_SIZE as usize];
    let src = unsafe {
        std::slice::from_raw_parts(
            (u as *const Uniforms).cast::<u8>(),
            std::mem::size_of::<Uniforms>(),
        )
    };
    bytes[..src.len()].copy_from_slice(src);
    bytes
}

fn padded_bytes_per_row(width: u32) -> u32 {
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let unpadded = width * 4;
    unpadded.div_ceil(align) * align
}

/// Copy canvas rows into a pre-allocated `out` buffer with row padding.
fn pad_rows_into(canvas: &[u8], width: u32, height: u32, bpr: u32, out: &mut Vec<u8>) {
    let needed = (bpr * height) as usize;
    if out.len() < needed {
        out.resize(needed, 0);
    }
    let row_bytes = width as usize * 4;
    let bpr = bpr as usize;
    for y in 0..height as usize {
        let src = y * row_bytes;
        let dst = y * bpr;
        out[dst..dst + row_bytes].copy_from_slice(&canvas[src..src + row_bytes]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_center_cell_uses_canvas() {
        let canvas = [0, 0, 0, 255, 10, 20, 30, 255, 0, 0, 0, 255, 0, 0, 0, 255];
        let rgba = gpu_lookup_rgba(&canvas, 2, 2, 4.0, 4.0, 2.5, 0.5);
        assert_eq!(rgba, [10, 20, 30, 255]);
    }

    #[test]
    fn lookup_letterbox_is_black() {
        let canvas = [255, 0, 0, 255];
        let rgba = gpu_lookup_rgba(&canvas, 1, 1, 4.0, 2.0, 0.2, 1.0);
        assert_eq!(rgba, [0, 0, 0, 255]);
    }

    #[test]
    fn map_cell_at_hits_cell_and_skips_letterbox() {
        assert_eq!(map_cell_at(2, 2, 4.0, 4.0, 2.5, 0.5), Some((1, 0)));
        assert_eq!(map_cell_at(2, 2, 4.0, 4.0, 0.5, 2.5), Some((0, 1)));
        assert_eq!(map_cell_at(1, 1, 4.0, 2.0, 0.2, 1.0), None);
        assert_eq!(map_cell_at(1, 1, 4.0, 2.0, 2.0, 1.0), Some((0, 0)));
        assert_eq!(map_cell_at(0, 1, 4.0, 4.0, 1.0, 1.0), None);
    }

    #[test]
    fn row_padding_aligns_to_256() {
        assert_eq!(padded_bytes_per_row(512), 2048);
        assert_eq!(padded_bytes_per_row(1470), 5888);
    }

    #[test]
    fn gpu_lookup_matches_canvas_when_widget_matches_map() {
        let canvas = vec![
            255, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255, 0, 255, 0, 255, 0, 0, 255, 255,
            255, 255, 0, 255, 0, 255, 255, 255, 255, 0, 255, 255,
        ];
        let (width, height) = (4usize, 2usize);
        for y in 0..height {
            for x in 0..width {
                let rgba = gpu_lookup_rgba(
                    &canvas,
                    width,
                    height,
                    width as f32,
                    height as f32,
                    x as f32 + 0.5,
                    y as f32 + 0.5,
                );
                let o = (y * width + x) * 4;
                assert_eq!(
                    rgba,
                    [canvas[o], canvas[o + 1], canvas[o + 2], canvas[o + 3]],
                    "cell ({x},{y})"
                );
            }
        }
    }

    #[test]
    fn uniforms_fit_uniform_buffer() {
        assert!(std::mem::size_of::<Uniforms>() <= UNIFORM_SIZE as usize);
        assert_eq!(UNIFORM_SIZE % 256, 0);
    }

    #[test]
    fn pad_rows_reuses_buffer() {
        let canvas = vec![1u8; 4 * 3 * 2]; // 3×2, 12 bytes/row
        let bpr = padded_bytes_per_row(3); // 256 aligned
        let mut buf = Vec::new();
        pad_rows_into(&canvas, 3, 2, bpr, &mut buf);
        assert!(buf.len() >= (bpr * 2) as usize);
        assert_eq!(&buf[0..12], &canvas[0..12]);
        let old_cap = buf.capacity();
        pad_rows_into(&canvas, 3, 2, bpr, &mut buf);
        assert_eq!(buf.capacity(), old_cap);
    }
}
