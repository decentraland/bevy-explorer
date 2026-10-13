//! Stars drawn over the finished frame after depth of field, so they stay pin sharp at screen
//! resolution instead of being baked into the sky cubemap and blurred with it. See `stars.wgsl`.

use bevy::{
    core_pipeline::{
        core_3d::graph::{Core3d, Node3d},
        dof::TransparentFocusTexture,
        fullscreen_vertex_shader::fullscreen_shader_vertex_state,
    },
    ecs::{
        query::{Has, QueryItem},
        system::lifetimeless::Read,
    },
    image::BevyDefault as _,
    platform::collections::HashMap,
    prelude::*,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        render_asset::RenderAssets,
        render_graph::{
            NodeRunError, RenderGraphApp, RenderGraphContext, RenderLabel, ViewNode, ViewNodeRunner,
        },
        render_resource::{
            binding_types::{
                sampler, texture_2d, texture_2d_multisampled, texture_cube, texture_depth_2d,
                texture_depth_2d_multisampled, uniform_buffer,
            },
            BindGroupEntries, BindGroupLayout, BlendComponent, BlendFactor, BlendOperation,
            BlendState, CachedRenderPipelineId, ColorTargetState, ColorWrites, FragmentState,
            LoadOp, MultisampleState, Operations, PipelineCache, PrimitiveState,
            RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor,
            SamplerBindingType, ShaderStages, ShaderType, SpecializedRenderPipeline,
            SpecializedRenderPipelines, StoreOp, TextureFormat, TextureSampleType, TextureUsages,
            UniformBuffer,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        texture::GpuImage,
        view::{
            ExtractedView, Msaa, ViewDepthTexture, ViewTarget, ViewUniform, ViewUniformOffset,
            ViewUniforms,
        },
        Render, RenderApp, RenderSet,
    },
};
use bevy_atmosphere::{pipeline::AtmosphereImage, prelude::AtmosphereCamera};
use common::structs::SceneGlobalLight;

pub struct StarsPlugin;

impl Plugin for StarsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StarsSettings>()
            .add_plugins((
                ExtractResourcePlugin::<StarsSettings>::default(),
                ExtractComponentPlugin::<StarsView>::default(),
            ))
            .add_systems(Update, (update_stars_settings, bind_depth_texture));

        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .init_resource::<SpecializedRenderPipelines<StarsPipeline>>()
            .init_resource::<StarsUniformBuffer>()
            .add_systems(
                Render,
                (
                    prepare_stars_pipelines.in_set(RenderSet::Prepare),
                    prepare_stars_uniform.in_set(RenderSet::PrepareResources),
                ),
            )
            .add_render_graph_node::<ViewNodeRunner<StarsNode>>(Core3d, StarsLabel)
            .add_render_graph_edges(
                Core3d,
                (Node3d::DepthOfField, StarsLabel, Node3d::Tonemapping),
            );
    }

    fn finish(&self, app: &mut App) {
        app.sub_app_mut(RenderApp).init_resource::<StarsPipeline>();
    }
}

pub use settings::StarsSettings;

mod settings {
    // the ShaderType derive's layout check function is never called
    #![allow(dead_code)]

    use super::*;

    /// The star field. Tuning lives in `stars.wgsl` apart from these.
    #[derive(Resource, Clone, Copy, ExtractResource, ShaderType)]
    pub struct StarsSettings {
        /// direction to the sun: the field turns with it through the day
        pub sun_position: Vec3,
        /// 0 by day, 1 at night
        pub night: f32,
        /// linear brightness of the brightest star
        pub brightness: f32,
        /// radius of the largest star in screen pixels
        pub size: f32,
        /// chance a grid cell holds a star
        pub density: f32,
        /// seconds, for the twinkle
        pub time: f32,
    }

    impl Default for StarsSettings {
        fn default() -> Self {
            Self {
                sun_position: Vec3::Y,
                night: 0.0,
                brightness: 4.0,
                size: 1.0,
                density: 0.65,
                time: 0.0,
            }
        }
    }
}

