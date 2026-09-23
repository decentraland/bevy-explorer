use std::{collections::HashSet, sync::Arc};

use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use common::{
    structs::{AppConfig, CurrentRealm, LandscapeTerrain},
    terrain::TerrainField,
    util::{TaskCompat, TaskExt},
};
use ipfs::{ipfs_path::IpfsPath, EntityDefinition, IpfsResource};
use scene_runner::initialize_scene::{PointerResult, ScenePointers};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Source {
    realm: String,
    connection: String,
    urns: Vec<String>,
}

impl From<&CurrentRealm> for Source {
    fn from(realm: &CurrentRealm) -> Self {
        Self {
            realm: realm.pointer_realm().to_owned(),
            // Scene ownership may use an internal backend identity. HTTP must
            // follow the public connection the player actually reached.
            connection: connection_url(realm).to_owned(),
            urns: realm.config.scenes_urn.clone().unwrap_or_default(),
        }
    }
}

#[derive(Resource, Default)]
pub(super) struct TerrainLoader {
    source: Option<Source>,
    task: Option<Task<Result<TerrainField, String>>>,
    retry_at: f64,
    failures: u32,
    pub status: String,
}

pub(super) struct TerrainLoadingPlugin;

impl Plugin for TerrainLoadingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LandscapeTerrain>()
            .init_resource::<TerrainLoader>()
            .add_systems(Update, load);
    }
}

fn connection_url(realm: &CurrentRealm) -> &str {
    let url = if realm.about_url.is_empty() {
        &realm.address
    } else {
        &realm.about_url
    };
    let url = url.trim_end_matches('/');
    url.strip_suffix("/about").unwrap_or(url)
}

fn manifest_url(realm: &str) -> Result<String, String> {
    let realm = url::Url::parse(realm).map_err(|error| error.to_string())?;
    if !matches!(realm.scheme(), "http" | "https") {
        return Err("Terrain realm must use HTTP or HTTPS".into());
    }
    let host = realm.host_str().ok_or("Terrain realm has no host")?;
    Ok(
        if host == "decentraland.org" || host.ends_with(".decentraland.org") {
            "https://places-dcf8abb.s3.amazonaws.com/WorldManifest.json".into()
        } else if host == "decentraland.zone" || host.ends_with(".decentraland.zone") {
            "https://places-e22845c.s3.us-east-1.amazonaws.com/WorldManifest.json".into()
        } else {
            format!(
                "{}/world-manifest.json",
                realm.origin().ascii_serialization()
            )
        },
    )
}

fn parse_manifest(value: serde_json::Value) -> Result<Vec<[i32; 2]>, String> {
    value
        .get("occupied")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Terrain manifest has no occupied parcel list".to_owned())?
        .iter()
        .map(|parcel| {
            let parcel = parcel.as_str().ok_or("Terrain parcel is not a string")?;
            let (x, z) = parcel
                .split_once(',')
                .ok_or("Terrain parcel needs two coordinates")?;
            Ok([
                x.trim().parse().map_err(|_| "Invalid terrain parcel X")?,
                z.trim().parse().map_err(|_| "Invalid terrain parcel Z")?,
            ])
        })
        .collect()
}

