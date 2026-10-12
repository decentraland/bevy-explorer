//! Scene-side plugin: turns each `PbParticleSystem` into a renderer child entity holding the
//! particle buffer, its material and the CPU spawner. Value changes update the emitter in
//! place so live particles survive; removing the component purges them.

mod config;
mod material;
mod simulate;

use bevy::{
    pbr::MaterialPlugin,
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssetUsages,
        render_resource::BufferUsages,
        storage::ShaderStorageBuffer,
        view::NoFrustumCulling,
    },
    transform::TransformSystem,
};
use common::{debug_panic, structs::PrimaryUser};
use dcl_component::{
    proto_components::{
        common::texture_union::Tex,
        sdk::components::{pb_particle_system::PlaybackState, PbParticleSystem},
    },
    ComponentPosition, SceneComponentId,
};
use scene_runner::{
    renderer_context::RendererSceneContext,
    update_world::{material::TextureResolver, AddCrdtInterfaceExt},
    ContainerEntity, ContainingScene,
};

use crate::ParticleSystem;
use config::{features, EmitterConfig, SimParams, Spawner, BUFFER_HEADER_SIZE, PARTICLE_SIZE};
use material::ParticleMaterial;
use simulate::ParticleSimulatePlugin;

/// Prewarm simulates one lifetime in steps of this many seconds.
const PREWARM_STEP: f32 = 1. / 30.;
const MAX_PREWARM_STEPS: usize = 300;

pub struct ParticleSystemPlugin;

impl Plugin for ParticleSystemPlugin {
    fn build(&self, app: &mut App) {
        crate::embed_shaders(app);

        app.add_plugins((
            MaterialPlugin::<ParticleMaterial> {
                prepass_enabled: false,
                shadows_enabled: false,
                ..default()
            },
            ParticleSimulatePlugin,
        ));

        app.init_resource::<ActiveScenes>();
        app.init_resource::<ParticleMeshes>();

        app.add_crdt_lww_component::<PbParticleSystem, ParticleSystem>(
            SceneComponentId::PARTICLE_SYSTEM,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(Update, (collect_active_scenes, sync_emitters));
        app.add_systems(
            PostUpdate,
            tick_emitters.after(TransformSystem::TransformPropagate),
        );

        app.add_observer(particle_system_on_remove);
    }
}

#[derive(Default, Resource, Deref, DerefMut)]
struct ActiveScenes(HashSet<Entity>);

#[derive(Component)]
#[relationship(relationship_target = ParticleRenderers)]
struct ParticleRendererOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = ParticleRendererOf)]
struct ParticleRenderers(Vec<Entity>);

/// The live state of one particle system, on its renderer child entity.
#[derive(Component)]
pub struct ParticleEmitter {
    pub buffer: Handle<ShaderStorageBuffer>,
    pub capacity: u32,
    pub features: u32,
    pub config: EmitterConfig,
    /// simulation steps to run this frame, rebuilt by `tick_emitters`
    pub steps: Vec<SimParams>,
    spawner: Spawner,
    scene_root: Entity,
    material: Handle<ParticleMaterial>,
    pending_prewarm: bool,
    pending_reset: bool,
}

/// Quad meshes by particle capacity; position.xy is the quad corner, position.z the index.
#[derive(Default, Resource)]
struct ParticleMeshes(HashMap<u32, Handle<Mesh>>);

impl ParticleMeshes {
    fn get(&mut self, capacity: u32, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        self.0
            .entry(capacity)
            .or_insert_with(|| {
                let mut positions = Vec::with_capacity(capacity as usize * 4);
                let mut indices = Vec::with_capacity(capacity as usize * 6);
                for index in 0..capacity {
                    let z = index as f32;
                    positions.extend([
                        [-0.5, -0.5, z],
                        [0.5, -0.5, z],
                        [0.5, 0.5, z],
                        [-0.5, 0.5, z],
                    ]);
                    let base = index * 4;
                    indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                }
                meshes.add(
                    Mesh::new(
                        PrimitiveTopology::TriangleList,
                        RenderAssetUsages::RENDER_WORLD,
                    )
                    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
                    .with_inserted_indices(Indices::U32(indices)),
                )
            })
            .clone()
    }
}