/// Stars fade in as the sun drops below the horizon, like the sky's night colour.
fn update_stars_settings(
    light: Res<SceneGlobalLight>,
    time: Res<Time>,
    mut settings: ResMut<StarsSettings>,
) {
    // the light travels upward once the sun is below the horizon
    let t = (light.sun_direction.y / 0.15).clamp(0.0, 1.0);
    let night = t * t * (3.0 - 2.0 * t);
    let sun_position = -light.sun_direction;
    settings.night = night;
    settings.sun_position = sun_position;
    settings.time = time.elapsed_secs();
}

/// The star pass reads the view's depth texture, which bevy only makes bindable on request.
fn bind_depth_texture(mut cameras: Query<&mut Camera3d, With<AtmosphereCamera>>) {
    for mut camera in cameras.iter_mut() {
        let usages = TextureUsages::from(camera.depth_texture_usages);
        if !usages.contains(TextureUsages::TEXTURE_BINDING) {
            camera.depth_texture_usages = (usages | TextureUsages::TEXTURE_BINDING).into();
        }
    }
}

/// A view that draws the sky, and so the stars.
#[derive(Component, Clone)]
struct StarsView;

impl ExtractComponent for StarsView {
    type QueryData = &'static AtmosphereCamera;
    type QueryFilter = ();
    type Out = Self;

    fn extract_component(_: QueryItem<'_, Self::QueryData>) -> Option<Self> {
        Some(Self)
    }
}

#[derive(Resource, Default)]
struct StarsUniformBuffer(UniformBuffer<StarsSettings>);

fn prepare_stars_uniform(
    settings: Res<StarsSettings>,
    mut buffer: ResMut<StarsUniformBuffer>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    buffer.0.set(*settings);
    buffer.0.write_buffer(&device, &queue);
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
struct StarsPipelineKey {
    samples: u32,
    /// the view has depth of field, and so a transparent focus texture to read coverage from
    focus: bool,
    hdr: bool,
}

#[derive(Resource)]
struct StarsPipeline {
    shader: Handle<Shader>,
    layouts: HashMap<(u32, bool), BindGroupLayout>,
}

impl FromWorld for StarsPipeline {
    fn from_world(world: &mut World) -> Self {
        let shader = world
            .resource::<AssetServer>()
            .load("embedded://shaders/stars.wgsl");
        let device = world.resource::<RenderDevice>();

        let mut layouts = HashMap::new();
        for samples in [1, 2, 4, 8] {
            let multisampled = samples > 1;
            for focus in [false, true] {
                let mut entries = vec![
                    uniform_buffer::<ViewUniform>(true).build(0, ShaderStages::FRAGMENT),
                    if multisampled {
                        texture_depth_2d_multisampled()
                    } else {
                        texture_depth_2d()
                    }
                    .build(1, ShaderStages::FRAGMENT),
                    texture_cube(TextureSampleType::Float { filterable: true })
                        .build(2, ShaderStages::FRAGMENT),
                    sampler(SamplerBindingType::Filtering).build(3, ShaderStages::FRAGMENT),
                    uniform_buffer::<StarsSettings>(false).build(4, ShaderStages::FRAGMENT),
                ];
                if focus {
                    entries.push(
                        if multisampled {
                            texture_2d_multisampled(TextureSampleType::Float { filterable: false })
                        } else {
                            texture_2d(TextureSampleType::Float { filterable: false })
                        }
                        .build(5, ShaderStages::FRAGMENT),
                    );
                }
                layouts.insert(
                    (samples, focus),
                    device.create_bind_group_layout(Some("stars bind group layout"), &entries),
                );
            }
        }

        Self { shader, layouts }
    }
}

impl SpecializedRenderPipeline for StarsPipeline {
    type Key = StarsPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        let mut shader_defs = Vec::new();
        if key.samples > 1 {
            shader_defs.push("MULTISAMPLED".into());
        }
        if key.focus {
            shader_defs.push("FOCUS".into());
        }
        RenderPipelineDescriptor {
            label: Some("stars pipeline".into()),
            layout: vec![self.layouts[&(key.samples, key.focus)].clone()],
            push_constant_ranges: Vec::new(),
            vertex: fullscreen_shader_vertex_state(),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs,
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format: if key.hdr {
                        ViewTarget::TEXTURE_FORMAT_HDR
                    } else {
                        TextureFormat::bevy_default()
                    },
                    // stars only add light to the frame already there
                    blend: Some(BlendState {
                        color: BlendComponent {
                            src_factor: BlendFactor::One,
                            dst_factor: BlendFactor::One,
                            operation: BlendOperation::Add,
                        },
                        alpha: BlendComponent {
                            src_factor: BlendFactor::Zero,
                            dst_factor: BlendFactor::One,
                            operation: BlendOperation::Add,
                        },
                    }),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            zero_initialize_workgroup_memory: false,
        }
    }
}

