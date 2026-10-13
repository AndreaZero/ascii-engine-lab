//! wgpu 26 display/capture backend: semantic cells -> font glyphs -> pixels.
//! API references: https://docs.rs/wgpu/26.0.1/wgpu/
//! https://docs.rs/wgpu-types/26.0.0/wgpu_types/enum.PollType.html
//! https://docs.rs/font8x8/0.3.1/font8x8/

use crate::core::GlyphCell;
use font8x8::UnicodeFonts;
use serde_json::{json, Value};
use std::{error::Error, fs::File, io::BufWriter, path::Path, sync::Arc};
use wgpu::util::DeviceExt;
use winit::window::Window;

type GpuResult<T> = Result<T, Box<dyn Error>>;
const CAPTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const GLYPH_NON_SAMPLER_BINDINGS: u32 = 3;
// wgpu 26 enables indirect-call validation even in release builds. Its
// persistent dispatch-validation bind group consumes one DX12 heap descriptor
// before this renderer creates its own group (wgpu-core 26.0.1
// indirect_validation/dispatch.rs, Dispatch::new). max_non_sampler_bindings
// sizes the *global* DX12 heap, not just the application's bind group.
const WGPU_INTERNAL_NON_SAMPLER_BINDINGS: u32 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackendChoice {
    #[default]
    Auto,
    Vulkan,
    Dx12,
    Metal,
    Gl,
}

