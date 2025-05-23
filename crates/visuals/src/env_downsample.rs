use bevy::{
    asset::load_internal_asset,
    core_pipeline::fullscreen_vertex_shader::fullscreen_shader_vertex_state,
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        graph::CameraDriverLabel,
        render_asset::{RenderAssetUsages, RenderAssets},
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{
            binding_types::{sampler, texture_2d},
            BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, BlendState,
            CachedPipelineState, CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d,
            FragmentState, MultisampleState, Operations, PipelineCache, PrimitiveState,
            RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler,
            SamplerDescriptor, ShaderStages, TextureAspect, TextureDescriptor, TextureDimension,
            TextureFormat, TextureUsages, TextureView, TextureViewDescriptor, TextureViewDimension,
        },
        renderer::RenderDevice,
        texture::GpuImage,
        Render, RenderApp, RenderSet,
    },
};
use bevy_atmosphere::pipeline::{AtmosphereImage, BevyAtmosphereLabel};

pub const ENV_DOWNSAMPLE_SHADER_HANDLE: Handle<Shader> =
    Handle::weak_from_u128(1245571048899995008);

pub struct EnvmapDownsamplePlugin;

#[derive(Resource, ExtractResource, Clone)]
pub struct Envmap(pub Handle<Image>);

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
pub struct EnvmapLabel;

#[derive(Resource)]
pub struct EnvmapDownsampleData {
    pub source_view: Vec<TextureView>,
    pub target_view: Vec<TextureView>,
    pub bind_group_layout: BindGroupLayout,
    pub bindgroups: Vec<BindGroup>,
    pub pipeline: CachedRenderPipelineId,
    pub sampler: Sampler,
}

impl FromWorld for EnvmapDownsampleData {
    fn from_world(render_world: &mut World) -> Self {
        let render_device = render_world.resource::<RenderDevice>();

        let bind_group_layout = render_device.create_bind_group_layout(
            "envmap downsample bindings",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::all(),
                (
                    texture_2d(bevy::render::render_resource::TextureSampleType::Float {
                        filterable: true,
                    }),
                    sampler(bevy::render::render_resource::SamplerBindingType::Filtering),
                ),
            ),
        );
        let sampler = render_device.create_sampler(&SamplerDescriptor::default());

        let pipeline_cache = render_world.resource::<PipelineCache>();

        let pipeline = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
            label: Some("env downsample pipeline".into()),
            layout: vec![bind_group_layout.clone()],
            vertex: fullscreen_shader_vertex_state(),
            fragment: Some(FragmentState {
                shader: ENV_DOWNSAMPLE_SHADER_HANDLE,
                shader_defs: vec![],
                entry_point: "downsample".into(),
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::Rgba16Float,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            push_constant_ranges: Vec::new(),
        });

        Self {
            source_view: Vec::default(),
            target_view: Vec::default(),
            sampler,
            bind_group_layout,
            bindgroups: Vec::default(),
            pipeline,
        }
    }
}

impl Plugin for EnvmapDownsamplePlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            ENV_DOWNSAMPLE_SHADER_HANDLE,
            "env_downsample.wgsl",
            Shader::from_wgsl
        );

        let mut image = Image::new_fill(
            Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 6,
            },
            TextureDimension::D2,
            &[0; 4 * 4],
            TextureFormat::Rgba16Float,
            RenderAssetUsages::default(),
        );

        image.texture_view_descriptor = Some(TextureViewDescriptor {
            label: Some("downsampled envmap view"),
            format: Some(TextureFormat::Rgba16Float),
            dimension: Some(TextureViewDimension::Cube),
            aspect: TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: None,
            base_array_layer: 0,
            array_layer_count: Some(6),
        });
        image.texture_descriptor = TextureDescriptor {
            label: Some("downsampled envmap"),
            size: Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba16Float,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[TextureFormat::Rgba16Float],
        };
        let mut image_assets = app.world_mut().resource_mut::<Assets<Image>>();
        let handle = image_assets.add(image);
        app.insert_resource(Envmap(handle));
        app.add_plugins(ExtractResourcePlugin::<Envmap>::default());
    }

    fn finish(&self, app: &mut App) {
        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .init_resource::<EnvmapDownsampleData>()
            .add_systems(
                Render,
                prepare_envmap_resources.in_set(RenderSet::PrepareResources),
            );

        let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
        render_graph.add_node(EnvmapLabel, EnvmapNode::default());
        render_graph.add_node_edge(BevyAtmosphereLabel, EnvmapLabel);
        render_graph.add_node_edge(EnvmapLabel, CameraDriverLabel);
    }
}

pub struct CubemapDownsamplePipeline {
    // bindgroup
}

fn prepare_envmap_resources(
    mut data: ResMut<EnvmapDownsampleData>,
    atmosphere_image: Res<AtmosphereImage>,
    envmap_image: Res<Envmap>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    device: Res<RenderDevice>,
) {
    if data.source_view.is_empty() {
        let Some(target_texture) = gpu_images.get(&envmap_image.0).map(|h| &h.texture) else {
            return;
        };
        let Some(source_texture) = gpu_images.get(&atmosphere_image.handle).map(|h| &h.texture)
        else {
            return;
        };
        for i in 0..6 {
            let source_view = source_texture.create_view(&TextureViewDescriptor {
                label: Some("source cube"),
                format: Some(TextureFormat::Rgba16Float),
                dimension: Some(TextureViewDimension::D2),
                aspect: TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: i,
                array_layer_count: Some(1),
            });

            let target_view = target_texture.create_view(&TextureViewDescriptor {
                label: Some("target cube"),
                format: Some(TextureFormat::Rgba16Float),
                dimension: Some(TextureViewDimension::D2),
                aspect: TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: i,
                array_layer_count: Some(1),
            });

            let bindgroup = device.create_bind_group(
                None,
                &data.bind_group_layout,
                &BindGroupEntries::sequential((&source_view, &data.sampler)),
            );

            data.source_view.push(source_view);
            data.target_view.push(target_view);
            data.bindgroups.push(bindgroup);
        }
    }
}

#[derive(Default)]
pub struct EnvmapNode {
    loading: bool,
}

impl render_graph::Node for EnvmapNode {
    fn update(&mut self, world: &mut World) {
        if self.loading {
            let pipeline = world.resource::<EnvmapDownsampleData>().pipeline;

            let pipeline_cache = world.resource::<PipelineCache>();
            if let CachedPipelineState::Ok(_) = pipeline_cache.get_render_pipeline_state(pipeline) {
                self.loading = false;
            }
        }
    }
    fn run<'w>(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut bevy::render::renderer::RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), render_graph::NodeRunError> {
        let pipeline_cache = world.get_resource::<PipelineCache>().unwrap();
        let data = world.get_resource::<EnvmapDownsampleData>().unwrap();

        for i in 0..6 {
            let target = &data.target_view[i];

            let bind_group = &data.bindgroups[i];

            let Some(pipeline) = pipeline_cache.get_render_pipeline(data.pipeline) else {
                return Ok(());
            };

            let pass_descriptor = RenderPassDescriptor {
                label: Some("env downsample pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: Operations::default(),
                })],
                ..Default::default()
            };

            let mut render_pass = render_context
                .command_encoder()
                .begin_render_pass(&pass_descriptor);

            render_pass.set_pipeline(pipeline);
            render_pass.set_bind_group(0, bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }

        Ok(())
    }
}