#[derive(Component)]
struct StarsPipelineId {
    id: CachedRenderPipelineId,
    key: StarsPipelineKey,
}

fn prepare_stars_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    mut pipelines: ResMut<SpecializedRenderPipelines<StarsPipeline>>,
    pipeline: Res<StarsPipeline>,
    views: Query<(Entity, &ExtractedView, &Msaa, Has<TransparentFocusTexture>), With<StarsView>>,
) {
    for (entity, view, msaa, focus) in views.iter() {
        let key = StarsPipelineKey {
            samples: msaa.samples(),
            focus,
            hdr: view.hdr,
        };
        let id = pipelines.specialize(&pipeline_cache, &pipeline, key);
        commands.entity(entity).insert(StarsPipelineId { id, key });
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct StarsLabel;

#[derive(Default)]
struct StarsNode;

impl ViewNode for StarsNode {
    type ViewQuery = (
        Read<ViewUniformOffset>,
        Read<ViewTarget>,
        Read<ViewDepthTexture>,
        Read<StarsPipelineId>,
        Option<Read<TransparentFocusTexture>>,
    );

    fn run<'w>(
        &self,
        _: &mut RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        (view_uniform_offset, view_target, depth, pipeline_id, focus): QueryItem<
            'w,
            Self::ViewQuery,
        >,
        world: &'w World,
    ) -> Result<(), NodeRunError> {
        // nothing to draw by day
        if world.resource::<StarsSettings>().night <= 0.0 {
            return Ok(());
        }
        let pipeline = world.resource::<StarsPipeline>();
        let (Some(render_pipeline), Some(view_binding), Some(stars_binding), Some(sky)) = (
            world
                .resource::<PipelineCache>()
                .get_render_pipeline(pipeline_id.id),
            world.resource::<ViewUniforms>().uniforms.binding(),
            world.resource::<StarsUniformBuffer>().0.binding(),
            world
                .resource::<RenderAssets<GpuImage>>()
                .get(&world.resource::<AtmosphereImage>().handle),
        ) else {
            return Ok(());
        };
        let layout = &pipeline.layouts[&(pipeline_id.key.samples, pipeline_id.key.focus)];

        let device = render_context.render_device();
        let bind_group = match (pipeline_id.key.focus, focus) {
            (true, Some(focus)) => device.create_bind_group(
                Some("stars bind group"),
                layout,
                &BindGroupEntries::sequential((
                    view_binding,
                    depth.view(),
                    &sky.texture_view,
                    &sky.sampler,
                    stars_binding,
                    &focus.texture.default_view,
                )),
            ),
            _ => device.create_bind_group(
                Some("stars bind group"),
                layout,
                &BindGroupEntries::sequential((
                    view_binding,
                    depth.view(),
                    &sky.texture_view,
                    &sky.sampler,
                    stars_binding,
                )),
            ),
        };

        let mut pass = render_context
            .command_encoder()
            .begin_render_pass(&RenderPassDescriptor {
                label: Some("stars"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    // drawn straight onto the frame, no copy
                    view: view_target.main_texture_view(),
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        pass.set_pipeline(render_pipeline);
        pass.set_bind_group(0, &bind_group, &[view_uniform_offset.offset]);
        pass.draw(0..3, 0..1);

        Ok(())
    }
}
