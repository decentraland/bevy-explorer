//! Reflection / irradiance environment maps generated from the atmosphere
//! skybox, attached as an `EnvironmentMapLight` to every camera that renders
//! the sky. Regenerated whenever the sky is redrawn.
//!
//! Only the upper hemisphere of the maps is generated. The pbr shaders' lookups
//! (`sky_envmap_lookup.wgsl`) mirror downward directions up, and level the result
//! towards the `AmbientLight`, which they use in place of the flat ambient.
//!
//! At the low sky reflections setting the maps are one texel per face instead:
//! the sky, the horizon and the ground as three colours, straight from the sky.

use bevy::{
    asset::{load_internal_asset, weak_handle},
    pbr::environment_map::EnvironmentMapLight,
    prelude::*,
    render::{
        camera::Exposure,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        graph::CameraDriverLabel,
        render_asset::{RenderAssetUsages, RenderAssets},
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{
            binding_types::{
                sampler, texture_2d_array, texture_cube, texture_storage_2d_array, uniform_buffer,
            },
            BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries,
            CachedComputePipelineId, ComputePassDescriptor, ComputePipelineDescriptor,
            DynamicUniformBuffer, Extent3d, FilterMode, PipelineCache, Sampler, SamplerBindingType,
            SamplerDescriptor, ShaderStages, StorageTextureAccess, Texture, TextureAspect,
            TextureDescriptor, TextureDimension, TextureFormat, TextureId, TextureSampleType,
            TextureUsages, TextureView, TextureViewDescriptor, TextureViewDimension,
        },
        renderer::{RenderDevice, RenderQueue},
        texture::GpuImage,
        Render, RenderApp, RenderSet,
    },
};
use bevy_atmosphere::{
    pipeline::{AtmosphereImage, AtmosphereUpdateEvent, BevyAtmosphereLabel},
    prelude::AtmosphereCamera,
};
use bevy_console::ConsoleCommand;
use common::structs::{AppConfig, SkyReflectionSetting};
use console::DoAddConsoleCommand;

const SKY_ENVMAP_SHADER_HANDLE: Handle<Shader> =
    weak_handle!("6b2f4c7e-3a1d-4f8e-9c2b-7d5e1a0f3b64");

const SPECULAR_SIZE: u32 = 128;
const SPECULAR_MIPS: u32 = 8;
const SPECULAR_SAMPLES: u32 = 32;
const DIFFUSE_SIZE: u32 = 32;
const DIFFUSE_SAMPLES: u32 = 128;
// the low setting: one texel per face, straight from the (unfiltered) sky
const TRILIGHT_SAMPLES: u32 = 256;
// mip of the downsampled sky the irradiance pass integrates over
const DIFFUSE_SOURCE_LOD: f32 = 2.0;
const WORKGROUP_SIZE: u32 = 8;
// the filter passes generate every face but -y (see sky_envmap.wgsl)
const GENERATED_FACES: u32 = 5;

pub struct SkyEnvmapPlugin;

/// The generated cubemaps, in the skybox's own units: the filtered maps, and the one-texel maps
/// of the low setting.
#[derive(Resource, Clone, ExtractResource)]
pub struct SkyEnvmap {
    pub diffuse: Handle<Image>,
    pub specular: Handle<Image>,
    pub trilight_diffuse: Handle<Image>,
    pub trilight_specular: Handle<Image>,
}

impl SkyEnvmap {
    fn maps(&self, setting: SkyReflectionSetting) -> (&Handle<Image>, &Handle<Image>) {
        match setting {
            SkyReflectionSetting::Low => (&self.trilight_diffuse, &self.trilight_specular),
            SkyReflectionSetting::High => (&self.diffuse, &self.specular),
        }
    }
}

#[derive(Resource, Clone, Copy, ExtractResource)]
pub struct SkyEnvmapSettings {
    /// scale on the irradiance (diffuse) map, applied when it is generated
    pub diffuse: f32,
    /// scale on the reflection (specular) map, applied when it is generated
    pub specular: f32,
    /// scale on the floor the lookups level the envmap towards, which comes from the ambient
    /// brightness setting. carried to the pbr shaders as the `AmbientLight`
    pub floor: f32,
    /// how far envmap light above the floor is compressed towards it, in log space (0 = not at
    /// all, 1 = down to the floor). carried with the floor
    pub compression: f32,
    /// which maps to generate, from the config
    pub quality: SkyReflectionSetting,
}

