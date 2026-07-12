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
            depth_stencil.depth_write_enabled = Some(false);
        }

        Ok(())
    }
}

/// Custom planetary-ring material: samples a radial color/alpha strip and
/// computes its own lighting in WGSL — Lambert + sigmoid terminator,
/// cylindrical eclipse from the parent planet, plus forward / back scatter
/// terms that capture the way real ring particles respond to the sun.
///
/// Coordinate convention: the sun is a point light fixed at the world
/// origin (see `app::setup`). `planet_position.xyz` is the world-space
/// position of the parent planet's centre, updated each frame from
/// `BodyRuntime::positions` by `simulation::sync_ring_material_uniforms`.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub(super) struct PlanetRingMaterial {
    #[uniform(0)]
    pub(super) tint: LinearRgba,
    /// x = inner_radius (scene units, unused in shader but kept for parity)
    /// y = outer_radius (scene units, ditto)
    /// z = planet_radius (scene units — drives the umbra width)
    /// w = ring_brightness (overall multiplier on the lit term)
    #[uniform(1)]
    pub(super) params: Vec4,
    /// x = forward scatter strength (sun behind ring, viewer in front)
    /// y = back scatter / opposition surge (sun behind viewer)
    /// z = specular strength (icy particle highlight)
    /// w = ambient floor (so the unlit side isn't pitch-black)
    #[uniform(2)]
    pub(super) lighting: Vec4,
    /// xyz = parent planet world-space position; w unused.
    #[uniform(3)]
    pub(super) planet_position: Vec4,
    #[texture(4)]
    #[sampler(5)]
    pub(super) color_texture: Handle<Image>,
}

impl Material for PlanetRingMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/planet_ring.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
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
        // Render both faces of the disc so the ring is visible from above and below.
        descriptor.primitive.cull_mode = None;

        if let Some(depth_stencil) = descriptor.depth_stencil.as_mut() {
            depth_stencil.depth_write_enabled = Some(false);
        }

        Ok(())
    }
}
