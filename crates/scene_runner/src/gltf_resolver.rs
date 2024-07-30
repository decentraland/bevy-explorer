use anyhow::anyhow;
use bevy::{
    ecs::system::SystemParam,
    gltf::{Gltf, GltfMesh},
    prelude::*,
    utils::HashMap,
};
use ipfs::IpfsAssetServer;

#[derive(SystemParam)]
pub struct GltfResolver<'w, 's> {
    prev_pending_gltfs: Local<'s, HashMap<String, Handle<Gltf>>>,
    pending_gltfs: Local<'s, HashMap<String, Handle<Gltf>>>,
    ipfas: IpfsAssetServer<'w, 's>,
    gltfs: Res<'w, Assets<Gltf>>,
}

impl<'w, 's> GltfResolver<'w, 's> {
    pub fn begin_frame(&mut self) {
        *self.prev_pending_gltfs = std::mem::take(&mut *self.pending_gltfs);
    }

    fn resolve_gltf(
        &mut self,
        gltf_src: &str,
        scene_hash: &str,
    ) -> Result<Option<Handle<Gltf>>, anyhow::Error> {
        let lookup = format!("{gltf_src}##{scene_hash}");
        let h_gltf = self
            .prev_pending_gltfs
            .remove(&lookup)
            .unwrap_or_else(|| self.ipfas.load_content_file(gltf_src, scene_hash).unwrap());
        match self.ipfas.asset_server().load_state(h_gltf.id()) {
            bevy::asset::LoadState::Loading => {
                self.pending_gltfs.insert(lookup, h_gltf.clone());
                Ok(None)
            }
            bevy::asset::LoadState::Loaded => Ok(Some(h_gltf)),
            bevy::asset::LoadState::NotLoaded | bevy::asset::LoadState::Failed => {
                warn!("failed to load gltf for mesh");
                Err(anyhow!("failed to load gltf for mesh"))
            }
        }
    }
}

#[derive(SystemParam)]
pub struct GltfMeshResolver<'w, 's> {
    gltf_resolver: GltfResolver<'w, 's>,
    gltf_meshes: Res<'w, Assets<GltfMesh>>,
}

impl<'w, 's> GltfMeshResolver<'w, 's> {
    pub fn begin_frame(&mut self) {
        self.gltf_resolver.begin_frame();
    }

    pub fn resolve_mesh(
        &mut self,
        gltf_src: &str,
        scene_hash: &str,
        name: &str,
    ) -> Result<Option<Handle<Mesh>>, anyhow::Error> {
        let Some(gltf) = self
            .gltf_resolver
            .resolve_gltf(gltf_src, scene_hash)?
            .and_then(|h_gltf| self.gltf_resolver.gltfs.get(h_gltf.id()))
        else {
            return Ok(None);
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

#[derive(SystemParam)]
pub struct GltfMaterialResolver<'w, 's> {
    gltf_resolver: GltfResolver<'w, 's>,
    std_materials: Res<'w, Assets<StandardMaterial>>,
}

impl<'w, 's> GltfMaterialResolver<'w, 's> {
    pub fn begin_frame(&mut self) {
        self.gltf_resolver.begin_frame();
    }

    pub fn resolve_material(
        &mut self,
        gltf_src: &str,
        scene_hash: &str,
        name: &str,
    ) -> Result<Option<&StandardMaterial>, anyhow::Error> {
        let Some(gltf) = self
            .gltf_resolver
            .resolve_gltf(gltf_src, scene_hash)?
            .and_then(|h_gltf| self.gltf_resolver.gltfs.get(h_gltf.id()))
        else {
            return Ok(None);
        };

        let h_gltf_material = match gltf.named_materials.get(name) {
            Some(h_gm) => h_gm,
            None => {
                let Some(h_gm) = name
                    .strip_prefix("Material")
                    .and_then(|ix_str| ix_str.parse::<usize>().ok())
                    .and_then(|ix| gltf.materials.get(ix))
                else {
                    return Err(anyhow!("mesh {name:?} not found in gltf {gltf_src}"));
                };

                h_gm
            }
        };
        Ok(self.std_materials.get(h_gltf_material))
    }
}