fn particle_buffer(capacity: u32) -> ShaderStorageBuffer {
    let size = BUFFER_HEADER_SIZE + PARTICLE_SIZE * capacity as u64;
    // zeroed = every slot dead
    let mut buffer =
        ShaderStorageBuffer::new(&vec![0; size as usize], RenderAssetUsages::RENDER_WORLD);
    buffer.buffer_description.label = Some("particles");
    buffer.buffer_description.usage = BufferUsages::STORAGE | BufferUsages::COPY_DST;
    buffer
}

fn collect_active_scenes(
    primary_user: Single<Entity, With<PrimaryUser>>,
    mut active_scenes: ResMut<ActiveScenes>,
    containing_scene: ContainingScene,
) {
    let player_in = containing_scene.get(*primary_user);
    if **active_scenes != player_in {
        **active_scenes = player_in;
    }
}

/// Create or update the renderer child for every new or changed `ParticleSystem`.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn sync_emitters(
    mut commands: Commands,
    particle_systems: Query<
        (
            Entity,
            &ParticleSystem,
            Option<&ContainerEntity>,
            Option<&ParticleRenderers>,
        ),
        Changed<ParticleSystem>,
    >,
    renderer_scene_contexts: Query<&RendererSceneContext>,
    mut emitters: Query<&mut ParticleEmitter>,
    mut buffers: ResMut<Assets<ShaderStorageBuffer>>,
    mut materials: ResMut<Assets<ParticleMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut particle_meshes: ResMut<ParticleMeshes>,
    mut texture_resolver: TextureResolver,
) {
    for (entity, particle_system, maybe_container_entity, maybe_renderers) in &particle_systems {
        let Some(container_entity) = maybe_container_entity else {
            debug_panic!("Particle system does not have ContainerEntity.");
        };
        let Ok(renderer_scene_context) = renderer_scene_contexts.get(container_entity.root) else {
            debug_panic!("Particle system is not contained in a valid scene.");
        };

        let mut config = EmitterConfig::from_proto(particle_system);
        let texture = particle_system.texture.as_ref().and_then(|texture| {
            texture_resolver
                .resolve_texture(renderer_scene_context, &Tex::Texture(texture.clone()))
                .inspect_err(|err| {
                    error!("Could not resolve particle system texture due to '{err:?}'.")
                })
                .ok()
                .map(|resolved| resolved.image)
        });
        if texture.is_some() {
            config.features |= features::TEXTURE;
        }

        let existing = maybe_renderers
            .and_then(|renderers| renderers.collection().first().copied())
            .and_then(|child| emitters.get_mut(child).ok().map(|emitter| (child, emitter)));

        match existing {
            Some((child, mut emitter)) => {
                debug!(
                    "particle emitter {child} updated: features {:#x}",
                    config.features
                );
                // modify in place: live particles survive value changes; the buffer only
                // grows, so lowering max_particles keeps the old limit
                let capacity = config.max_particles.max(1).max(emitter.capacity);
                if capacity != emitter.capacity {
                    emitter.buffer = buffers.add(particle_buffer(capacity));
                    emitter.capacity = capacity;
                    commands
                        .entity(child)
                        .insert(Mesh3d(particle_meshes.get(capacity, &mut meshes)));
                }

                let was = emitter.config.playback;
                if config.playback == PlaybackState::PsStopped && was != PlaybackState::PsStopped {
                    emitter.pending_reset = true;
                }
                if config.playback != PlaybackState::PsStopped && was == PlaybackState::PsStopped {
                    let spawner_config = config.clone();
                    emitter.spawner.reset(&spawner_config);
                    emitter.pending_prewarm = config.prewarm && config.looping;
                }
                emitter.spawner.sync_bursts(&config);

                if let Some(material) = materials.get_mut(&emitter.material) {
                    material.params = config.render;
                    material.particles = emitter.buffer.clone();
                    material.texture = texture;
                    material.features = config.features;
                    material.alpha_mode = config.alpha_mode;
                }
                emitter.features = config.features;
                emitter.config = config;
            }
            None => {
                let capacity = config.max_particles.max(1);
                let buffer = buffers.add(particle_buffer(capacity));
                let material = materials.add(ParticleMaterial {
                    params: config.render,
                    particles: buffer.clone(),
                    texture,
                    features: config.features,
                    alpha_mode: config.alpha_mode,
                });
                let spawner = Spawner::new(&config);
                let pending_prewarm = config.prewarm && config.looping;
                debug!(
                    "particle emitter for {entity} created: features {:#x}, capacity {capacity}, rate {}, playback {:?}, bursts {}",
                    config.features, config.rate, config.playback, config.bursts.len()
                );
                commands.spawn((
                    ChildOf(entity),
                    ParticleRendererOf(entity),
                    Transform::default(),
                    Mesh3d(particle_meshes.get(capacity, &mut meshes)),
                    MeshMaterial3d(material.clone()),
                    NoFrustumCulling,
                    ParticleEmitter {
                        buffer,
                        capacity,
                        features: config.features,
                        config,
                        steps: Vec::new(),
                        spawner,
                        scene_root: container_entity.root,
                        material,
                        pending_prewarm,
                        pending_reset: false,
                    },
                ));
            }
        }
    }
}

