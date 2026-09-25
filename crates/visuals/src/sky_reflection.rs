//! Filters the procedural sky cubemap into the primary camera's environment
//! map so water and rigid surfaces reflect the current sky.
use bevy::{
    asset::{load_internal_asset, weak_handle, RenderAssetUsages},
    image::ImageSampler,
    prelude::*,
    render::{
        camera::Exposure,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        graph::CameraDriverLabel,
        render_asset::RenderAssets,
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{binding_types::*, *},
        renderer::{RenderContext, RenderDevice},
        texture::GpuImage,
        RenderApp,
    },
};
use bevy_atmosphere::{
    model::AtmosphereModelMetadata,
    pipeline::{AtmosphereImage, BevyAtmosphereLabel},
};
use common::structs::PrimaryCamera;

const SIZE: u32 = 256;
const MIPS: u32 = 9;
const SHADER: Handle<Shader> = weak_handle!("6f2a1c58-9d3e-4b7a-8c15-2e4f6a7b8c9d");

pub struct SkyReflectionPlugin;

impl Plugin for SkyReflectionPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SHADER, "sky_reflection.wgsl", Shader::from_wgsl);
        app.init_resource::<SkyReflectionMaps>()
            .init_resource::<SkyReflectionInput>()
            .add_plugins((
                ExtractResourcePlugin::<SkyReflectionMaps>::default(),
                ExtractResourcePlugin::<SkyReflectionInput>::default(),
            ))
            .add_systems(Update, update_world_reflections);
        let registry = app.world().resource::<AppTypeRegistry>().clone();
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.insert_resource(SkyTypeRegistry(registry));
        }
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app.init_resource::<FilterPipeline>();
        let mut graph = render_app.world_mut().resource_mut::<RenderGraph>();
        graph.add_node(SkyReflectionLabel, SkyReflectionNode::default());
        graph.add_node_edge(BevyAtmosphereLabel, SkyReflectionLabel);
        graph.add_node_edge(SkyReflectionLabel, CameraDriverLabel);
    }
}

#[derive(Resource, Clone, Default, ExtractResource)]
pub struct SkyReflectionInput {
    pub source: Option<Handle<Image>>,
    pub generation: u64,
}

#[derive(Resource, Clone, Default, ExtractResource)]
pub struct SkyReflectionMaps {
    pub specular: Handle<Image>,
    pub diffuse: Handle<Image>,
    scratch: Handle<Image>,
}

fn cube(size: u32, mips: u32) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        &[0; 8],
        TextureFormat::Rgba16Float,
        RenderAssetUsages::all(),
    );
    image.data = None;
    image.texture_descriptor.mip_level_count = mips;
    image.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image.sampler = ImageSampler::linear();
    image
}

impl SkyReflectionMaps {
    fn prepare(&mut self, images: &mut Assets<Image>) {
        if self.specular != Handle::default() {
            return;
        }
        *self = Self {
            specular: images.add(cube(SIZE, MIPS)),
            diffuse: images.add(cube(1, 1)),
            scratch: images.add(cube(SIZE, MIPS)),
        };
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_world_reflections(
    mut commands: Commands,
    sky: Option<Res<AtmosphereImage>>,
    mut maps: ResMut<SkyReflectionMaps>,
    mut images: ResMut<Assets<Image>>,
    mut input: ResMut<SkyReflectionInput>,
    mut frames: Local<u64>,
    cameras: Query<(Entity, Option<&EnvironmentMapLight>, Option<&Exposure>), With<PrimaryCamera>>,
) {
    let Some(sky) = sky else {
        return;
    };
    *frames += 1;
    maps.prepare(&mut images);
    if input.source.as_ref() != Some(&sky.handle) {
        input.source = Some(sky.handle.clone());
    }
    let generation = *frames / 30;
    if input.generation != generation {
        input.generation = generation;
    }
    for (entity, previous, exposure) in &cameras {
        let intensity = exposure.copied().unwrap_or_default().exposure().recip();
        if previous.is_some_and(|light| {
            light.specular_map == maps.specular && light.intensity == intensity
        }) {
            continue;
        }
        commands.entity(entity).insert(EnvironmentMapLight {
            diffuse_map: maps.diffuse.clone(),
            specular_map: maps.specular.clone(),
            intensity,
            ..default()
        });
    }
}

#[derive(Resource)]
struct SkyTypeRegistry(AppTypeRegistry);

#[derive(Resource)]
struct FilterPipeline {
    layout: BindGroupLayout,
    id: CachedComputePipelineId,
    sampler: Sampler,
}

impl FromWorld for FilterPipeline {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        let layout = device.create_bind_group_layout(
            "sky_reflection",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    texture_cube(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                    texture_storage_2d_array(
                        TextureFormat::Rgba16Float,
                        StorageTextureAccess::WriteOnly,
                    ),
                    uniform_buffer_sized(false, Some(FilterSettings::SIZE)),
                ),
            ),
        );
        let sampler = device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            ..default()
        });
        let id =
            world
                .resource::<PipelineCache>()
                .queue_compute_pipeline(ComputePipelineDescriptor {
                    label: Some("sky_reflection".into()),
                    layout: vec![layout.clone()],
                    push_constant_ranges: vec![],
                    shader: SHADER,
                    shader_defs: vec![],
                    entry_point: "main".into(),
                    zero_initialize_workgroup_memory: false,
                });
        Self {
            layout,
            id,
            sampler,
        }
    }
}

