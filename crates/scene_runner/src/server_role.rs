//! In-engine authoritative server: with [`LocalSceneServer`] naming the current realm, each of its
//! authoritative scenes gets a second copy of itself in server role (isServer() true, its own crdt context) joined to the
//! client copy over the loopback scene room (`comms::loopback`). The copy only forwards
//! logic and collision components across the scene boundary, and anything renderable its
//! gltf colliders bring along is stripped, so it never draws.

use bevy::{platform::collections::HashMap, prelude::*};
use bevy_console::ConsoleCommand;
use common::structs::{server_mode, CurrentRealm, LocalSceneServer};
use comms::{
    global_crdt::{ForeignPlayer, GlobalCrdtState, HiddenPeer, SceneCrdtContext},
    loopback::{
        spawn_server_end, LocalSceneServer as LocalServerRoom, LocalSceneServers, LoopbackEnd,
        AUTH_SERVER_IDENTITY,
    },
};
use console::DoAddConsoleCommand;
use dcl::interface::CrdtComponentInterfaces;
use dcl_component::SceneComponentId;

use crate::{
    initialize_scene::{
        LiveScenes, PortableScenes, SceneEntityDefinitionHandle, SceneHash, SceneLoading,
    },
    renderer_context::{RendererSceneContext, FROZEN_BLOCK},
    update_world::{gltf_container::GltfDefinition, CrdtExtractors},
    ContainerEntity, SceneSets,
};

/// Marks a scene root as the server copy of `client`.
#[derive(Component)]
pub struct ServerRole {
    pub client: Entity,
}

/// What a server copy forwards to the engine: its logic-side state and what raycasts need to
/// collide with. Everything else (mesh renderers, materials, ui, audio, video, avatars, ...)
/// stays in the scene's own store.
const SERVER_COMPONENTS: [SceneComponentId; 5] = [
    SceneComponentId::TRANSFORM,
    SceneComponentId::TWEEN,
    SceneComponentId::RAYCAST,
    SceneComponentId::MESH_COLLIDER,
    SceneComponentId::GLTF_CONTAINER,
];

pub fn scene_interfaces(extractors: &CrdtExtractors, server: bool) -> CrdtComponentInterfaces {
    CrdtComponentInterfaces(HashMap::from_iter(
        extractors
            .0
            .iter()
            .filter(|(id, _)| !server || SERVER_COMPONENTS.contains(id))
            .map(|(id, interface)| (*id, interface.crdt_type())),
    ))
}

pub struct ServerRolePlugin;

impl Plugin for ServerRolePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalSceneServer>().add_systems(
            Update,
            (
                reap_server_copies,
                spawn_server_copies,
                follow_client_freeze,
            )
                .chain()
                .in_set(SceneSets::PostInit),
        );
        app.add_systems(PostUpdate, strip_server_renderables);
        app.add_console_command::<SceneRenderStatsCommand, _>(scene_render_stats);
        app.add_console_command::<LocalSceneServerCommand, _>(local_scene_server_cmd);
    }
}

/// Serve the authoritative scenes of a realm in this engine; for the editor host, whose preview
/// realm it names when Play can run a server (`off` stops it). Never a scene's or a link's call.
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/local_scene_server")]
struct LocalSceneServerCommand {
    /// realm url, `off`, or nothing for the current realm
    realm: Option<String>,
    /// the secret the server copy's storage requests to that realm carry
    storage_token: Option<String>,
}

fn local_scene_server_cmd(
    mut input: ConsoleCommand<LocalSceneServerCommand>,
    mut local_server: ResMut<LocalSceneServer>,
    realm: Res<CurrentRealm>,
) {
    let Some(Ok(LocalSceneServerCommand {
        realm: target,
        storage_token,
    })) = input.take()
    else {
        return;
    };
    let target = match target.as_deref() {
        Some("off") => None,
        Some(url) => Some(LocalSceneServer::realm_root(url).to_owned()),
        None => Some(LocalSceneServer::realm_root(&realm.about_url).to_owned()),
    };
    let reply = match &target {
        Some(root) => format!("local scene server on for {root}"),
        None => "local scene server off".to_owned(),
    };
    info!("{reply}");
    input.reply_ok(reply);
    let storage_token = storage_token.filter(|_| target.is_some());
    if local_server.realm != target || local_server.storage_token != storage_token {
        *local_server = LocalSceneServer {
            realm: target,
            storage_token,
        };
    }
}