impl Default for SkyEnvmapSettings {
    fn default() -> Self {
        Self {
            diffuse: 1.0,
            specular: 1.0,
            floor: 0.5,
            compression: 0.8,
            quality: SkyReflectionSetting::High,
        }
    }
}

impl Plugin for SkyEnvmapPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            SKY_ENVMAP_SHADER_HANDLE,
            "sky_envmap.wgsl",
            Shader::from_wgsl
        );

        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        let specular = images.add(cube_image(
            "sky specular envmap",
            SPECULAR_SIZE,
            SPECULAR_MIPS,
        ));
        let diffuse = images.add(cube_image("sky diffuse envmap", DIFFUSE_SIZE, 1));
        let trilight_diffuse = images.add(cube_image("sky trilight diffuse envmap", 1, 1));
        let trilight_specular = images.add(cube_image("sky trilight specular envmap", 1, 1));

        app.insert_resource(SkyEnvmap {
            diffuse,
            specular,
            trilight_diffuse,
            trilight_specular,
        })
        .init_resource::<SkyEnvmapSettings>()
        .add_plugins((
            ExtractResourcePlugin::<SkyEnvmap>::default(),
            ExtractResourcePlugin::<SkyEnvmapSettings>::default(),
        ))
        .add_systems(Update, attach_sky_envmap);

        app.add_console_command::<EnvmapConsoleCommand, _>(envmap_console_command);
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .init_resource::<SkyEnvmapPipelines>()
            .add_systems(
                Render,
                prepare_sky_envmap.in_set(RenderSet::PrepareResources),
            );

        let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
        render_graph.add_node(SkyEnvmapLabel, SkyEnvmapNode);
        render_graph.add_node_edge(BevyAtmosphereLabel, SkyEnvmapLabel);
        render_graph.add_node_edge(SkyEnvmapLabel, CameraDriverLabel);
    }
}