fn particle_system_on_remove(
    trigger: Trigger<OnRemove, ParticleSystem>,
    mut commands: Commands,
    renderers: Query<&ParticleRenderers>,
) {
    if let Ok(renderers) = renderers.get(trigger.target()) {
        for renderer in renderers.collection() {
            commands.entity(*renderer).try_despawn();
        }
    }
}

/// Build this frame's simulation steps: the spawner decides how many particles to emit,
/// the shader does the rest.
fn tick_emitters(
    time: Res<Time>,
    active_scenes: Res<ActiveScenes>,
    mut emitters: Query<(&mut ParticleEmitter, &GlobalTransform)>,
) {
    let dt = time.delta_secs();
    for (mut emitter, transform) in &mut emitters {
        let ParticleEmitter {
            capacity,
            config,
            steps,
            spawner,
            scene_root,
            pending_prewarm,
            pending_reset,
            ..
        } = &mut *emitter;
        steps.clear();

        let playing = config.playback == PlaybackState::PsPlaying;
        let emitting = playing && config.active && active_scenes.contains(scene_root);
        let reset = std::mem::take(pending_reset);

        let (_, rotation, _) = transform.to_scale_rotation_translation();
        let mut params = config.sim;
        params.emitter_transform = transform.compute_matrix();
        params.emitter_rotation = Vec4::from(rotation);
        params.emitter_inv_rotation = Vec4::from(rotation.inverse());
        params.capacity = *capacity;

        let mut push = |dt: f32, spawn_count: u32, reset: bool| {
            steps.push(SimParams {
                dt,
                spawn_count,
                reset: reset as u32,
                seed: rand::random(),
                ..params
            });
        };

        if *pending_prewarm && emitting {
            *pending_prewarm = false;
            let count =
                ((config.sim.lifetime / PREWARM_STEP).ceil() as usize).clamp(1, MAX_PREWARM_STEPS);
            let step = config.sim.lifetime / count as f32;
            for _ in 0..count {
                push(step, spawner.tick(step, config), false);
            }
        }

        if playing {
            let spawn_count = if emitting {
                spawner.tick(dt, config)
            } else {
                0
            };
            push(dt, spawn_count, reset);
        } else if reset {
            push(0., 0, true);
        }
    }
}