struct FilterSettings {
    source_mip: f32,
    radius: f32,
    flip_z: u32,
    padding: u32,
}

impl FilterSettings {
    const SIZE: std::num::NonZeroU64 = std::num::NonZeroU64::new(16).unwrap();

    fn bytes(&self) -> [u8; 16] {
        let mut bytes = [0; 16];
        bytes[..4].copy_from_slice(&self.source_mip.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.radius.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.flip_z.to_le_bytes());
        bytes[12..].copy_from_slice(&self.padding.to_le_bytes());
        bytes
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct SkyReflectionLabel;

#[derive(Default)]
struct SkyReflectionNode {
    source: Option<TextureId>,
    target: Option<TextureId>,
    generation: Option<u64>,
    warmup: u32,
    dispatch: bool,
}

impl render_graph::Node for SkyReflectionNode {
    fn update(&mut self, world: &mut World) {
        self.dispatch = false;
        let input = world.resource::<SkyReflectionInput>();
        let Some(source_handle) = &input.source else {
            *self = default();
            return;
        };
        let maps = world.resource::<SkyReflectionMaps>();
        let images = world.resource::<RenderAssets<GpuImage>>();
        let (Some(source), Some(output)) = (images.get(source_handle), images.get(&maps.specular))
        else {
            return;
        };
        let cache = world.resource::<PipelineCache>();
        if cache
            .get_compute_pipeline(world.resource::<FilterPipeline>().id)
            .is_none()
        {
            return;
        }
        if self.source != Some(source.texture.id()) || self.target != Some(output.texture.id()) {
            *self = Self {
                source: Some(source.texture.id()),
                target: Some(output.texture.id()),
                ..default()
            };
        }
        // The sky itself is drawn by a compute pass that only starts once its
        // pipeline is ready; wait for it before sampling, or the reflections
        // bake an empty cubemap.
        let types = world.resource::<SkyTypeRegistry>().0.read();
        let Some(metadata) =
            types.get_type_data::<AtmosphereModelMetadata>(std::any::TypeId::of::<
                super::nishita_cloud::NishitaCloud,
            >())
        else {
            return;
        };
        if cache.get_compute_pipeline(metadata.pipeline).is_none() {
            return;
        }
        if self.warmup < 65 {
            self.warmup += 1;
            return;
        }
        if self.generation == Some(input.generation) {
            return;
        }
        self.generation = Some(input.generation);
        self.dispatch = true;
    }

    fn run<'w>(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), render_graph::NodeRunError> {
        if !self.dispatch {
            return Ok(());
        }
        let input = world.resource::<SkyReflectionInput>();
        let maps = world.resource::<SkyReflectionMaps>();
        let images = world.resource::<RenderAssets<GpuImage>>();
        let pipeline = world.resource::<FilterPipeline>();
        let device = world.resource::<RenderDevice>();
        let (Some(source_handle), Some(compute)) = (
            &input.source,
            world
                .resource::<PipelineCache>()
                .get_compute_pipeline(pipeline.id),
        ) else {
            return Ok(());
        };
        let (Some(source), Some(scratch), Some(output)) = (
            images.get(source_handle),
            images.get(&maps.scratch),
            images.get(&maps.specular),
        ) else {
            return Ok(());
        };
        let mut dispatch =
            |src: &GpuImage, dst: &GpuImage, mip: u32, mut settings: FilterSettings| {
                let source_view = src.texture.create_view(&TextureViewDescriptor {
                    dimension: Some(TextureViewDimension::Cube),
                    base_mip_level: settings.source_mip as u32,
                    mip_level_count: Some(1),
                    ..default()
                });
                settings.source_mip = 0.0;
                let view = dst.texture.create_view(&TextureViewDescriptor {
                    dimension: Some(TextureViewDimension::D2Array),
                    base_mip_level: mip,
                    mip_level_count: Some(1),
                    ..default()
                });
                let uniform = device.create_buffer_with_data(&BufferInitDescriptor {
                    label: Some("sky_reflection_settings"),
                    contents: &settings.bytes(),
                    usage: BufferUsages::UNIFORM,
                });
                let group = device.create_bind_group(
                    "sky_reflection",
                    &pipeline.layout,
                    &BindGroupEntries::sequential((
                        &source_view,
                        &pipeline.sampler,
                        &view,
                        uniform.as_entire_binding(),
                    )),
                );
                let mut pass =
                    context
                        .command_encoder()
                        .begin_compute_pass(&ComputePassDescriptor {
                            label: Some("sky_reflection"),
                            timestamp_writes: None,
                        });
                pass.set_pipeline(compute);
                pass.set_bind_group(0, &group, &[]);
                let size = (SIZE >> mip).max(1);
                pass.dispatch_workgroups(size.div_ceil(8), size.div_ceil(8), 6);
            };
        let copy = FilterSettings {
            source_mip: 0.0,
            radius: 0.0,
            flip_z: 1,
            padding: 0,
        };
        dispatch(source, scratch, 0, copy);
        dispatch(
            source,
            output,
            0,
            FilterSettings {
                source_mip: 0.0,
                radius: 0.0,
                flip_z: 1,
                padding: 0,
            },
        );
        for mip in 1..MIPS {
            dispatch(
                scratch,
                scratch,
                mip,
                FilterSettings {
                    source_mip: (mip - 1) as f32,
                    radius: 0.0,
                    flip_z: 0,
                    padding: 0,
                },
            );
        }
        for mip in 1..MIPS {
            dispatch(
                scratch,
                output,
                mip,
                FilterSettings {
                    source_mip: (mip - 1) as f32,
                    radius: 1.6 * mip as f32 / (MIPS - 1) as f32,
                    flip_z: 0,
                    padding: 0,
                },
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_camera_gets_the_sky_environment_map_scaled_by_exposure() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_resource::<Assets<Image>>()
            .insert_resource(AtmosphereImage {
                handle: Handle::default(),
                array_view: None,
            })
            .init_resource::<SkyReflectionMaps>()
            .init_resource::<SkyReflectionInput>()
            .add_systems(Update, update_world_reflections);
        let camera = app.world_mut().spawn(PrimaryCamera::default()).id();
        let other = app
            .world_mut()
            .spawn(EnvironmentMapLight {
                intensity: 21.0,
                ..default()
            })
            .id();
        app.update();
        let light = app.world().get::<EnvironmentMapLight>(camera).unwrap();
        assert_eq!(light.intensity, Exposure::default().exposure().recip());
        assert_eq!(
            light.specular_map,
            app.world().resource::<SkyReflectionMaps>().specular
        );
        assert_eq!(
            app.world()
                .get::<EnvironmentMapLight>(other)
                .unwrap()
                .intensity,
            21.0
        );
        let exposure = Exposure { ev100: 5.0 };
        app.world_mut().entity_mut(camera).insert(exposure);
        app.update();
        assert_eq!(
            app.world()
                .get::<EnvironmentMapLight>(camera)
                .unwrap()
                .intensity,
            exposure.exposure().recip()
        );
    }
}