fn cube_image(label: &'static str, size: u32, mips: u32) -> Image {
    let mut image = Image::new_uninit(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.label = Some(label);
    image.texture_descriptor.mip_level_count = mips;
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        label: Some(label),
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image
}

// the skybox material draws the cube without the view exposure, while the pbr
// shader applies it to environment light. cancel it so a mirror reflects the
// sky exactly as drawn.
#[allow(clippy::type_complexity)]
fn attach_sky_envmap(
    mut commands: Commands,
    envmap: Res<SkyEnvmap>,
    config: Res<AppConfig>,
    mut settings: ResMut<SkyEnvmapSettings>,
    mut cameras: Query<
        (Entity, Option<&Exposure>, Option<&mut EnvironmentMapLight>),
        With<AtmosphereCamera>,
    >,
) {
    let quality = config.graphics.sky_reflections;
    if settings.quality != quality {
        settings.quality = quality;
    }
    let (diffuse, specular) = envmap.maps(quality);
    for (camera, exposure, light) in cameras.iter_mut() {
        let intensity = 1.0 / exposure.copied().unwrap_or_default().exposure();
        match light {
            Some(mut light) => {
                if light.intensity != intensity {
                    light.intensity = intensity;
                }
                if light.diffuse_map != *diffuse {
                    light.diffuse_map = diffuse.clone();
                    light.specular_map = specular.clone();
                }
            }
            None => {
                commands.entity(camera).try_insert(EnvironmentMapLight {
                    diffuse_map: diffuse.clone(),
                    specular_map: specular.clone(),
                    intensity,
                    ..default()
                });
            }
        }
    }
}

/// scale the sky reflection map, the sky irradiance map, and their floor, and set the compression
/// above the floor
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/envmap")]
struct EnvmapConsoleCommand {
    specular: Option<f32>,
    diffuse: Option<f32>,
    floor: Option<f32>,
    compression: Option<f32>,
}

fn envmap_console_command(
    mut input: ConsoleCommand<EnvmapConsoleCommand>,
    mut settings: ResMut<SkyEnvmapSettings>,
) {
    if let Some(Ok(command)) = input.take() {
        if let Some(specular) = command.specular {
            settings.specular = specular;
        }
        if let Some(diffuse) = command.diffuse {
            settings.diffuse = diffuse;
        }
        if let Some(floor) = command.floor {
            settings.floor = floor;
        }
        if let Some(compression) = command.compression {
            settings.compression = compression.clamp(0.0, 1.0);
        }
        input.reply_ok(format!(
            "envmap specular {} diffuse {} floor {} compression {}",
            settings.specular, settings.diffuse, settings.floor, settings.compression
        ));
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct SkyEnvmapLabel;

mod decl {
    // temporary for ShaderType macro, remove in future
    #![allow(dead_code)]

    use bevy::render::render_resource::ShaderType;

    #[derive(ShaderType, Clone, Copy)]
    pub struct Constants {
        pub intensity: f32,
        pub roughness: f32,
        pub sample_count: u32,
        pub source_lod: f32,
    }
}
use decl::Constants;

#[derive(Resource)]
struct SkyEnvmapPipelines {
    downsample_layout: BindGroupLayout,
    filter_layout: BindGroupLayout,
    sampler: Sampler,
    downsample_sky: CachedComputePipelineId,
    downsample: CachedComputePipelineId,
    specular: CachedComputePipelineId,
    irradiance: CachedComputePipelineId,
    trilight: CachedComputePipelineId,
}

impl FromWorld for SkyEnvmapPipelines {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();

        let downsample_layout = device.create_bind_group_layout(
            "sky envmap downsample layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    texture_2d_array(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    texture_storage_2d_array(
                        TextureFormat::Rgba16Float,
                        StorageTextureAccess::WriteOnly,
                    ),
                ),
            ),
        );
        let filter_layout = device.create_bind_group_layout(
            "sky envmap filter layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    texture_cube(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    texture_storage_2d_array(
                        TextureFormat::Rgba16Float,
                        StorageTextureAccess::WriteOnly,
                    ),
                    uniform_buffer::<Constants>(true),
                ),
            ),
        );
        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("sky envmap sampler"),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            ..default()
        });

        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = |label: &'static str,
                        layout: &BindGroupLayout,
                        entry: &'static str,
                        shader_defs: &[&str]| {
            pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some(label.into()),
                layout: vec![layout.clone()],
                push_constant_ranges: Vec::new(),
                shader: SKY_ENVMAP_SHADER_HANDLE,
                shader_defs: shader_defs.iter().map(|def| (*def).into()).collect(),
                entry_point: entry.into(),
                zero_initialize_workgroup_memory: false,
            })
        };

        Self {
            downsample_sky: pipeline(
                "sky envmap downsample sky",
                &filter_layout,
                "downsample_sky",
                &[],
            ),
            downsample: pipeline(
                "sky envmap downsample",
                &downsample_layout,
                "downsample",
                &["DOWNSAMPLE"],
            ),
            specular: pipeline("sky envmap specular", &filter_layout, "specular", &[]),
            irradiance: pipeline("sky envmap irradiance", &filter_layout, "irradiance", &[]),
            // the irradiance pass again, reading the sky itself
            trilight: pipeline(
                "sky envmap trilight",
                &filter_layout,
                "irradiance",
                &["RAW_SKY"],
            ),
            downsample_layout,
            filter_layout,
            sampler,
        }
    }
}

/// Bind groups for one generation: box-downsample the sky into a mipped
/// intermediate cube, then filter each output mip from it. Rebuilt when the
/// sky or output textures are recreated; the uniforms are rewritten in place
/// when the scales change.
#[derive(Resource)]
struct SkyEnvmapResources {
    /// the sky and output textures the bind groups were built for
    textures: [TextureId; 5],
    downsample_sky: (BindGroup, u32),
    downsample: Vec<BindGroup>,
    specular: Vec<(BindGroup, u32)>,
    irradiance: (BindGroup, u32),
    /// the low setting's diffuse and specular maps
    trilight: [(BindGroup, u32); 2],
    _intermediate: Texture,
    uniforms: DynamicUniformBuffer<Constants>,
}