#[allow(clippy::type_complexity)]
fn spawn_server_copies(
    mut commands: Commands,
    enabled: Res<LocalSceneServer>,
    realm: Res<CurrentRealm>,
    clients: Query<
        (Entity, &RendererSceneContext, &SceneEntityDefinitionHandle),
        Without<ServerRole>,
    >,
    live_scenes: Res<LiveScenes>,
    portables: Res<PortableScenes>,
    mut servers: ResMut<LocalSceneServers>,
) {
    if !enabled.serves(&realm) || server_mode() {
        return;
    }
    for (client, ctx, definition) in &clients {
        // the realm's own parcel scenes only, never a portable one
        if !ctx.authoritative_multiplayer
            || servers.0.contains_key(&ctx.hash)
            || live_scenes.scenes.get(&ctx.hash) != Some(&client)
            || portables.contains_key(&ctx.hash)
        {
            continue;
        }
        let context = commands
            .spawn(GlobalCrdtState::new(Some(ctx.hash.clone())).server_role(AUTH_SERVER_IDENTITY))
            .id();
        let server_end = spawn_server_end(&mut commands, &ctx.hash, context);
        let root = commands
            .spawn((
                SceneHash(ctx.hash.clone()),
                SceneLoading::SceneEntity,
                // weak: a reload must find the definition gone and fetch it again, not this copy's
                SceneEntityDefinitionHandle(definition.0.clone_weak()),
                ServerRole { client },
                SceneCrdtContext(context),
            ))
            .id();
        info!(
            "starting the local server copy of {} ({}) as {root:?}",
            ctx.title, ctx.hash
        );
        servers.0.insert(
            ctx.hash.clone(),
            LocalServerRoom {
                context,
                server_end,
            },
        );
    }
}

/// A server copy is paused while its client copy is (the editor's edit mode), so it only runs
/// while the scene does.
fn follow_client_freeze(
    clients: Query<&RendererSceneContext, Without<ServerRole>>,
    mut copies: Query<(&ServerRole, &mut RendererSceneContext)>,
) {
    for (role, mut ctx) in &mut copies {
        let Ok(client) = clients.get(role.client) else {
            continue;
        };
        let frozen = client.blocked.contains(FROZEN_BLOCK);
        if frozen != ctx.blocked.contains(FROZEN_BLOCK) {
            if frozen {
                ctx.blocked.insert(FROZEN_BLOCK);
            } else {
                ctx.blocked.remove(FROZEN_BLOCK);
            }
        }
    }
}

/// A server copy's gltf colliders arrive inside full gltf instances; keep the collider data
/// and drop what would draw (the shared mesh assets are the client copy's, nothing new uploads).
#[allow(clippy::type_complexity)]
fn strip_server_renderables(
    mut commands: Commands,
    meshes: Query<
        (Entity, &ContainerEntity),
        (With<Mesh3d>, Or<(Added<Mesh3d>, Added<ContainerEntity>)>),
    >,
    servers: Query<(), With<ServerRole>>,
) {
    for (entity, container) in &meshes {
        if servers.contains(container.root) {
            commands.entity(entity).remove::<(
                Mesh3d,
                MeshMaterial3d<StandardMaterial>,
                MeshMaterial3d<scene_material::SceneMaterial>,
            )>();
        }
    }
}

fn reap_server_copies(
    mut commands: Commands,
    copies: Query<(Entity, &ServerRole, &SceneHash)>,
    clients: Query<(), Or<(With<RendererSceneContext>, With<SceneLoading>)>>,
    enabled: Res<LocalSceneServer>,
    realm: Res<CurrentRealm>,
    mut servers: ResMut<LocalSceneServers>,
) {
    let serving = enabled.serves(&realm);
    for (root, role, hash) in &copies {
        if serving && clients.contains(role.client) {
            continue;
        }
        info!("stopping the local server copy of {}", hash.0);
        commands.entity(root).despawn();
        if let Some(room) = servers.0.remove(&hash.0) {
            commands.entity(room.server_end).try_despawn();
            commands.entity(room.context).try_despawn();
        }
    }
}

/// Per-scene renderable counts (entities, gltf containers, meshes) and world asset totals
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/scene_render_stats")]
struct SceneRenderStatsCommand;

