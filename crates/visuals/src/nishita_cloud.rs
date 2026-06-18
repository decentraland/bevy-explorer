// temporary for ShaderType macro, remove in future
#![allow(dead_code)]

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
    /// normalized time of day (0.0 = midnight, 0.5 = noon), drives the
    /// measured-sky lut
    pub day: f32,
    /// sky color cycle lut (see build_sky_lut)
    pub sky_lut: Handle<Image>,
    /// godot-explorer painted cloud cubemap, 6x1 face strip.
    /// R = cloud body, G = silhouette mask, B = sun highlight
    pub clouds_strip: Handle<Image>,
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
    pub day: f32,
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
            day: value.day,
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
            day: 0.5,
            sky_lut: Default::default(),
            clouds_strip: Default::default(),
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
        let sky_lut = &images.get(&self.sky_lut).unwrap_or(&fallback_image.d2);
        let clouds = &images.get(&self.clouds_strip).unwrap_or(&fallback_image.d2);
        let bind_group = render_device.create_bind_group(
            None,
            layout,
            &BindGroupEntries::sequential((
                binding,
                &image.texture_view,
                &image.sampler,
                &sky_lut.texture_view,
                &sky_lut.sampler,
                &clouds.texture_view,
                &clouds.sampler,
            )),
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

        let handle = asset_server.load("embedded://shaders/nishita_cloud.wgsl");
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
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(bevy::render::render_resource::SamplerBindingType::Filtering),
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
        .map(|pixel| pixel as f32)
        .flat_map(f32::to_le_bytes)
        .collect();
    let mut image = Image::new(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        bevy::render::render_resource::TextureFormat::R32Float,
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

/// Live tuning applied on top of the measured sky color gradients.
/// `zenith`/`horizon`/`nadir` are per-zone RGB gains; `master` scales every
/// zone together; `sat` boosts color vibrancy of the SKY ONLY (pushes each
/// color away from its grey/luminance) to counter the washed-out look without
/// touching the rest of the scene. Driven by the /skyzenith /skyhorizon
/// /skynadir /skygain /skysat console commands; mutating any of these rebuilds
/// the sky LUT (see `rebuild_sky_lut`).
#[derive(Resource, Clone)]
pub struct SkyColorTuning {
    pub zenith: Vec3,
    pub horizon: Vec3,
    pub nadir: Vec3,
    pub master: f32,
    /// sky-only saturation: 1.0 = measured, >1 more vivid, <1 greyer
    pub sat: f32,
    /// cloud HORIZON-BAND mapping (see skybox.wgsl sample_cloud_panorama).
    /// `cloud_horizon` = texture row that sits on the horizon (~0.47, the
    /// cloud/ground boundary). `cloud_vscale` here means the band TOP: the
    /// elevation (ray.y, 0 = horizon .. 1 = straight up) where the cloud band
    /// ends — clear sky above it. keep it low (≈0.5) so clouds stay near the
    /// horizon and never reach the stretch-prone zenith.
    pub cloud_horizon: f32,
    pub cloud_vscale: f32,
}

impl Default for SkyColorTuning {
    fn default() -> Self {
        Self {
            zenith: Vec3::ONE,
            horizon: Vec3::ONE,
            nadir: Vec3::ONE,
            master: 1.0,
            // the measured Unity screenshots are paler than the live client;
            // this boost restores the vivid blue (user-chosen baseline). the
            // day/night ease-off in build_sky_lut keeps night from going neon.
            sat: 4.5,
            // horizon band: horizon at the texture's cloud/ground line, band
            // ends at ray.y = 0.5 (~30 deg up) with clear sky above — no spike.
            cloud_horizon: 0.47,
            cloud_vscale: 0.5,
        }
    }
}

/// Handle to the live sky color-cycle LUT, kept so console commands can
/// rebuild its pixels in place when tuning changes.
#[derive(Resource)]
pub struct SkyLut(pub Handle<Image>);

/// Build the sky color-cycle lookup texture from the measured Unity sky
/// gradients. x = time of day (0 = midnight .. 1). Rows:
///   0 zenith, 1 horizon, 2 nadir, 3 sun (HDR), 4 rim (HDR),
///   5 cloud color (HDR), 6 cloud highlight, 7 celestial params, 8 moon tint,
///   9 cloud mapping params (r = cloud_horizon, g = cloud_vscale).
/// Rgba32Float, linear values; the zenith/horizon/nadir rows are scaled by the
/// live `tuning` gains.
pub fn build_sky_lut(tuning: &SkyColorTuning) -> Image {
    use common::godot_sky as g;

    const W: usize = 256;
    // rows fed directly from gradients, in LUT row order
    let rows: [&g::Gradient; 7] = [
        &g::ZENITH,
        &g::HORIZON,
        &g::NADIR,
        &g::SUN,
        &g::RIM,
        &g::CLOUDS,
        &g::CLOUD_HIGHLIGHTS,
    ];
    // per-row RGB gains; sun/rim/cloud/highlight (rows 3..) stay unscaled.
    let gains = [
        tuning.zenith * tuning.master,
        tuning.horizon * tuning.master,
        tuning.nadir * tuning.master,
    ];
    let extra_rows = 3; // celestial params + moon tint + cloud params appended below
    let mut data: Vec<f32> = Vec::with_capacity(W * (rows.len() + extra_rows) * 4);
    // rec709 luma weights, for the sky-only saturation push
    let luma = Vec3::new(0.2126, 0.7152, 0.0722);
    for (row, grad) in rows.iter().enumerate() {
        let gain = gains.get(row).copied().unwrap_or(Vec3::ONE);
        // only the sky-color rows (0..3) get the saturation boost
        let sat = if row < 3 { tuning.sat } else { 1.0 };
        for x in 0..W {
            let mut c = grad.sample(x as f32 / W as f32) * gain;
            if sat != 1.0 {
                let l = c.dot(luma);
                // ease the saturation boost off on dark (night) colors so they
                // don't blow out to neon — daytime (bright) gets the full boost,
                // night (dark, already-rich) is left close to its measured value.
                let t = ((l - 0.12) / (0.5 - 0.12)).clamp(0.0, 1.0);
                let bright = t * t * (3.0 - 2.0 * t); // smoothstep
                let amt = 1.0 + (sat - 1.0) * bright;
                c = (Vec3::splat(l) + (c - Vec3::splat(l)) * amt).max(Vec3::ZERO);
            }
            data.extend_from_slice(&[c.x, c.y, c.z, 1.0]);
        }
    }
    // celestial params — r = sun opacity, g = sun size, b = moon bite size
    for x in 0..W {
        let t = x as f32 / W as f32;
        data.extend_from_slice(&[
            g::SUN_OPACITY.sample(t),
            g::SUN_SIZE.sample(t),
            g::MOON_MASK_SIZE.sample(t),
            1.0,
        ]);
    }
    // moon tint cycle
    for x in 0..W {
        let c = g::MOON.sample(x as f32 / W as f32);
        data.extend_from_slice(&[c.x, c.y, c.z, 1.0]);
    }
    // cloud mapping params (constant across time): r = cloud_horizon, g = cloud_vscale
    for _x in 0..W {
        data.extend_from_slice(&[tuning.cloud_horizon, tuning.cloud_vscale, 0.0, 1.0]);
    }

    let mut image = Image::new(
        Extent3d {
            width: W as u32,
            height: (rows.len() + extra_rows) as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data.into_iter().flat_map(f32::to_le_bytes).collect(),
        bevy::render::render_resource::TextureFormat::Rgba32Float,
        RenderAssetUsages::all(),
    );

    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        label: Some("sky_color_cycles".to_owned()),
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::ClampToEdge,
        ..ImageSamplerDescriptor::linear()
    });

    image
}

/// Load the godot-explorer painted cloud cubemap (6x1 face strip png).
/// Channels are data (R body, G mask, B highlight), not color — no sRGB.
pub fn load_clouds_strip() -> Image {
    let bytes = include_bytes!("assets/horizon_clouds.png");
    let mut image = Image::from_buffer(
        bytes,
        bevy::image::ImageType::Extension("png"),
        bevy::image::CompressedImageFormats::NONE,
        false,
        ImageSampler::Descriptor(ImageSamplerDescriptor {
            label: Some("horizon_clouds".to_owned()),
            address_mode_u: ImageAddressMode::ClampToEdge,
            address_mode_v: ImageAddressMode::ClampToEdge,
            ..ImageSamplerDescriptor::linear()
        }),
        RenderAssetUsages::all(),
    )
    .expect("invalid horizon_clouds.png");
    image.texture_descriptor.label = Some("horizon_clouds");
    image
}

/// load an embedded png as a texture (data channels, no sRGB)
fn load_embedded_png(bytes: &[u8], label: &'static str, repeat_u: bool) -> Image {
    let mut image = Image::from_buffer(
        bytes,
        bevy::image::ImageType::Extension("png"),
        bevy::image::CompressedImageFormats::NONE,
        false,
        ImageSampler::Descriptor(ImageSamplerDescriptor {
            label: Some(label.to_owned()),
            address_mode_u: if repeat_u {
                ImageAddressMode::Repeat
            } else {
                ImageAddressMode::ClampToEdge
            },
            address_mode_v: ImageAddressMode::ClampToEdge,
            ..ImageSamplerDescriptor::linear()
        }),
        RenderAssetUsages::all(),
    )
    .expect("invalid embedded png");
    image.texture_descriptor.label = Some(label);
    image
}

/// unity explorer's sky textures (StylizedSkybox/Textures)
pub fn load_unity_clouds() -> Image {
    load_embedded_png(
        include_bytes!("assets/unity_clouds.png"),
        "unity_clouds",
        true,
    )
}
pub fn load_unity_sun() -> Image {
    load_embedded_png(include_bytes!("assets/unity_sun.png"), "unity_sun", false)
}
pub fn load_unity_moon() -> Image {
    load_embedded_png(include_bytes!("assets/unity_moon.png"), "unity_moon", false)
}
pub fn load_unity_stars() -> Image {
    let mut img = load_embedded_png(
        include_bytes!("assets/unity_stars.png"),
        "unity_stars",
        true,
    );
    img.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        label: Some("unity_stars".to_owned()),
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    img
}