/// Pushes the constants for every filter pass: one per specular mip, the irradiance pass, then
/// the two trilight passes. The order (and so the offsets) is the same every time, so rewriting
/// keeps bind groups valid.
fn push_constants(
    uniforms: &mut DynamicUniformBuffer<Constants>,
    settings: &SkyEnvmapSettings,
) -> (Vec<u32>, u32, [u32; 2]) {
    uniforms.clear();
    let specular = (0..SPECULAR_MIPS)
        .map(|mip| {
            uniforms.push(&Constants {
                intensity: settings.specular,
                roughness: mip as f32 / (SPECULAR_MIPS - 1) as f32,
                sample_count: SPECULAR_SAMPLES,
                source_lod: 0.0,
            })
        })
        .collect();
    let irradiance = uniforms.push(&Constants {
        intensity: settings.diffuse,
        roughness: 0.0,
        sample_count: DIFFUSE_SAMPLES,
        source_lod: DIFFUSE_SOURCE_LOD,
    });
    let trilight = [settings.diffuse, settings.specular].map(|intensity| {
        uniforms.push(&Constants {
            intensity,
            roughness: 0.0,
            sample_count: TRILIGHT_SAMPLES,
            source_lod: 0.0,
        })
    });
    (specular, irradiance, trilight)
}