impl BackendChoice {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "vulkan" => Some(Self::Vulkan),
            "dx12" => Some(Self::Dx12),
            "metal" => Some(Self::Metal),
            "gl" => Some(Self::Gl),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Vulkan => "vulkan",
            Self::Dx12 => "dx12",
            Self::Metal => "metal",
            Self::Gl => "gl",
        }
    }

    fn mask(self, auto_mask: wgpu::Backends) -> wgpu::Backends {
        match self {
            // A no-op adapter must never be mistaken for a real GPU capture.
            Self::Auto => {
                auto_mask
                    & (wgpu::Backends::VULKAN
                        | wgpu::Backends::DX12
                        | wgpu::Backends::METAL
                        | wgpu::Backends::GL
                        | wgpu::Backends::BROWSER_WEBGPU)
            }
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::Dx12 => wgpu::Backends::DX12,
            Self::Metal => wgpu::Backends::METAL,
            Self::Gl => wgpu::Backends::GL,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AdapterPower {
    #[default]
    Auto,
    Low,
    High,
}

impl AdapterPower {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "low" => Some(Self::Low),
            "high" => Some(Self::High),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Low => "low",
            Self::High => "high",
        }
    }

    fn preference(self) -> wgpu::PowerPreference {
        match self {
            Self::Auto => wgpu::PowerPreference::None,
            Self::Low => wgpu::PowerPreference::LowPower,
            Self::High => wgpu::PowerPreference::HighPerformance,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuOptions {
    pub backend: BackendChoice,
    pub power: AdapterPower,
    /// Explicit diagnostic request, not a retry after a hardware failure.
    pub fallback_adapter: bool,
}

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
    capabilities: Value,
    resize_error: Option<String>,
}

impl Gpu {
    pub async fn new(
        window: Option<Arc<Window>>,
        width: u32,
        height: u32,
        cols: u32,
        rows: u32,
    ) -> GpuResult<Self> {
        Self::new_with_options(window, width, height, cols, rows, GpuOptions::default()).await
    }

    pub async fn new_with_options(
        window: Option<Arc<Window>>,
        width: u32,
        height: u32,
        cols: u32,
        rows: u32,
        options: GpuOptions,
    ) -> GpuResult<Self> {
        let requirements = frame_requirements(width.max(1), height.max(1), cols, rows)?;
        let mut instance_options = wgpu::InstanceDescriptor::from_env_or_default();
        instance_options.backends = options.backend.mask(instance_options.backends);
        let enabled_backends = instance_options.backends;
        let compiled_backends = wgpu::Instance::enabled_backend_features();
        if (enabled_backends & compiled_backends).is_empty() {
            return Err(format!("GPU backend '{}' is unavailable in this build/target (requested {:?}, compiled {:?}); no other backend will be substituted",
                options.backend.name(), enabled_backends, compiled_backends).into());
        }
        let instance = wgpu::Instance::new(&instance_options);
        let surface = window
            .map(|window| instance.create_surface(window))
            .transpose()
            .map_err(|e| {
                format!(
                    "cannot create presentation surface for backend '{}': {e}",
                    options.backend.name()
                )
            })?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: options.power.preference(),
                compatible_surface: surface.as_ref(),
                force_fallback_adapter: options.fallback_adapter,
            })
            .await
            .map_err(|e| format!("no compatible GPU adapter: backend='{}', power='{}', fallback_adapter={}; {e}. No implicit backend/fallback retry was performed",
                options.backend.name(), options.power.name(), options.fallback_adapter))?;
        let info = adapter.get_info();
        if !enabled_backends.contains(wgpu::Backends::from(info.backend)) {
            return Err(format!(
                "adapter backend {:?} is outside the requested backend mask {:?}",
                info.backend, enabled_backends
            )
            .into());
        }
        let description = format!(
            "{} | {:?} | {:?} | driver {} {}",
            info.name, info.backend, info.device_type, info.driver, info.driver_info
        );
        let adapter_limits = adapter.limits();
        let downlevel = adapter.get_downlevel_capabilities();
        check_frame_limits(&requirements, &adapter_limits)?;
        if !downlevel
            .flags
            .contains(wgpu::DownlevelFlags::FRAGMENT_STORAGE)
        {
            return Err(format!("{description}: direct glyph rendering requires read-only storage buffers in the fragment stage (FRAGMENT_STORAGE); this adapter does not expose it. GL/WebGL support is not assumed").into());
        }
        let atlas_features = adapter.get_texture_format_features(wgpu::TextureFormat::R8Unorm);
        let capture_features = adapter.get_texture_format_features(CAPTURE_FORMAT);
        require_texture_usage(
            "ASCII font atlas R8Unorm",
            atlas_features.allowed_usages,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        )?;
        require_texture_usage(
            "glyph capture Rgba8UnormSrgb",
            capture_features.allowed_usages,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        )?;
        let limits = renderer_limits(&requirements, &adapter_limits);
        let mut failures = Vec::new();
        limits.check_limits_with_fail_fn(&adapter_limits, false, |name, wanted, available| {
            failures.push(format!("{name}: requested {wanted}, adapter {available}"));
        });
        if !failures.is_empty() {
            return Err(format!(
                "{description}: unsupported glyph renderer limits: {}",
                failures.join("; ")
            )
            .into());
        }
        let config = surface
            .as_ref()
            .map(|surface| {
                let caps = surface.get_capabilities(&adapter);
                make_surface_config(&caps, width.max(1), height.max(1))
            })
            .transpose()?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("ASCII glyph device"),
                required_features: wgpu::Features::empty(),
                required_limits: limits.clone(),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("{description}: cannot create the glyph GPU device with the reported minimal renderer requirements: {e}"))?;
        let capabilities = json!({
            "wgpu_version": "26.0.1",
            "selection": {"backend": options.backend.name(), "power": options.power.name(),
                "fallback_adapter_requested": options.fallback_adapter,
                "enabled_backend_mask": format!("{enabled_backends:?}"),
                "compiled_backend_mask": format!("{compiled_backends:?}"),
                "power_is_preference_not_guarantee": true, "implicit_retry": false},
            "adapter": {"name": info.name, "backend": format!("{:?}", info.backend),
                "device_type": format!("{:?}", info.device_type), "vendor_id": info.vendor,
                "device_id": info.device, "driver": info.driver, "driver_info": info.driver_info},
            "adapter_limits": limits_json(&adapter_limits),
            "requested_device_limits": limits_json(&limits),
            "device_limits": limits_json(&device.limits()),
            "initial_frame_requirements": {"glyph_cells": requirements.count,
                "storage_buffer_bytes": requirements.cell_bytes, "readback_buffer_bytes": requirements.readback_bytes,
                "minimum_texture_dimension_2d": requirements.texture_dimension,
                "output": [width.max(1), height.max(1)], "grid": [cols, rows]},
            "features": {"required": "empty", "device_enabled": format!("{:?}", device.features()),
                "fragment_storage": downlevel.flags.contains(wgpu::DownlevelFlags::FRAGMENT_STORAGE),
                "downlevel_flags": format!("{:?}", downlevel.flags), "shader_model": format!("{:?}", downlevel.shader_model),
                "adapter_timestamp_query_available": adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY),
                "timestamp_queries_enabled": false, "compute_used": false},
            "formats": {"atlas": "R8Unorm", "atlas_usages": format!("{:?}", atlas_features.allowed_usages),
                "capture": format!("{CAPTURE_FORMAT:?}"), "capture_usages": format!("{:?}", capture_features.allowed_usages),
                "surface": config.as_ref().map(|c| format!("{:?}", c.format))},
            "descriptor_budget": {"glyph_bind_group": GLYPH_NON_SAMPLER_BINDINGS,
                "wgpu_internal_validation_reserve": WGPU_INTERNAL_NON_SAMPLER_BINDINGS,
                "indirect_call_validation_enabled": instance_options.flags.contains(wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL)},
            "limits_policy": "Unused renderer resources request zero; one glyph storage buffer, one 16-byte uniform and one font texture. The DX12 global descriptor heap also reserves one descriptor for wgpu 26 internal indirect-call validation, which remains enabled by default. Texture2D and allocation ceilings follow the selected adapter for window resize/readback, with per-frame preflight; these are not fixed hardware minimums.",
            "validation_scope": "Capabilities are reported by this adapter/driver; they do not certify other GPUs, CPUs, architectures or backend implementations. Allocations may still fail below reported limits."
        });
        // Allocation/internal failures must be returned with adapter context,
        // not escape through wgpu's default uncaptured-error panic handler.
        device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        device.push_error_scope(wgpu::ErrorFilter::Internal);
        device.push_error_scope(wgpu::ErrorFilter::Validation);

        let cells = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("semantic glyph cells; no world image"),
            size: requirements.cell_bytes,
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
        let mut initialization_errors = Vec::new();
        for kind in ["validation", "internal", "allocation"] {
            if let Some(error) = device.pop_error_scope().await {
                initialization_errors.push(format!("{kind}: {error}"));
            }
        }
        if !initialization_errors.is_empty() {
            return Err(format!(
                "{description}: glyph GPU initialization failed (non-sampler descriptor budget {} = {} glyph + {} wgpu internal): {}",
                limits.max_non_sampler_bindings,
                GLYPH_NON_SAMPLER_BINDINGS,
                WGPU_INTERNAL_NON_SAMPLER_BINDINGS,
                initialization_errors.join("; ")
            ).into());
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
            capabilities,
            resize_error: None,
        })
    }

    pub fn adapter_description(&self) -> String {
        self.description.clone()
    }

    pub fn capabilities(&self) -> Value {
        self.capabilities.clone()
    }

    /// Keeps the semantic grid fixed; only the presentation extent changes.
    /// Zero-sized/minimized windows retain the last valid capture extent.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.resize_error = None;
        self.suspended = width == 0 || height == 0;
        if self.suspended {
            return;
        }
        let preflight = frame_requirements(width, height, self.cols, self.rows)
            .and_then(|r| check_frame_limits(&r, &self.device.limits()));
        if let Err(error) = preflight {
            self.resize_error = Some(format!(
                "cannot resize glyph output to {width}x{height}: {error}"
            ));
            self.suspended = true;
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
        if let Some(error) = &self.resize_error {
            return Err(error.clone().into());
        }
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
        if let Some(error) = &self.resize_error {
            return Err(error.clone().into());
        }
        let requirements = frame_requirements(self.width, self.height, self.cols, self.rows)?;
        check_frame_limits(&requirements, &self.device.limits())?;
        self.upload(cells)?;
        let row_bytes = requirements.row_bytes;
        let padded_row_bytes = requirements.padded_row_bytes;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("GPU glyph capture readback"),
            size: requirements.readback_bytes,
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

#[derive(Debug)]
struct FrameRequirements {
    count: u64,
    cell_bytes: u64,
    readback_bytes: u64,
    row_bytes: u32,
    padded_row_bytes: u32,
    texture_dimension: u32,
}

fn frame_requirements(
    width: u32,
    height: u32,
    cols: u32,
    rows: u32,
) -> GpuResult<FrameRequirements> {
    if width == 0 || height == 0 || cols == 0 || rows == 0 {
        return Err("output and glyph grid dimensions must be nonzero".into());
    }
    let count = u64::from(cols) * u64::from(rows);
    let cell_bytes = count
        .checked_mul(std::mem::size_of::<GlyphCell>() as u64)
        .ok_or("glyph buffer byte count overflow")?;
    if count > u64::from(u32::MAX) || cell_bytes > u64::from(u32::MAX) {
        return Err(
            "glyph grid exceeds WGSL u32 indexing or wgpu storage-binding byte range".into(),
        );
    }
    let row_bytes = width
        .checked_mul(4)
        .ok_or("RGBA row byte count exceeds u32")?;
    let alignment = u64::from(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let padded = u64::from(row_bytes).div_ceil(alignment) * alignment;
    let padded_row_bytes =
        u32::try_from(padded).map_err(|_| "aligned capture row byte count exceeds u32")?;
    let readback_bytes = padded
        .checked_mul(u64::from(height))
        .ok_or("capture byte count overflow")?;
    if readback_bytes > usize::MAX as u64 {
        return Err("capture exceeds this host's addressable buffer range".into());
    }
    Ok(FrameRequirements {
        count,
        cell_bytes,
        readback_bytes,
        row_bytes,
        padded_row_bytes,
        texture_dimension: width.max(height).max(128),
    })
}

fn check_frame_limits(r: &FrameRequirements, limits: &wgpu::Limits) -> GpuResult<()> {
    let mut errors = Vec::new();
    if r.texture_dimension > limits.max_texture_dimension_2d {
        errors.push(format!(
            "texture dimension including atlas {} > max_texture_dimension_2d {}",
            r.texture_dimension, limits.max_texture_dimension_2d
        ));
    }
    if r.cell_bytes > u64::from(limits.max_storage_buffer_binding_size) {
        errors.push(format!(
            "glyph storage {} bytes > max_storage_buffer_binding_size {}",
            r.cell_bytes, limits.max_storage_buffer_binding_size
        ));
    }
    if r.cell_bytes.max(r.readback_bytes) > limits.max_buffer_size {
        errors.push(format!(
            "glyph/capture allocation {} bytes > max_buffer_size {}",
            r.cell_bytes.max(r.readback_bytes),
            limits.max_buffer_size
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; ").into())
    }
}

/// Explicit shader/pipeline footprint, not the WebGPU defaults (which would
/// request unused compute, samplers, vertex input and multiple bind groups).
/// The two allocation/resolution ceilings follow the selected adapter so a
/// resized window can still be drawn and captured without recreating its device.
fn renderer_limits(r: &FrameRequirements, adapter: &wgpu::Limits) -> wgpu::Limits {
    wgpu::Limits {
        max_texture_dimension_1d: 1,
        max_texture_dimension_2d: adapter.max_texture_dimension_2d,
        max_texture_dimension_3d: 1,
        max_texture_array_layers: 1,
        max_bind_groups: 1,
        max_bindings_per_bind_group: 3,
        max_dynamic_uniform_buffers_per_pipeline_layout: 0,
        max_dynamic_storage_buffers_per_pipeline_layout: 0,
        max_sampled_textures_per_shader_stage: 1,
        max_samplers_per_shader_stage: 0,
        max_storage_buffers_per_shader_stage: 1,
        max_storage_textures_per_shader_stage: 0,
        max_uniform_buffers_per_shader_stage: 1,
        max_binding_array_elements_per_shader_stage: 0,
        max_binding_array_sampler_elements_per_shader_stage: 0,
        max_uniform_buffer_binding_size: 16,
        max_storage_buffer_binding_size: r.cell_bytes as u32,
        max_vertex_buffers: 0,
        max_buffer_size: adapter.max_buffer_size,
        max_vertex_attributes: 0,
        max_vertex_buffer_array_stride: 0,
        min_uniform_buffer_offset_alignment: adapter.min_uniform_buffer_offset_alignment,
        min_storage_buffer_offset_alignment: adapter.min_storage_buffer_offset_alignment,
        max_inter_stage_shader_components: 0,
        max_color_attachments: 1,
        // wgpu charges RGBA8/BGRA8 attachments 8 bytes per sample, not 4.
        max_color_attachment_bytes_per_sample: 8,
        max_compute_workgroup_storage_size: 0,
        max_compute_invocations_per_workgroup: 0,
        max_compute_workgroup_size_x: 0,
        max_compute_workgroup_size_y: 0,
        max_compute_workgroup_size_z: 0,
        max_compute_workgroups_per_dimension: 0,
        min_subgroup_size: 0,
        max_subgroup_size: 0,
        max_push_constant_size: 0,
        max_non_sampler_bindings: GLYPH_NON_SAMPLER_BINDINGS + WGPU_INTERNAL_NON_SAMPLER_BINDINGS,
        max_blas_primitive_count: 0,
        max_blas_geometry_count: 0,
        max_tlas_instance_count: 0,
        max_acceleration_structures_per_shader_stage: 0,
    }
}

fn require_texture_usage(
    label: &str,
    available: wgpu::TextureUsages,
    required: wgpu::TextureUsages,
) -> GpuResult<()> {
    if !available.contains(required) {
        return Err(format!(
            "unsupported {label}: requires {required:?}, adapter exposes {available:?}"
        )
        .into());
    }
    Ok(())
}

fn make_surface_config(
    caps: &wgpu::SurfaceCapabilities,
    width: u32,
    height: u32,
) -> GpuResult<wgpu::SurfaceConfiguration> {
    require_texture_usage(
        "presentation surface",
        caps.usages,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    )?;
    let format = caps.formats.iter().copied().find(|f| f.is_srgb()).ok_or(
        "presentation surface has no sRGB format; refusing an implicit color-transfer change",
    )?;
    let alpha_mode = caps
        .alpha_modes
        .first()
        .copied()
        .ok_or("presentation surface has no supported alpha mode")?;
    if !caps.present_modes.contains(&wgpu::PresentMode::Fifo) {
        return Err("presentation surface does not expose required FIFO present mode".into());
    }
    Ok(wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        desired_maximum_frame_latency: 2,
        alpha_mode,
        view_formats: vec![],
    })
}

fn limits_json(l: &wgpu::Limits) -> Value {
    // Repetition avoids a large recursive json! object and keeps u32/u64 exact.
    macro_rules! numeric_limits {
        ($($field:ident),+ $(,)?) => {{
            let mut values = serde_json::Map::new();
            $(values.insert(stringify!($field).to_owned(), Value::from(u64::from(l.$field)));)+
            Value::Object(values)
        }};
    }
    numeric_limits!(
        max_texture_dimension_1d,
        max_texture_dimension_2d,
        max_texture_dimension_3d,
        max_texture_array_layers,
        max_bind_groups,
        max_bindings_per_bind_group,
        max_dynamic_uniform_buffers_per_pipeline_layout,
        max_dynamic_storage_buffers_per_pipeline_layout,
        max_sampled_textures_per_shader_stage,
        max_samplers_per_shader_stage,
        max_storage_buffers_per_shader_stage,
        max_storage_textures_per_shader_stage,
        max_uniform_buffers_per_shader_stage,
        max_binding_array_elements_per_shader_stage,
        max_binding_array_sampler_elements_per_shader_stage,
        max_uniform_buffer_binding_size,
        max_storage_buffer_binding_size,
        max_vertex_buffers,
        max_buffer_size,
        max_vertex_attributes,
        max_vertex_buffer_array_stride,
        min_uniform_buffer_offset_alignment,
        min_storage_buffer_offset_alignment,
        max_inter_stage_shader_components,
        max_color_attachments,
        max_color_attachment_bytes_per_sample,
        max_compute_workgroup_storage_size,
        max_compute_invocations_per_workgroup,
        max_compute_workgroup_size_x,
        max_compute_workgroup_size_y,
        max_compute_workgroup_size_z,
        max_compute_workgroups_per_dimension,
        min_subgroup_size,
        max_subgroup_size,
        max_push_constant_size,
        max_non_sampler_bindings,
        max_blas_primitive_count,
        max_blas_geometry_count,
        max_tlas_instance_count,
        max_acceleration_structures_per_shader_stage,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_backend_is_not_overridden_by_auto_mask() {
        for (value, expected) in [
            ("vulkan", wgpu::Backends::VULKAN),
            ("dx12", wgpu::Backends::DX12),
            ("metal", wgpu::Backends::METAL),
            ("gl", wgpu::Backends::GL),
        ] {
            let choice = BackendChoice::parse(value).unwrap();
            assert_eq!(choice.name(), value);
            assert_eq!(choice.mask(wgpu::Backends::all()), expected);
            assert_eq!(choice.mask(wgpu::Backends::empty()), expected);
        }
        assert_eq!(
            BackendChoice::Auto.mask(wgpu::Backends::GL),
            wgpu::Backends::GL
        );
        assert!(!BackendChoice::Auto
            .mask(wgpu::Backends::all())
            .contains(wgpu::Backends::NOOP));
        assert!(BackendChoice::parse("typo").is_none());
    }

    #[test]
    fn power_selection_and_fallback_are_explicit() {
        let default = GpuOptions::default();
        assert_eq!(default.backend, BackendChoice::Auto);
        assert_eq!(default.power.preference(), wgpu::PowerPreference::None);
        assert!(!default.fallback_adapter);
        for (text, expected) in [
            ("auto", wgpu::PowerPreference::None),
            ("low", wgpu::PowerPreference::LowPower),
            ("high", wgpu::PowerPreference::HighPerformance),
        ] {
            let power = AdapterPower::parse(text).unwrap();
            assert_eq!(power.name(), text);
            assert_eq!(power.preference(), expected);
        }
        assert!(AdapterPower::parse("fastest").is_none());
    }

    #[test]
    fn capture_padding_atlas_and_storage_have_distinct_requirements() {
        let r = frame_requirements(65, 3, 4, 3).unwrap();
        assert_eq!(r.count, 12);
        assert_eq!(r.cell_bytes, 384);
        assert_eq!(r.row_bytes, 260);
        assert_eq!(r.padded_row_bytes, 512);
        assert_eq!(r.readback_bytes, 1536);
        assert_eq!(r.texture_dimension, 128); // Atlas is wider than this output.
        assert!(frame_requirements(0, 1, 1, 1).is_err());
        assert!(frame_requirements(1, 1, 0, 1).is_err());
        assert!(frame_requirements(u32::MAX, 1, 1, 1).is_err());
        assert!(frame_requirements(1, 1, u32::MAX, u32::MAX).is_err());
    }

    #[test]
    fn renderer_does_not_require_compute_or_default_webgpu_resource_counts() {
        let r = frame_requirements(1280, 720, 160, 90).unwrap();
        let adapter = wgpu::Limits::default();
        let limits = renderer_limits(&r, &adapter);
        assert!(limits.check_limits(&adapter));
        assert_eq!(limits.max_storage_buffers_per_shader_stage, 1);
        assert_eq!(limits.max_storage_buffer_binding_size, 160 * 90 * 32);
        assert_eq!(limits.max_uniform_buffer_binding_size, 16);
        assert_eq!(limits.max_bind_groups, 1);
        assert_eq!(limits.max_sampled_textures_per_shader_stage, 1);
        assert_eq!(limits.max_samplers_per_shader_stage, 0);
        assert_eq!(limits.max_compute_invocations_per_workgroup, 0);
        assert_eq!(limits.max_vertex_buffers, 0);
        assert_eq!(limits.max_color_attachment_bytes_per_sample, 8);
        // Three app descriptors alone exhaust the DX12 heap: wgpu's
        // release-default indirect-call validator already occupies one slot.
        assert_eq!(GLYPH_NON_SAMPLER_BINDINGS, 3);
        assert_eq!(WGPU_INTERNAL_NON_SAMPLER_BINDINGS, 1);
        assert_eq!(limits.max_non_sampler_bindings, 4);
        assert_eq!(
            limits.max_texture_dimension_2d,
            adapter.max_texture_dimension_2d
        );
        let unavailable = wgpu::Limits {
            max_storage_buffers_per_shader_stage: 0,
            ..adapter.clone()
        };
        assert!(!limits.check_limits(&unavailable));
    }

    #[test]
    fn unsupported_dimensions_storage_and_readback_fail_preflight() {
        let r = frame_requirements(1280, 720, 160, 90).unwrap();
        let mut limits = wgpu::Limits::default();
        assert!(check_frame_limits(&r, &limits).is_ok());
        limits.max_buffer_size = r.readback_bytes - 1;
        assert!(check_frame_limits(&r, &limits)
            .unwrap_err()
            .to_string()
            .contains("max_buffer_size"));
        limits = wgpu::Limits::default();
        limits.max_storage_buffer_binding_size = r.cell_bytes as u32 - 1;
        assert!(check_frame_limits(&r, &limits)
            .unwrap_err()
            .to_string()
            .contains("storage"));
        limits = wgpu::Limits::default();
        limits.max_texture_dimension_2d = 1024;
        assert!(check_frame_limits(&r, &limits)
            .unwrap_err()
            .to_string()
            .contains("texture"));
    }

    #[test]
    fn unavailable_surface_and_texture_usages_return_errors() {
        let required = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC;
        assert!(require_texture_usage("capture", required, required).is_ok());
        assert!(
            require_texture_usage("capture", wgpu::TextureUsages::RENDER_ATTACHMENT, required)
                .is_err()
        );
        let mut caps = wgpu::SurfaceCapabilities {
            formats: vec![wgpu::TextureFormat::Bgra8UnormSrgb],
            present_modes: vec![wgpu::PresentMode::Fifo],
            alpha_modes: vec![wgpu::CompositeAlphaMode::Opaque],
            usages: wgpu::TextureUsages::RENDER_ATTACHMENT,
        };
        assert_eq!(
            make_surface_config(&caps, 640, 480).unwrap().format,
            wgpu::TextureFormat::Bgra8UnormSrgb
        );
        caps.formats.clear();
        assert!(make_surface_config(&caps, 640, 480).is_err());
        caps.formats.push(wgpu::TextureFormat::Bgra8Unorm);
        assert!(make_surface_config(&caps, 640, 480).is_err());
    }
}