#[allow(clippy::too_many_arguments)]
fn scene_render_stats(
    mut input: ConsoleCommand<SceneRenderStatsCommand>,
    scenes: Query<(Entity, &RendererSceneContext, Has<ServerRole>)>,
    containers: Query<&ContainerEntity>,
    gltfs: Query<&ContainerEntity, With<GltfDefinition>>,
    meshes: Query<Entity, With<Mesh3d>>,
    parents: Query<&ChildOf>,
    mesh_assets: Res<Assets<Mesh>>,
    images: Res<Assets<Image>>,
    gltf_assets: Res<Assets<bevy::gltf::Gltf>>,
    (players, contexts, loopbacks, servers): (
        Query<Has<HiddenPeer>, With<ForeignPlayer>>,
        Query<&GlobalCrdtState>,
        Query<(), With<LoopbackEnd>>,
        Res<LocalSceneServers>,
    ),
) {
    if input.take().is_none() {
        return;
    }
    let mut counts: HashMap<Entity, [usize; 3]> = HashMap::new();
    for container in &containers {
        counts.entry(container.root).or_default()[0] += 1;
    }
    for container in &gltfs {
        counts.entry(container.root).or_default()[1] += 1;
    }
    for mesh in &meshes {
        let mut entity = mesh;
        while !scenes.contains(entity) {
            let Ok(parent) = parents.get(entity) else {
                break;
            };
            entity = parent.parent();
        }
        if scenes.contains(entity) {
            counts.entry(entity).or_default()[2] += 1;
        }
    }
    let mut lines = scenes
        .iter()
        .map(|(root, ctx, server)| {
            let [entities, gltf, mesh] = counts.get(&root).copied().unwrap_or_default();
            format!(
                "{} [{}] role={} entities={entities} gltf_containers={gltf} meshes={mesh}",
                ctx.title,
                ctx.hash,
                if server { "server" } else { "client" },
            )
        })
        .collect::<Vec<_>>();
    lines.push(format!(
        "world: mesh3d={} mesh_assets={} images={} gltf_assets={}",
        meshes.iter().count(),
        mesh_assets.len(),
        images.len(),
        gltf_assets.len()
    ));
    lines.push(format!(
        "comms: foreign_players={} hidden_peers={} crdt_contexts={} server_rooms={} loopback_ends={} local_servers={}",
        players.iter().count(),
        players.iter().filter(|hidden| *hidden).count(),
        contexts.iter().count(),
        contexts.iter().filter(|c| c.is_server_role()).count(),
        loopbacks.iter().count(),
        servers.0.len(),
    ));
    input.reply_ok(lines.join("\n"));
}

#[cfg(test)]
mod tests {
    use bevy::{ecs::system::RunSystemOnce, platform::collections::HashSet};
    use dcl::SceneId;

    use super::*;

    fn client_scene(world: &mut World, hash: &str, authoritative: bool) -> Entity {
        let scene = world
            .spawn((
                RendererSceneContext::new(
                    SceneId::DUMMY,
                    hash.to_owned(),
                    "storage_root".to_owned(),
                    false,
                    0,
                    "title".to_owned(),
                    IVec2::ZERO,
                    HashSet::from_iter([IVec2::ZERO]),
                    vec![],
                    Default::default(),
                    vec![],
                    Entity::PLACEHOLDER,
                    0.,
                    false,
                    "sdk7",
                    false,
                    authoritative,
                ),
                SceneEntityDefinitionHandle(Handle::default()),
            ))
            .id();
        world
            .resource_mut::<LiveScenes>()
            .scenes
            .insert(hash.to_owned(), scene);
        scene
    }

    #[test]
    fn only_the_named_realms_authoritative_scenes_get_a_server_copy() {
        let mut world = World::new();
        world.insert_resource(CurrentRealm {
            about_url: "https://page/preview/p1/about".to_owned(),
            ..Default::default()
        });
        world.insert_resource(LocalSceneServer {
            realm: Some("https://page/preview/p2".to_owned()),
            storage_token: None,
        });
        world.init_resource::<LocalSceneServers>();
        world.init_resource::<LiveScenes>();
        world.init_resource::<PortableScenes>();
        let authoritative = client_scene(&mut world, "auth", true);
        client_scene(&mut world, "plain", false);
        let copies = |world: &mut World| {
            world.run_system_once(spawn_server_copies).unwrap();
            world.flush();
            world
                .query::<&ServerRole>()
                .iter(world)
                .map(|role| role.client)
                .collect::<Vec<_>>()
        };

        assert_eq!(copies(&mut world), vec![], "another realm's switch");

        world.resource_mut::<LocalSceneServer>().realm = Some("https://page/preview/p1".to_owned());
        assert_eq!(
            copies(&mut world),
            vec![],
            "a realm that serves no local project"
        );

        world
            .resource_mut::<CurrentRealm>()
            .config
            .local_scene_parcels = Some(vec!["0,0".to_owned()]);
        assert_eq!(copies(&mut world), vec![authoritative]);
        let rooms = world.resource::<LocalSceneServers>();
        assert!(rooms.0.contains_key("auth") && !rooms.0.contains_key("plain"));
    }
}
