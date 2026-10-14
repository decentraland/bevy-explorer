//! Render-world side: one compute dispatch per emitter per step, before the cameras draw.

use bevy::{
    platform::collections::HashMap,
    prelude::*,
    render::{
        extract_component::{ExtractComponent, ExtractComponentPlugin},
        graph::CameraDriverLabel,
        render_asset::RenderAssets,
        render_graph::{self, RenderGraph, RenderLabel},
        render_resource::{
            binding_types::{storage_buffer_sized, uniform_buffer},
            BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries, BufferId,
            CachedComputePipelineId, ComputePassDescriptor, ComputePipelineDescriptor,
            DynamicUniformBuffer, PipelineCache, ShaderStages, SpecializedComputePipeline,
            SpecializedComputePipelines,
        },
        renderer::{RenderContext, RenderDevice, RenderQueue},
        storage::{GpuShaderStorageBuffer, ShaderStorageBuffer},
        Render, RenderApp, RenderSet,
    },
};

use super::{
    config::{features, shader_defs, SimParams, WORKGROUP_SIZE},
    ParticleEmitter,
};

pub struct ParticleSimulatePlugin;

impl Plugin for ParticleSimulatePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractComponentPlugin::<ParticleEmitter>::default());
    }

    fn finish(&self, app: &mut App) {
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .init_resource::<ParticleSimPipeline>()
            .init_resource::<SpecializedComputePipelines<ParticleSimPipeline>>()
            .init_resource::<ParticleSimBatches>()
            .add_systems(
                Render,
                prepare_particle_sims.in_set(RenderSet::PrepareBindGroups),
            );

        let mut render_graph = render_app.world_mut().resource_mut::<RenderGraph>();
        render_graph.add_node(ParticleSimLabel, ParticleSimNode);
        render_graph.add_node_edge(ParticleSimLabel, CameraDriverLabel);
    }
}

/// What the render world needs from an emitter: its buffer and this frame's steps.
#[derive(Component, Clone)]
pub struct ExtractedEmitter {
    pub buffer: Handle<ShaderStorageBuffer>,
    pub features: u32,
    pub capacity: u32,
    pub steps: Vec<SimParams>,
}

impl ExtractComponent for ParticleEmitter {
    type QueryData = &'static ParticleEmitter;
    type QueryFilter = ();
    type Out = ExtractedEmitter;

    fn extract_component(emitter: &ParticleEmitter) -> Option<ExtractedEmitter> {
        if emitter.steps.is_empty() {
            return None;
        }
        Some(ExtractedEmitter {
            buffer: emitter.buffer.clone(),
            features: emitter.features & features::SIM_MASK,
            capacity: emitter.capacity,
            steps: emitter.steps.clone(),
        })
    }
}

#[derive(Resource)]
struct ParticleSimPipeline {
    layout: BindGroupLayout,
    shader: Handle<Shader>,
}

impl FromWorld for ParticleSimPipeline {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        let layout = device.create_bind_group_layout(
            "particle sim layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::COMPUTE,
                (
                    uniform_buffer::<SimParams>(true),
                    storage_buffer_sized(false, None),
                ),
            ),
        );
        let shader = world
            .resource::<AssetServer>()
            .load("embedded://particle_system/shaders/simulate.wgsl");
        Self { layout, shader }
    }
}

impl SpecializedComputePipeline for ParticleSimPipeline {
    type Key = u32;

    fn specialize(&self, features: u32) -> ComputePipelineDescriptor {
        ComputePipelineDescriptor {
            label: Some("particle simulate".into()),
            layout: vec![self.layout.clone()],
            push_constant_ranges: Vec::new(),
            shader: self.shader.clone(),
            shader_defs: shader_defs(features),
            entry_point: "simulate".into(),
            zero_initialize_workgroup_memory: false,
        }
    }
}

struct SimBatch {
    pipeline: CachedComputePipelineId,
    bind_group: BindGroup,
    /// (uniform offset, workgroups) per step
    steps: Vec<(u32, u32)>,
}

#[derive(Resource, Default)]
struct ParticleSimBatches {
    uniforms: DynamicUniformBuffer<SimParams>,
    /// keyed by (uniform buffer, particle buffer); either reallocating invalidates it
    bind_groups: HashMap<(BufferId, BufferId), BindGroup>,
    batches: Vec<SimBatch>,
}

#[allow(clippy::too_many_arguments)]
fn prepare_particle_sims(
    mut batches: ResMut<ParticleSimBatches>,
    emitters: Query<&ExtractedEmitter>,
    gpu_buffers: Res<RenderAssets<GpuShaderStorageBuffer>>,
    pipeline: Res<ParticleSimPipeline>,
    mut pipelines: ResMut<SpecializedComputePipelines<ParticleSimPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    queue: Res<RenderQueue>,
) {
    let ParticleSimBatches {
        uniforms,
        bind_groups,
        batches,
    } = &mut *batches;
    batches.clear();
    uniforms.clear();

    let mut pending = Vec::new();
    for emitter in &emitters {
        let Some(gpu_buffer) = gpu_buffers.get(&emitter.buffer) else {
            continue;
        };
        let steps = emitter
            .steps
            .iter()
            .filter(|step| step.slot_count > 0)
            .map(|step| {
                (
                    uniforms.push(step),
                    step.slot_count.div_ceil(WORKGROUP_SIZE),
                )
            })
            .collect::<Vec<_>>();
        if steps.is_empty() {
            continue;
        }
        let pipeline_id = pipelines.specialize(&pipeline_cache, &pipeline, emitter.features);
        pending.push((gpu_buffer.buffer.clone(), pipeline_id, steps));
    }
    if pending.is_empty() {
        bind_groups.clear();
        return;
    }

    uniforms.write_buffer(&device, &queue);
    let (Some(uniform_buffer), Some(uniform_binding)) = (uniforms.buffer(), uniforms.binding())
    else {
        return;
    };
    let uniform_id = uniform_buffer.id();

    let mut used = Vec::with_capacity(pending.len());
    for (buffer, pipeline_id, steps) in pending {
        let key = (uniform_id, buffer.id());
        let bind_group = bind_groups
            .entry(key)
            .or_insert_with(|| {
                device.create_bind_group(
                    "particle sim",
                    &pipeline.layout,
                    &BindGroupEntries::sequential((
                        uniform_binding.clone(),
                        buffer.as_entire_binding(),
                    )),
                )
            })
            .clone();
        used.push(key);
        batches.push(SimBatch {
            pipeline: pipeline_id,
            bind_group,
            steps,
        });
    }
    bind_groups.retain(|key, _| used.contains(key));
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
struct ParticleSimLabel;

struct ParticleSimNode;

impl render_graph::Node for ParticleSimNode {
    fn run<'w>(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        render_context: &mut RenderContext<'w>,
        world: &'w World,
    ) -> Result<(), render_graph::NodeRunError> {
        let batches = world.resource::<ParticleSimBatches>();
        let pipeline_cache = world.resource::<PipelineCache>();

        for batch in &batches.batches {
            let Some(pipeline) = pipeline_cache.get_compute_pipeline(batch.pipeline) else {
                continue;
            };
            let mut pass =
                render_context
                    .command_encoder()
                    .begin_compute_pass(&ComputePassDescriptor {
                        label: Some("particle simulate"),
                        timestamp_writes: None,
                    });
            pass.set_pipeline(pipeline);
            for (offset, workgroups) in &batch.steps {
                pass.set_bind_group(0, &batch.bind_group, &[*offset]);
                pass.dispatch_workgroups(*workgroups, 1, 1);
            }
        }
        Ok(())
    }
}
