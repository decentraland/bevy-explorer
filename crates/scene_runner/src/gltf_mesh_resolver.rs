use anyhow::anyhow;
use bevy::{ecs::system::SystemParam, gltf::{Gltf, GltfMesh}, prelude::*, utils::HashMap};
use ipfs::IpfsAssetServer;


#[derive(SystemParam)]
pub struct GltfMeshResolver<'w, 's> {
    prev_pending_gltfs: Local<'s, HashMap<String, Handle<Gltf>>>,
    pending_gltfs: Local<'s, HashMap<String, Handle<Gltf>>>,
    ipfas: IpfsAssetServer<'w, 's>,
    gltfs: Res<'w, Assets<Gltf>>,
    gltf_meshes: Res<'w, Assets<GltfMesh>>,
}

impl<'w, 's> GltfMeshResolver<'w, 's> {
    pub fn begin_frame(&mut self) {
        *self.prev_pending_gltfs = std::mem::take(&mut *self.pending_gltfs);
    }

    pub fn resolve_mesh(
        &mut self,
        gltf_src: &str,
        scene_hash: &str,
        name: &str,
    ) -> Result<Option<Handle<Mesh>>, anyhow::Error> {
        let lookup = format!("{gltf_src}##{scene_hash}");
        let h_gltf = self
            .prev_pending_gltfs
            .remove(&lookup)
            .unwrap_or_else(|| self.ipfas.load_content_file(gltf_src, scene_hash).unwrap());
        let gltf = match self.ipfas.asset_server().load_state(h_gltf.id()) {
            bevy::asset::LoadState::Loading => {
                self.pending_gltfs.insert(lookup, h_gltf.clone());
                return Ok(None);
            }
            bevy::asset::LoadState::Loaded => self.gltfs.get(h_gltf.id()).unwrap(),
            bevy::asset::LoadState::NotLoaded | bevy::asset::LoadState::Failed => {
                warn!("failed to load gltf for mesh");
                return Err(anyhow!("failed to load gltf for mesh"));
            }
        };

        let (mesh_index, primitive_index) = name.split_once('/').unwrap_or((name, ""));
        let h_gltf_mesh = match gltf.named_meshes.get(name) {
            Some(h_gm) => h_gm,
            None => {
                let Some(h_gm) = mesh_index
                    .strip_prefix("Mesh")
                    .and_then(|ix_str| ix_str.parse::<usize>().ok())
                    .and_then(|ix| gltf.meshes.get(ix))
                else {
                    return Err(anyhow!("mesh {name:?} not found in gltf {gltf_src}"));
                };

                h_gm
            }
        };
        let Some(gltf_mesh) = self.gltf_meshes.get(h_gltf_mesh) else {
            return Err(anyhow!("no gltf mesh"));
        };
        let primitive = primitive_index
            .strip_prefix("Primitive")
            .and_then(|ix_str| ix_str.parse::<usize>().ok())
            .unwrap_or(0);
        let Some(primitive) = gltf_mesh.primitives.get(primitive) else {
            return Err(anyhow!(
                "primitive index {primitive} out of bounds, count was {}",
                gltf_mesh.primitives.len()
            ));
        };

        Ok(Some(primitive.mesh.clone()))
    }
}
