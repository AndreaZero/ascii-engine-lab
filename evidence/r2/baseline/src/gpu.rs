//! wgpu 26 display/capture backend: semantic cells -> font glyphs -> pixels.
//! API references: https://docs.rs/wgpu/26.0.1/wgpu/
//! https://docs.rs/wgpu-types/26.0.0/wgpu_types/enum.PollType.html
//! https://docs.rs/font8x8/0.3.1/font8x8/

use crate::core::GlyphCell;
use font8x8::UnicodeFonts;
use std::{error::Error, fs::File, io::BufWriter, path::Path, sync::Arc};
use wgpu::util::DeviceExt;
use winit::window::Window;

type GpuResult<T> = Result<T, Box<dyn Error>>;
const CAPTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

pub struct Gpu {
    // create_surface owns the Arc<Window>, so the surface is genuinely 'static.
    surface: Option<wgpu::Surface<'static>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: Option<wgpu::SurfaceConfiguration>,
    surface_pipeline: Option<wgpu::RenderPipeline>,
    capture_pipeline: wgpu::RenderPipeline,
    cells: wgpu::Buffer,
    view: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    offscreen: wgpu::Texture,
    width: u32,
    height: u32,
    cols: u32,
    rows: u32,
    suspended: bool,
    description: String,
}