fn world_parcels(
    source: &Source,
    pointers: &ScenePointers,
    definitions: Option<&Assets<EntityDefinition>>,
) -> Result<Option<Vec<[i32; 2]>>, String> {
    let required: HashSet<_> = source
        .urns
        .iter()
        .map(|urn| {
            IpfsPath::new_from_urn::<EntityDefinition>(&urn.replace('?', "?=&"))
                .and_then(|path| path.context_free_hash())
                .map_err(|error| error.to_string())?
                .ok_or_else(|| "Terrain scene URN has no deployment hash".to_owned())
        })
        .collect::<Result<_, _>>()?;
    let mut found = HashSet::new();
    let mut parcels: HashSet<_> = pointers
        .iter()
        .filter_map(|(parcel, pointer)| match pointer {
            PointerResult::Exists { realm, hash, .. }
                if realm == &source.realm && required.contains(hash) =>
            {
                found.insert(hash.clone());
                Some(parcel.to_array())
            }
            _ => None,
        })
        .collect();
    if let Some(definitions) = definitions {
        for (_, definition) in definitions.iter() {
            if !required.contains(&definition.id) {
                continue;
            }
            let coordinates = definition
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("scene"))
                .and_then(|scene| scene.get("parcels"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!(definition.pointers));
            let occupied = parse_manifest(serde_json::json!({"occupied": coordinates}))?;
            if occupied.is_empty() {
                return Err("Terrain scene definition has no parcels".into());
            }
            parcels.extend(occupied);
            found.insert(definition.id.clone());
        }
    }
    Ok((found == required).then(|| parcels.into_iter().collect()))
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn load(
    config: Option<Res<AppConfig>>,
    realm: Option<Res<CurrentRealm>>,
    pointers: Option<Res<ScenePointers>>,
    definitions: Option<Res<Assets<EntityDefinition>>>,
    ipfs: Option<Res<IpfsResource>>,
    time: Res<Time>,
    mut loader: ResMut<TerrainLoader>,
    mut terrain: ResMut<LandscapeTerrain>,
) {
    if loader.is_added()
        || config.as_ref().is_none_or(|value| value.is_changed())
        || realm.as_ref().is_none_or(|value| value.is_changed())
        || definitions.as_ref().is_some_and(|value| value.is_changed())
    {
        let source = realm
            .as_ref()
            .filter(|realm| !realm.address.is_empty())
            .filter(|realm| {
                definitions.as_ref().is_none_or(|definitions| {
                    crate::ground_visibility::terrain_visible(realm, definitions)
                })
            })
            .map(|realm| Source::from(&**realm));
        if source != loader.source {
            *loader = TerrainLoader {
                source,
                status: "waiting for terrain parcels".into(),
                ..default()
            };
            terrain.field = None;
            terrain.generation = terrain.generation.wrapping_add(1);
        }
    };
    if terrain.field.is_some() {
        return;
    }
    let Some(source) = loader.source.clone() else {
        return;
    };
    if let Some(result) = loader.task.as_mut().and_then(TaskExt::complete) {
        loader.task = None;
        match result {
            Ok(field) => {
                loader.status = format!(
                    "ready: {}x{}, floor {}, {} distance steps",
                    field.size, field.size, field.floor, field.max_steps
                );
                terrain.field = Some(Arc::new(field));
                terrain.generation = terrain.generation.wrapping_add(1);
            }
            Err(error) => {
                warn!("Terrain generation failed: {error}");
                loader.status = error;
                loader.failures += 1;
                loader.retry_at =
                    time.elapsed_secs_f64() + f64::from(1_u32 << loader.failures.min(6));
            }
        }
        return;
    }
    if loader.task.is_some() || time.elapsed_secs_f64() < loader.retry_at {
        return;
    }
    if !source.urns.is_empty() {
        let Some(pointers) = pointers else {
            return;
        };
        match world_parcels(&source, &pointers, definitions.as_deref()) {
            Ok(Some(parcels)) => {
                loader.status = format!("building terrain from {} world parcels", parcels.len());
                loader.task = Some(crate::spawn_cpu(move || {
                    TerrainField::from_parcels(&parcels, true).map_err(str::to_owned)
                }));
            }
            Ok(None) => {}
            Err(error) => {
                loader.status = error;
                loader.retry_at = f64::INFINITY;
            }
        }
    } else if let Some(ipfs) = ipfs {
        let ipfs = ipfs.inner.clone();
        loader.status = "loading city terrain manifest".into();
        loader.task = Some(IoTaskPool::get().spawn_compat(async move {
            let manifest = manifest_url(&source.connection)?;
            let client = ipfs.client();
            let request = client
                .get(&manifest)
                .build()
                .map_err(|error| error.to_string())?;
            let response = ipfs
                .async_request(request, client)
                .await
                .map_err(|error| format!("Terrain manifest {manifest}: {error:#}"))?
                .error_for_status()
                .map_err(|error| error.to_string())?;
            let manifest = response.json().await.map_err(|error| error.to_string())?;
            crate::spawn_cpu(move || {
                TerrainField::from_parcels(&parse_manifest(manifest)?, false).map_err(str::to_owned)
            })
            .await
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_realm_identity_is_not_used_as_the_manifest_network_endpoint() {
        let realm = CurrentRealm {
            address: "http://backend.internal".into(),
            about_url: "https://city.example/about".into(),
            ..default()
        };
        let source = Source::from(&realm);
        assert_eq!(source.realm, "http://backend.internal");
        assert_eq!(
            manifest_url(&source.connection).unwrap(),
            "https://city.example/world-manifest.json"
        );
    }

    #[test]
    fn city_manifest_follows_the_connected_realm_not_native_launch_defaults() {
        for (realm, expected) in [
            (
                "https://catalyst.dcl.one/about",
                "https://catalyst.dcl.one/world-manifest.json",
            ),
            (
                "https://city.example:8443/realm/about?realm=city",
                "https://city.example:8443/world-manifest.json",
            ),
            (
                "https://peer.decentraland.org/about",
                "https://places-dcf8abb.s3.amazonaws.com/WorldManifest.json",
            ),
            (
                "https://peer.decentraland.zone",
                "https://places-e22845c.s3.us-east-1.amazonaws.com/WorldManifest.json",
            ),
            (
                "https://decentraland.org.example",
                "https://decentraland.org.example/world-manifest.json",
            ),
        ] {
            assert_eq!(manifest_url(realm).unwrap(), expected);
        }
        assert!(manifest_url("file:///tmp/world-manifest.json").is_err());
        assert!(manifest_url("not-a-realm-url").is_err());
    }

    #[test]
    fn world_waits_for_every_deployment_and_excludes_other_realms() {
        let source = Source {
            realm: "world".into(),
            connection: "https://world.example".into(),
            urns: vec![
                "urn:decentraland:entity:a".into(),
                "urn:decentraland:entity:b".into(),
            ],
        };
        let mut pointers = ScenePointers::default();
        let pointer = |realm: &str, hash: &str| PointerResult::Exists {
            realm: realm.into(),
            hash: hash.into(),
            urn: None,
        };
        pointers.insert(IVec2::ZERO, pointer("world", "a"));
        pointers.insert(IVec2::ONE, pointer("old-world", "b"));
        assert!(world_parcels(&source, &pointers, None).unwrap().is_none());
        pointers.insert(IVec2::new(2, 3), pointer("world", "b"));
        let mut parcels = world_parcels(&source, &pointers, None).unwrap().unwrap();
        parcels.sort();
        assert_eq!(parcels, [[0, 0], [2, 3]]);
    }

    #[test]
    fn overlapping_world_deployments_use_their_complete_definitions() {
        let source = Source {
            realm: "world".into(),
            connection: "https://world.example".into(),
            urns: vec![
                "urn:decentraland:entity:a".into(),
                "urn:decentraland:entity:b".into(),
            ],
        };
        let mut pointers = ScenePointers::default();
        for hash in ["a", "b"] {
            pointers.insert(
                IVec2::ZERO,
                PointerResult::Exists {
                    realm: source.realm.clone(),
                    hash: hash.into(),
                    urn: None,
                },
            );
        }
        assert!(world_parcels(&source, &pointers, None).unwrap().is_none());
        let mut definitions = Assets::<EntityDefinition>::default();
        let _a = definitions.add(EntityDefinition {
            id: "a".into(),
            metadata: Some(serde_json::json!({"scene": {"parcels": ["0,0", "1,0"]}})),
            ..default()
        });
        let _unrelated = definitions.add(EntityDefinition {
            id: "unrelated".into(),
            pointers: vec!["999,999".into()],
            ..default()
        });
        let mut parcels = world_parcels(&source, &pointers, Some(&definitions))
            .unwrap()
            .unwrap();
        parcels.sort();
        assert_eq!(parcels, [[0, 0], [1, 0]]);
        definitions.get_mut(&_a).unwrap().metadata =
            Some(serde_json::json!({"scene": {"parcels": ["bad"]}}));
        assert!(world_parcels(&source, &pointers, Some(&definitions)).is_err());
    }

    #[test]
    fn malformed_manifest_is_not_treated_as_empty_terrain() {
        assert!(parse_manifest(serde_json::json!({"roads": []})).is_err());
        assert!(parse_manifest(serde_json::json!({"occupied": ["1,2,3"]})).is_err());
        assert_eq!(
            parse_manifest(serde_json::json!({"occupied": [" -2, 3 "]})).unwrap(),
            [[-2, 3]]
        );
    }

    #[test]
    fn changing_realm_clears_the_old_field() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        let config = AppConfig::default();
        let realm = CurrentRealm {
            address: "world-one".into(),
            about_url: "world-one/about".into(),
            config: common::structs::ServerConfiguration {
                scenes_urn: Some(vec!["urn:decentraland:entity:a".into()]),
                ..default()
            },
            ..default()
        };
        let mut pointers = ScenePointers::default();
        pointers.insert(
            IVec2::ZERO,
            PointerResult::Exists {
                realm: realm.pointer_realm().into(),
                hash: "a".into(),
                urn: None,
            },
        );
        app.insert_resource(config)
            .insert_resource(realm)
            .insert_resource(pointers)
            .add_plugins(TerrainLoadingPlugin);
        let wait = |app: &mut App| {
            let start = std::time::Instant::now();
            while app.world().resource::<LandscapeTerrain>().field.is_none() {
                assert!(start.elapsed().as_secs() < 5, "terrain task did not finish");
                app.update();
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        };
        wait(&mut app);
        assert_eq!(
            app.world()
                .resource::<LandscapeTerrain>()
                .field
                .as_ref()
                .unwrap()
                .min_parcel,
            [-2, -2]
        );
        app.world_mut().resource_mut::<CurrentRealm>().about_url = "world-two/about".into();
        app.update();
        assert!(app.world().resource::<LandscapeTerrain>().field.is_none());
        app.world_mut().resource_mut::<ScenePointers>().insert(
            IVec2::splat(100),
            PointerResult::Exists {
                realm: "world-two/about".into(),
                hash: "a".into(),
                urn: None,
            },
        );
        wait(&mut app);
        assert_eq!(
            app.world()
                .resource::<LandscapeTerrain>()
                .field
                .as_ref()
                .unwrap()
                .min_parcel,
            [98, 98]
        );
    }
}