fn array_view(texture: &Texture, mip: u32) -> TextureView {
    texture.create_view(&TextureViewDescriptor {
        label: Some("sky envmap mip"),
        dimension: Some(TextureViewDimension::D2Array),
        aspect: TextureAspect::All,
        base_mip_level: mip,
        mip_level_count: Some(1),
        base_array_layer: 0,
        array_layer_count: Some(6),
        ..default()
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_sky_envmap(
    mut commands: Commands,
    existing: Option<ResMut<SkyEnvmapResources>>,
    pipelines: Res<SkyEnvmapPipelines>,
    settings: Res<SkyEnvmapSettings>,
    atmosphere_image: Res<AtmosphereImage>,
    envmap: Res<SkyEnvmap>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let (
        Some(source),
        Some(specular),
        Some(diffuse),
        Some(trilight_diffuse),
        Some(trilight_specular),
    ) = (
        gpu_images.get(&atmosphere_image.handle),
        gpu_images.get(&envmap.specular),
        gpu_images.get(&envmap.diffuse),
        gpu_images.get(&envmap.trilight_diffuse),
        gpu_images.get(&envmap.trilight_specular),
    )
    else {
        return;
    };
    let textures = [
        source,
        specular,
        diffuse,
        trilight_diffuse,
        trilight_specular,
    ]
    .map(|image| image.texture.id());
    if let Some(mut existing) = existing {
        if existing.textures == textures {
            if settings.is_changed() {
                push_constants(&mut existing.uniforms, &settings);
                existing.uniforms.write_buffer(&device, &queue);
            }
            return;
        }
    }

    let intermediate = device.create_texture(&TextureDescriptor {
        label: Some("sky envmap intermediate"),
        size: Extent3d {
            width: SPECULAR_SIZE,
            height: SPECULAR_SIZE,
            depth_or_array_layers: 6,
        },
        mip_level_count: SPECULAR_MIPS,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba16Float,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING,
        view_formats: &[],
    });
    let intermediate_cube = intermediate.create_view(&TextureViewDescriptor {
        label: Some("sky envmap intermediate cube"),
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });

    let mut uniforms = DynamicUniformBuffer::<Constants>::default();
    let (specular_offsets, irradiance_offset, trilight_offsets) =
        push_constants(&mut uniforms, &settings);
    uniforms.write_buffer(&device, &queue);
    let uniform_binding = uniforms.binding().unwrap();

    // mip 0 comes from the sky cube itself (with the lower half mirrored), the rest from the
    // mip above
    let downsample = (1..SPECULAR_MIPS)
        .map(|mip| {
            device.create_bind_group(
                "sky envmap downsample",
                &pipelines.downsample_layout,
                &BindGroupEntries::sequential((
                    &array_view(&intermediate, mip - 1),
                    &pipelines.sampler,
                    &array_view(&intermediate, mip),
                )),
            )
        })
        .collect();

    let filter_bind_group = |input: &TextureView, output: &TextureView, offset: u32| {
        (
            device.create_bind_group(
                "sky envmap filter",
                &pipelines.filter_layout,
                &BindGroupEntries::sequential((
                    input,
                    &pipelines.sampler,
                    output,
                    uniform_binding.clone(),
                )),
            ),
            offset,
        )
    };
    // the first downsample reads the sky as a cube, so it shares the filter layout; it ignores
    // the constants
    let downsample_sky = filter_bind_group(
        &source.texture_view,
        &array_view(&intermediate, 0),
        irradiance_offset,
    );
    let specular = (0..SPECULAR_MIPS)
        .map(|mip| {
            filter_bind_group(
                &intermediate_cube,
                &array_view(&specular.texture, mip),
                specular_offsets[mip as usize],
            )
        })
        .collect();
    let irradiance = filter_bind_group(
        &intermediate_cube,
        &array_view(&diffuse.texture, 0),
        irradiance_offset,
    );
    let trilight = [
        filter_bind_group(
            &source.texture_view,
            &array_view(&trilight_diffuse.texture, 0),
            trilight_offsets[0],
        ),
        filter_bind_group(
            &source.texture_view,
            &array_view(&trilight_specular.texture, 0),
            trilight_offsets[1],
        ),
    ];

    commands.insert_resource(SkyEnvmapResources {
        textures,
        downsample_sky,
        downsample,
        specular,
        irradiance,
        trilight,
        _intermediate: intermediate,
        uniforms,
    });
}

struct SkyEnvmapNode;

impl render_graph::Node for SkyEnvmapNode {
    fn run<'w>(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut bevy::render::renderer::RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), render_graph::NodeRunError> {
        // the sky only redraws when the atmosphere changes; follow it
        if world.resource::<Events<AtmosphereUpdateEvent>>().is_empty() {
            return Ok(());
        }
        let Some(resources) = world.get_resource::<SkyEnvmapResources>() else {
            return Ok(());
        };
        let pipelines = world.resource::<SkyEnvmapPipelines>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let quality = world.resource::<SkyEnvmapSettings>().quality;
        let (
            Some(downsample_sky),
            Some(downsample),
            Some(specular),
            Some(irradiance),
            Some(trilight),
        ) = (
            pipeline_cache.get_compute_pipeline(pipelines.downsample_sky),
            pipeline_cache.get_compute_pipeline(pipelines.downsample),
            pipeline_cache.get_compute_pipeline(pipelines.specular),
            pipeline_cache.get_compute_pipeline(pipelines.irradiance),
            pipeline_cache.get_compute_pipeline(pipelines.trilight),
        )
        else {
            return Ok(());
        };

        let mut pass =
            render_context
                .command_encoder()
                .begin_compute_pass(&ComputePassDescriptor {
                    label: Some("sky envmap"),
                    timestamp_writes: None,
                });
        let groups = |size: u32| size.div_ceil(WORKGROUP_SIZE);

        if quality == SkyReflectionSetting::Low {
            pass.set_pipeline(trilight);
            for (bind_group, offset) in &resources.trilight {
                pass.set_bind_group(0, bind_group, &[*offset]);
                pass.dispatch_workgroups(1, 1, GENERATED_FACES);
            }
            return Ok(());
        }

        pass.set_pipeline(downsample_sky);
        let (bind_group, offset) = &resources.downsample_sky;
        pass.set_bind_group(0, bind_group, &[*offset]);
        pass.dispatch_workgroups(groups(SPECULAR_SIZE), groups(SPECULAR_SIZE), 6);

        pass.set_pipeline(downsample);
        for (mip, bind_group) in resources.downsample.iter().enumerate() {
            let size = groups(SPECULAR_SIZE >> (mip + 1));
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups(size, size, 6);
        }

        pass.set_pipeline(specular);
        for (mip, (bind_group, offset)) in resources.specular.iter().enumerate() {
            let size = groups(SPECULAR_SIZE >> mip);
            pass.set_bind_group(0, bind_group, &[*offset]);
            pass.dispatch_workgroups(size, size, GENERATED_FACES);
        }

        pass.set_pipeline(irradiance);
        let (bind_group, offset) = &resources.irradiance;
        pass.set_bind_group(0, bind_group, &[*offset]);
        pass.dispatch_workgroups(groups(DIFFUSE_SIZE), groups(DIFFUSE_SIZE), GENERATED_FACES);

        Ok(())
    }
}
