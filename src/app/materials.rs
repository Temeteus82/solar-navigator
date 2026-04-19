use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{
    AsBindGroup, Face, RenderPipelineDescriptor, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(super) struct PlanetAtmosphereMaterial {
    #[uniform(0)]
    pub(super) tint: LinearRgba,
    #[uniform(1)]
    pub(super) params: Vec4,
}

impl Material for PlanetAtmosphereMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/planet_atmosphere.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = Some(Face::Front);

        if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
            depth_stencil.depth_write_enabled = false;
        }

        Ok(())
    }
}
