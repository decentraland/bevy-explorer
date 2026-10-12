//! The particle quads are a bevy `Material` so the transparent pass, blend modes and view
//! bindings come for free. The vertex shader pulls particles from the storage buffer the
//! simulation writes.

use bevy::{
    pbr::{Material, MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::{
        mesh::MeshVertexBufferLayoutRef,
        render_resource::{
            AsBindGroup, RenderPipelineDescriptor, ShaderRef, SpecializedMeshPipelineError,
        },
        storage::ShaderStorageBuffer,
    },
};

use super::config::{features, shader_defs, RenderParams};

#[derive(Asset, TypePath, AsBindGroup, Clone)]
#[bind_group_data(ParticleMaterialKey)]
pub struct ParticleMaterial {
    #[uniform(0)]
    pub params: RenderParams,
    #[storage(1, read_only)]
    pub particles: Handle<ShaderStorageBuffer>,
    #[texture(2)]
    #[sampler(3)]
    pub texture: Option<Handle<Image>>,
    pub features: u32,
    pub alpha_mode: AlphaMode,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
pub struct ParticleMaterialKey {
    features: u32,
}

impl From<&ParticleMaterial> for ParticleMaterialKey {
    fn from(material: &ParticleMaterial) -> Self {
        Self {
            features: material.features & features::RENDER_MASK,
        }
    }
}

impl Material for ParticleMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Path("embedded://particle_system/shaders/render.wgsl".into())
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path("embedded://particle_system/shaders/render.wgsl".into())
    }

    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    fn specialize(
        _pipeline: &MaterialPipeline<Self>,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let defs = shader_defs(key.bind_group_data.features);
        descriptor.vertex.shader_defs.extend(defs.iter().cloned());
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader_defs.extend(defs);
        }
        descriptor.vertex.buffers = vec![layout
            .0
            .get_layout(&[Mesh::ATTRIBUTE_POSITION.at_shader_location(0)])?];
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}