impl Gpu {
    pub async fn new(
        window: Option<Arc<Window>>,
        width: u32,
        height: u32,
        cols: u32,
        rows: u32,
    ) -> GpuResult<Self> {
        if cols == 0 || rows == 0 {
            return Err("glyph grid dimensions must be nonzero".into());
        }
        let count = u64::from(cols) * u64::from(rows);
        let bytes = count
            .checked_mul(std::mem::size_of::<GlyphCell>() as u64)
            .ok_or("glyph buffer size overflow")?;
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::from_env_or_default());
        let surface = window
            .map(|window| instance.create_surface(window))
            .transpose()?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface.as_ref(),
                force_fallback_adapter: false,
            })
            .await?;
        let info = adapter.get_info();
        let description = format!(
            "{} | {:?} | {:?} | driver {} {}",
            info.name, info.backend, info.device_type, info.driver, info.driver_info
        );
        let limits = wgpu::Limits::default();
        if bytes > u64::from(limits.max_storage_buffer_binding_size)
            || bytes > limits.max_buffer_size
            || count > u64::from(u32::MAX)
        {
            return Err("glyph grid exceeds requested GPU buffer limits".into());
        }
        if width.max(1) > limits.max_texture_dimension_2d
            || height.max(1) > limits.max_texture_dimension_2d
        {
            return Err("output dimensions exceed requested GPU texture limits".into());
        }
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("ASCII glyph device"),
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                ..Default::default()
            })
            .await?;
        device.push_error_scope(wgpu::ErrorFilter::Validation);

        let cells = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("semantic glyph cells; no world image"),
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("glyph output dimensions"),
            contents: bytemuck::cast_slice(&[width.max(1), height.max(1), cols, rows]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // font8x8 BASIC_FONTS ships with the binary; no system font lookup.
        // Rows are top-to-bottom and each byte's least significant bit is x=0.
        let mut atlas_pixels = vec![0u8; 128 * 64];
        for code in 32u32..=126 {
            if let Some(bitmap) = font8x8::BASIC_FONTS.get(char::from_u32(code).unwrap()) {
                let origin_x = (code as usize % 16) * 8;
                let origin_y = (code as usize / 16) * 8;
                for (y, row) in bitmap.iter().enumerate() {
                    for x in 0..8 {
                        atlas_pixels[(origin_y + y) * 128 + origin_x + x] =
                            if row & (1 << x) != 0 { 255 } else { 0 };
                    }
                }
            }
        }
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bundled ASCII 8x8 atlas"),
            size: wgpu::Extent3d {
                width: 128,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            atlas.as_image_copy(),
            &atlas_pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(128),
                rows_per_image: Some(64),
            },
            atlas.size(),
        );
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
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
        });
        let atlas_view = atlas.create_view(&Default::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glyph data and font"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: cells.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: view.as_entire_binding(),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("direct ASCII compositor"),
            source: wgpu::ShaderSource::Wgsl(include_str!("glyph.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glyph pipeline layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let capture_pipeline = make_pipeline(&device, &pipeline_layout, &shader, CAPTURE_FORMAT);
        let config = surface.as_ref().map(|surface| {
            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| f.is_srgb())
                .unwrap_or(caps.formats[0]);
            wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: width.max(1),
                height: height.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: caps.alpha_modes[0],
                view_formats: vec![],
            }
        });
        let surface_pipeline = config
            .as_ref()
            .map(|config| make_pipeline(&device, &pipeline_layout, &shader, config.format));
        let suspended = width == 0 || height == 0;
        if !suspended {
            if let (Some(surface), Some(config)) = (&surface, &config) {
                surface.configure(&device, config);
            }
        }
        let offscreen = make_offscreen(&device, width.max(1), height.max(1));
        if let Some(error) = device.pop_error_scope().await {
            return Err(error.into());
        }
        Ok(Self {
            surface,
            device,
            queue,
            config,
            surface_pipeline,
            capture_pipeline,
            cells,
            view,
            bind_group,
            offscreen,
            width: width.max(1),
            height: height.max(1),
            cols,
            rows,
            suspended,
            description,
        })
    }

    pub fn adapter_description(&self) -> String {
        self.description.clone()
    }

    /// Keeps the semantic grid fixed; only the presentation extent changes.
    /// Zero-sized/minimized windows retain the last valid capture extent.
    pub fn resize(&mut self, width: u32, height: u32) {
        let max = self.device.limits().max_texture_dimension_2d;
        self.suspended = width == 0 || height == 0 || width > max || height > max;
        if self.suspended {
            return;
        }
        self.width = width;
        self.height = height;
        self.queue.write_buffer(
            &self.view,
            0,
            bytemuck::cast_slice(&[width, height, self.cols, self.rows]),
        );
        self.offscreen = make_offscreen(&self.device, width, height);
        if let (Some(surface), Some(config)) = (&self.surface, &mut self.config) {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
    }

    fn upload(&self, cells: &[GlyphCell]) -> GpuResult<()> {
        if cells.len() as u64 != u64::from(self.cols) * u64::from(self.rows) {
            return Err(format!(
                "expected {}x{} glyph cells, got {}",
                self.cols,
                self.rows,
                cells.len()
            )
            .into());
        }
        self.queue
            .write_buffer(&self.cells, 0, bytemuck::cast_slice(cells));
        Ok(())
    }

    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        pipeline: &wgpu::RenderPipeline,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ASCII glyph raster only"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    pub fn render(&mut self, cells: &[GlyphCell]) -> GpuResult<()> {
        if self.suspended {
            return Ok(());
        }
        let frame = if let Some(surface) = &self.surface {
            match surface.get_current_texture() {
                Ok(frame) => Some(frame),
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                    surface.configure(&self.device, self.config.as_ref().unwrap());
                    return Ok(()); // retry on the next redraw; do not spin on acquisition
                }
                Err(wgpu::SurfaceError::Timeout | wgpu::SurfaceError::Other) => return Ok(()),
                Err(error) => return Err(error.into()),
            }
        } else {
            None
        };
        // Do not accumulate deferred staging writes while minimized or while
        // acquisition is failing. Every upload below has a matching submission.
        self.upload(cells)?;
        let view = frame.as_ref().map_or_else(
            || self.offscreen.create_view(&Default::default()),
            |frame| frame.texture.create_view(&Default::default()),
        );
        let pipeline = if frame.is_some() {
            self.surface_pipeline.as_ref().unwrap()
        } else {
            &self.capture_pipeline
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("glyph frame"),
            });
        self.encode(&mut encoder, &view, pipeline);
        self.queue.submit(Some(encoder.finish()));
        if let Some(frame) = frame {
            frame.present();
        }
        Ok(())
    }

    /// Completion barrier for end-to-end wall-clock benchmarks, NOT a GPU timestamp.
    pub fn render_wait(&self) -> GpuResult<()> {
        self.device.poll(wgpu::PollType::Wait)?;
        Ok(())
    }

    /// Renders the same cells/shader to an sRGB GPU attachment and reads it back.
    /// Encoding and readback are intentionally excluded from render benchmarks.
    pub fn capture(&mut self, cells: &[GlyphCell], path: &Path) -> GpuResult<()> {
        self.upload(cells)?;
        let row_bytes = self.width * 4;
        let padded_row_bytes = row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU glyph capture readback"),
            size: u64::from(padded_row_bytes) * u64::from(self.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let view = self.offscreen.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("glyph capture"),
            });
        self.encode(&mut encoder, &view, &self.capture_pipeline);
        encoder.copy_texture_to_buffer(
            self.offscreen.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: Some(self.height),
                },
            },
            self.offscreen.size(),
        );
        self.queue.submit(Some(encoder.finish()));
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        self.render_wait()?;
        receiver.recv()??;
        let mapped = readback.slice(..).get_mapped_range();
        let mut rgba = Vec::with_capacity(row_bytes as usize * self.height as usize);
        for row in mapped.chunks_exact(padded_row_bytes as usize) {
            rgba.extend_from_slice(&row[..row_bytes as usize]);
        }
        drop(mapped);
        readback.unmap();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let mut encoder =
            png::Encoder::new(BufWriter::new(File::create(path)?), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&rgba)?;
        writer.finish()?;
        Ok(())
    }
}

fn make_offscreen(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen glyph output, not a world render input"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: CAPTURE_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn make_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("ASCII compositor; full-screen triangle"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}
