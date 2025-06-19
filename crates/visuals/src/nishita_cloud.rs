use bevy::{
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    prelude::*,
    render::{
        render_asset::{RenderAssetUsages, RenderAssets},
        render_resource::{
            binding_types::{sampler, texture_2d},
            encase, BindGroup, BindGroupEntries, BindGroupLayout, BindGroupLayoutEntries,
            BindGroupLayoutEntry, BindingType, BufferBindingType, ComputePipelineDescriptor,
            Extent3d, OwnedBindingResource, PipelineCache, ShaderStages, ShaderType,
            TextureDimension, TextureSampleType,
        },
        renderer::RenderDevice,
        texture::{FallbackImage, GpuImage},
        RenderApp,
    },
};
use bevy_atmosphere::{model::AtmosphereModelMetadata, pipeline::AtmosphereImageBindGroupLayout};
use chrono::Datelike;
use noise::{
    utils::{NoiseMapBuilder, PlaneMapBuilder},
    Perlin,
};

/// The Nishita sky model.
///
/// An atmospheric model that uses Rayleigh and Mie scattering to simulate a realistic sky.
#[derive(Reflect, Debug, Clone)]
pub struct NishitaCloud {
    /// Ray Origin (Default: `(0.0, 6372e3, 0.0)`).
    ///
    /// Controls orientation of the sky and height of the sun.
    /// It can be thought of as the up-axis and values should be somewhere between planet radius and atmosphere radius (with a bias towards lower values).
    /// When used with `planet_radius` and `atmosphere_radius`, it can be used to change sky brightness and falloff
    pub ray_origin: Vec3,
    /// Sun Position (Default: `(1.0, 1.0, 1.0)`).
    ///
    /// Controls position of the sun in the sky.
    /// Scale doesn't matter, as it will be normalized.
    pub sun_position: Vec3,
    /// Sun Intensity (Default: `22.0`).
    ///
    /// Controls how intense the sun's brightness is.
    pub sun_intensity: f32,
    /// Planet Radius (Default: `6371e3`).
    ///
    /// Controls the radius of the planet.
    /// Heavily interdependent with `atmosphere_radius`
    pub planet_radius: f32,
    /// Atmosphere Radius (Default: `6471e3`).
    ///
    /// Controls the radius of the atmosphere.
    /// Heavily interdependent with `planet_radius`.
    pub atmosphere_radius: f32,
    /// Rayleigh Scattering Coefficient (Default: `(5.5e-6, 13.0e-6, 22.4e-6)`).
    ///
    /// Strongly influences the color of the sky.
    pub rayleigh_coefficient: Vec3,
    /// Rayleigh Scattering Scale Height (Default: `8e3`).
    ///
    /// Controls the amount of Rayleigh scattering.
    pub rayleigh_scale_height: f32,
    /// Mie Scattering Coefficient (Default: `21e-6`).
    ///
    /// Strongly influences the color of the horizon.
    pub mie_coefficient: f32,
    /// Mie Scattering Scale Height (Default: `1.2e3`).
    ///
    /// Controls the amount of Mie scattering.
    pub mie_scale_height: f32,
    /// Mie Scattering Preferred Direction (Default: `0.758`).
    ///
    /// Controls the general direction of Mie scattering.
    pub mie_direction: f32,
    pub noise_texture: Handle<Image>,
    pub time: f32,
    pub cloudy: f32,
    pub tick: u32,
    pub sun_color: Vec3,
    pub dir_light_intensity: f32,
}

#[derive(ShaderType)]
pub struct NishitaCloudUniform {
    pub ray_origin: Vec3,
    pub sun_position: Vec3,
    pub sun_intensity: f32,
    pub planet_radius: f32,
    pub atmosphere_radius: f32,
    pub rayleigh_coefficient: Vec3,
    pub rayleigh_scale_height: f32,
    pub mie_coefficient: f32,
    pub mie_scale_height: f32,
    pub mie_direction: f32,
    pub time: f32,
    pub cloudy: f32,
    pub tick: u32,
    pub sun_color: Vec3,
    pub dir_light_intensity: f32,
}

impl From<&NishitaCloud> for NishitaCloudUniform {
    fn from(value: &NishitaCloud) -> Self {
        Self {
            ray_origin: value.ray_origin,
            sun_position: value.sun_position,
            sun_intensity: value.sun_intensity,
            planet_radius: value.planet_radius,
            atmosphere_radius: value.atmosphere_radius,
            rayleigh_coefficient: value.rayleigh_coefficient,
            rayleigh_scale_height: value.rayleigh_scale_height,
            mie_coefficient: value.mie_coefficient,
            mie_scale_height: value.mie_scale_height,
            mie_direction: value.mie_direction,
            time: value.time,
            cloudy: value.cloudy,
            tick: value.tick,
            sun_color: value.sun_color,
            dir_light_intensity: value.dir_light_intensity,
        }
    }
}

impl Default for NishitaCloud {
    fn default() -> Self {
        Self {
            ray_origin: Vec3::new(0.0, 6372e3, 0.0),
            sun_position: Vec3::new(1.0, 1.0, 1.0),
            sun_intensity: 22.0,
            planet_radius: 6371e3,
            atmosphere_radius: 6471e3,
            rayleigh_coefficient: Vec3::new(5.5e-6, 13.0e-6, 22.4e-6),
            rayleigh_scale_height: 8e3,
            mie_coefficient: 21e-6,
            mie_scale_height: 1.2e3,
            mie_direction: 0.758,
            noise_texture: Default::default(),
            time: 0.0,
            cloudy: 0.25,
            tick: 0,
            sun_color: Vec3::new(1.0, 1.0, 0.7),
            dir_light_intensity: 10000.0,
        }
    }
}

impl From<&NishitaCloud> for NishitaCloud {
    fn from(nishita: &NishitaCloud) -> Self {
        nishita.clone()
    }
}

// Recursive expansion of Atmospheric macro
// =========================================

impl bevy_atmosphere::model::Atmospheric for NishitaCloud {
    fn as_bind_group(
        &self,
        layout: &BindGroupLayout,
        render_device: &RenderDevice,
        images: &RenderAssets<GpuImage>,
        fallback_image: &FallbackImage,
    ) -> BindGroup {
        let uniform: NishitaCloudUniform = self.into();
        let mut encase_buffer = encase::UniformBuffer::new(Vec::default());
        encase_buffer.write(&uniform).unwrap();
        let buffer = render_device.create_buffer_with_data(
            &bevy::render::render_resource::BufferInitDescriptor {
                label: None,
                usage: bevy::render::render_resource::BufferUsages::COPY_DST
                    | bevy::render::render_resource::BufferUsages::UNIFORM,
                contents: encase_buffer.as_ref(),
            },
        );
        let owned = OwnedBindingResource::Buffer(buffer);
        let binding = owned.get_binding();
        let image = &images
            .get(&self.noise_texture)
            .unwrap_or(&fallback_image.d2);
        let bind_group = render_device.create_bind_group(
            None,
            layout,
            &BindGroupEntries::sequential((binding, &image.texture_view, &image.sampler)),
        );
        bind_group
    }
    fn clone_dynamic(&self) -> Box<dyn bevy_atmosphere::model::Atmospheric> {
        Box::new((*self).clone())
    }
    fn as_reflect(&self) -> &dyn Reflect {
        self
    }
    fn as_reflect_mut(&mut self) -> &mut dyn Reflect {
        self
    }
}

impl bevy_atmosphere::model::RegisterAtmosphereModel for NishitaCloud {
    fn register(app: &mut App) {
        use std::any::TypeId;
        use std::borrow::Cow;

        app.register_type::<Self>();
        let asset_server = app.world().resource::<AssetServer>();

        let handle = asset_server.load("shaders/nishita_cloud.wgsl");
        // let handle = {
        //     let handle: Handle<Shader> = Handle::weak_from_u128(2735577752830981861u64 as u128);
        //     let internal_handle = handle.clone();

        //     bevy::asset::load_internal_asset!(
        //         app,
        //         internal_handle,
        //         concat!(env!("CARGO_MANIFEST_DIR"), "/src/", "nishita_cloud.wgsl"),
        //         Shader::from_wgsl
        //     );
        //     handle
        // };

        let render_app = app.sub_app_mut(RenderApp);
        let render_device = render_app.world().resource::<RenderDevice>();
        let AtmosphereImageBindGroupLayout(image_bind_group_layout) = render_app
            .world()
            .resource::<AtmosphereImageBindGroupLayout>()
            .clone();
        let bind_group_layout = Self::bind_group_layout(render_device);
        let pipeline_cache = render_app.world_mut().resource_mut::<PipelineCache>();
        let pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some(Cow::from("bevy_atmosphere_compute_pipeline")),
            layout: vec![bind_group_layout.clone(), image_bind_group_layout],
            push_constant_ranges: vec![],
            shader: handle,
            shader_defs: vec![],
            entry_point: Cow::from("main"),
            zero_initialize_workgroup_memory: false,
        });
        let id = TypeId::of::<Self>();
        let data = AtmosphereModelMetadata {
            id,
            bind_group_layout,
            pipeline,
        };
        let type_registry = app
            .world_mut()
            .resource_mut::<bevy::ecs::reflect::AppTypeRegistry>();
        {
            let mut type_registry = type_registry.write();
            let registration = type_registry
                .get_mut(std::any::TypeId::of::<Self>())
                .expect("Type not registered");
            registration.insert(data);
        }
    }
    fn bind_group_layout(render_device: &bevy::render::renderer::RenderDevice) -> BindGroupLayout {
        render_device.create_bind_group_layout(
            "atmospheric_bind_group_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::all(),
                (
                    BindGroupLayoutEntry {
                        binding: 0u32,
                        visibility: ShaderStages::all(),
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: Some(<NishitaCloudUniform as ShaderType>::min_size()),
                        },
                        count: None,
                    },
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(bevy::render::render_resource::SamplerBindingType::Filtering),
                ),
            ),
        )
    }
}

pub fn init_noise(size: usize) -> Image {
    // let fbm = Fbm::<Perlin>::new(170);
    let datetime: chrono::DateTime<chrono::Utc> = chrono::DateTime::from_timestamp_millis(
        web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64,
    )
    .unwrap();
    let seed = datetime.date_naive().day();
    let noise_pixels = PlaneMapBuilder::new(Perlin::new(seed))
        .set_size(size, size)
        .set_is_seamless(true)
        .set_x_bounds(-5.0, 5.0)
        .set_y_bounds(-5.0, 5.0)
        .build(); // range[-0.5, 0.5]
    let data: Vec<_> = noise_pixels
        .into_iter()
        .map(|pixel| (pixel * 65535.0).round() as i16)
        .flat_map(i16::to_le_bytes)
        .collect();
    let mut image = Image::new(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        bevy::render::render_resource::TextureFormat::R16Snorm,
        RenderAssetUsages::all(),
    );

    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        label: Some("noise".to_owned()),
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        address_mode_w: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });

    image
}
