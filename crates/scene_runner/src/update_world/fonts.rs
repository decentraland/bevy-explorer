//! Scene-selectable fonts.
//!
//! Text components carry an optional `font_src`. It is resolved, in order, as a file in the
//! scene's content map (a bare url also works via the content-file loader's url fallback),
//! then as a Google Fonts family name looked up through the Fontsource api, which serves
//! static per-weight ttfs from jsdelivr with no api key. If neither works the text renders
//! with the built-in family named by the `font` enum.
//!
//! Only the `latin` subset of a Fontsource family is loaded for now; other characters fall
//! through to the built-in fonts via glyph fallback.
//!
//! Text is handed reserved handles which are filled once the source arrives, so a text
//! entity never has to be re-evaluated when a font lands or fails: bevy's text systems wait
//! for the handle to load, and on failure the handle is filled with the fallback font's data.
//!
//! Each family also reports [`FamilyMetrics`]: a default line height derived from the font's
//! own vertical metrics, and the vertical padding a text block needs so glyph ink reaching
//! beyond the line box (deep descenders, tall swashes) is not clipped.

use std::{path::PathBuf, time::Duration};

use anyhow::anyhow;
use bevy::{
    asset::LoadState,
    ecs::system::SystemParam,
    platform::collections::HashMap,
    prelude::*,
    tasks::{IoTaskPool, Task},
    text::Font,
};
use common::util::TaskCompat;
use dcl_component::proto_components::sdk::components::common::Font as ProtoFont;
use ipfs::{ipfs_path::IpfsPath, IpfsAssetServer};
use serde::Deserialize;
use ui_core::{user_font, FontName, WeightName};

use crate::renderer_context::RendererSceneContext;

pub struct SceneFontsPlugin;

impl Plugin for SceneFontsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SceneFonts>();
        app.add_systems(Update, update_scene_fonts);
    }
}

/// The font family a text component resolved to; created by [`SceneFontServer::family`] and
/// consumed by [`SceneFontServer::face`].
#[derive(Clone, Debug)]
pub enum TextFontFamily {
    Builtin(FontName),
    Scene {
        key: FamilyKey,
        scene_hash: String,
        fallback: FontName,
    },
}

/// Families are per scene entity, so a reloaded scene (same hash, new entity) starts afresh
/// and a family dies with its scene.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FamilyKey {
    scene: Entity,
    src: String,
}

#[derive(Resource, Default)]
pub struct SceneFonts {
    families: HashMap<FamilyKey, SceneFontFamily>,
    builtin_metrics: HashMap<FontName, FamilyMetrics>,
    /// Multiplier from a font's own line metric to its default line height, chosen so the
    /// built-in fonts land on [`REFERENCE_LINE_HEIGHT`].
    line_height_scale: Option<f32>,
}

struct SceneFontFamily {
    fallback: FontName,
    stage: Stage,
    slots: HashMap<WeightName, Slot>,
    /// Computed once every slot has data.
    metrics: Option<FamilyMetrics>,
}

/// Line height (in ems) of the built-in fonts, calibrated against the reference renderer.
const REFERENCE_LINE_HEIGHT: f32 = 1.2;

/// Vertical metrics of a family, in ems.
#[derive(Clone, Copy, Debug)]
pub struct FamilyMetrics {
    /// Default line height: the font's ascent + descent + line gap, scaled so the built-in
    /// fonts land on [`REFERENCE_LINE_HEIGHT`], and floored there.
    pub line_height: f32,
    /// Vertical padding a text block needs above and below so ink outside the line box is
    /// not clipped.
    pub padding: f32,
}

/// Vertical metrics of one face, in ems.
#[derive(Clone, Copy)]
struct FaceMetrics {
    ascent: f32,
    descent: f32,
    line_gap: f32,
    /// Ink bounds (the os/2 "win" metrics), never less than the ascent / descent.
    ink_ascent: f32,
    ink_descent: f32,
}

impl FaceMetrics {
    /// Uses the same ascent / descent tables as cosmic-text so the padding matches how the
    /// glyphs are actually placed within the line box.
    fn parse(font: &Font) -> Option<Self> {
        let face = ttf_parser::Face::parse(&font.data, 0).ok()?;
        let em = face.units_per_em() as f32;
        let ascent = face.ascender() as f32 / em;
        let descent = -face.descender() as f32 / em;
        let (ink_ascent, ink_descent) = face
            .tables()
            .os2
            .map(|os2| {
                (
                    os2.windows_ascender() as f32 / em,
                    os2.windows_descender() as f32 / em,
                )
            })
            .unwrap_or((ascent, descent));
        Some(Self {
            ascent,
            descent,
            line_gap: face.line_gap() as f32 / em,
            ink_ascent: ink_ascent.max(ascent),
            ink_descent: ink_descent.max(descent),
        })
    }

    fn line_metric(&self) -> f32 {
        self.ascent + self.descent + self.line_gap
    }
}

impl FamilyMetrics {
    fn from_faces(faces: &[FaceMetrics], line_height_scale: f32) -> Self {
        let metric = faces
            .iter()
            .map(FaceMetrics::line_metric)
            .fold(0.0, f32::max);
        let line_height = (metric * line_height_scale).max(REFERENCE_LINE_HEIGHT);
        // cosmic-text centres a line's ascent + descent within the line box, so the ink
        // overflow is whatever the win bounds add beyond that, less the box's slack
        let padding = faces
            .iter()
            .map(|face| {
                let slack = (line_height - (face.ascent + face.descent)) / 2.0;
                (face.ink_ascent - face.ascent - slack).max(face.ink_descent - face.descent - slack)
            })
            .fold(0.0, f32::max);
        Self {
            line_height,
            padding,
        }
    }
}

enum Stage {
    /// Loading `src` as a scene content file (or bare url).
    ContentFile(Handle<Font>),
    /// One face serves every weight (content file / url).
    Single(Font),
    /// Resolving `src` as a font family through Fontsource.
    Lookup(Task<Result<FontsourceFamily, anyhow::Error>>),
    Fontsource(FontsourceFamily),
    /// Nothing resolved; slots are filled with the built-in fallback.
    Failed,
}

/// The handle handed to text for one weight, and where its data comes from.
struct Slot {
    handle: Handle<Font>,
    source: SlotSource,
}

enum SlotSource {
    /// Waiting for the family stage to settle.
    Pending,
    /// Loading the file chosen for this weight.
    Loading(Handle<Font>),
    /// Uses the built-in fallback family.
    Fallback,
    Done,
}

#[derive(SystemParam)]
pub struct SceneFontServer<'w, 's> {
    families: ResMut<'w, SceneFonts>,
    assets: ResMut<'w, Assets<Font>>,
    ipfas: IpfsAssetServer<'w, 's>,
}

impl SceneFontServer<'_, '_> {
    pub fn assets(&self) -> &Assets<Font> {
        &self.assets
    }

    pub fn family(
        &self,
        scene: Entity,
        scene_hash: &str,
        font: ProtoFont,
        font_src: Option<&str>,
    ) -> TextFontFamily {
        let fallback = match font {
            ProtoFont::FSansSerif => FontName::Sans,
            ProtoFont::FSerif => FontName::Serif,
            ProtoFont::FMonospace => FontName::Mono,
        };
        match font_src.map(str::trim).filter(|src| !src.is_empty()) {
            None => TextFontFamily::Builtin(fallback),
            Some(src) => TextFontFamily::Scene {
                key: FamilyKey {
                    scene,
                    src: src.to_owned(),
                },
                scene_hash: scene_hash.to_owned(),
                fallback,
            },
        }
    }

    /// Whether every requested face of the family has data (real or fallback).
    pub fn family_ready(&self, family: &TextFontFamily) -> bool {
        let TextFontFamily::Scene { key, .. } = family else {
            return true;
        };
        self.families
            .families
            .get(key)
            .into_iter()
            .flat_map(|family| family.slots.values())
            .all(|slot| self.assets.contains(slot.handle.id()))
    }

    /// Metrics of a family; the fallback family's until every slot has data.
    pub fn metrics(&mut self, family: &TextFontFamily) -> FamilyMetrics {
        let (key, fallback) = match family {
            TextFontFamily::Builtin(name) => return self.builtin_metrics(*name),
            TextFontFamily::Scene { key, fallback, .. } => (key, *fallback),
        };
        if let Some(metrics) = self.families.families.get(key).and_then(|f| f.metrics) {
            return metrics;
        }
        if !self.family_ready(family) {
            return self.builtin_metrics(fallback);
        }
        let line_height_scale = self.line_height_scale();
        let Some(family) = self.families.families.get_mut(key) else {
            return self.builtin_metrics(fallback);
        };
        let faces = family
            .slots
            .values()
            .filter_map(|slot| self.assets.get(slot.handle.id()))
            .filter_map(FaceMetrics::parse)
            .collect::<Vec<_>>();
        let metrics = FamilyMetrics::from_faces(&faces, line_height_scale);
        family.metrics = Some(metrics);
        metrics
    }

    fn builtin_faces(&self, name: FontName) -> Vec<FaceMetrics> {
        [
            WeightName::Regular,
            WeightName::Bold,
            WeightName::Italic,
            WeightName::BoldItalic,
        ]
        .into_iter()
        .filter_map(|weight| self.assets.get(user_font(name, weight).id()))
        .filter_map(FaceMetrics::parse)
        .collect()
    }

    fn line_height_scale(&mut self) -> f32 {
        if let Some(scale) = self.families.line_height_scale {
            return scale;
        }
        let metric = self
            .builtin_faces(FontName::Sans)
            .iter()
            .map(FaceMetrics::line_metric)
            .fold(0.0, f32::max);
        let scale = if metric > 0.0 {
            REFERENCE_LINE_HEIGHT / metric
        } else {
            1.0
        };
        self.families.line_height_scale = Some(scale);
        scale
    }

    fn builtin_metrics(&mut self, name: FontName) -> FamilyMetrics {
        if let Some(metrics) = self.families.builtin_metrics.get(&name) {
            return *metrics;
        }
        let line_height_scale = self.line_height_scale();
        let metrics = FamilyMetrics::from_faces(&self.builtin_faces(name), line_height_scale);
        self.families.builtin_metrics.insert(name, metrics);
        metrics
    }

    pub fn face(&mut self, family: &TextFontFamily, weight: WeightName) -> Handle<Font> {
        let (key, scene_hash, fallback) = match family {
            TextFontFamily::Builtin(name) => return user_font(*name, weight),
            TextFontFamily::Scene {
                key,
                scene_hash,
                fallback,
            } => (key, scene_hash, *fallback),
        };

        if !self.families.families.contains_key(key) {
            let stage = match self.ipfas.load_content_file::<Font>(&key.src, scene_hash) {
                Ok(handle) => Stage::ContentFile(handle),
                Err(e) => {
                    warn!("font `{}` failed to load: {e}", key.src);
                    Stage::Failed
                }
            };
            self.families.families.insert(
                key.clone(),
                SceneFontFamily {
                    fallback,
                    stage,
                    slots: HashMap::default(),
                    metrics: None,
                },
            );
        }

        let family = self.families.families.get_mut(key).unwrap();
        if let Some(slot) = family.slots.get(&weight) {
            return slot.handle.clone();
        }

        let handle = self.assets.reserve_handle();
        family.slots.insert(
            weight,
            Slot {
                handle: handle.clone(),
                source: SlotSource::Pending,
            },
        );
        handle
    }
}

/// The parts of a Fontsource family record (`api.fontsource.org/v1/fonts/{id}`) we use.
#[derive(Deserialize)]
pub struct FontsourceFamily {
    subsets: Vec<String>,
    #[serde(rename = "npmVersion")]
    npm_version: String,
    /// weight ("400") -> style ("normal" / "italic") -> subset ("latin") -> files
    variants: HashMap<String, HashMap<String, HashMap<String, Variant>>>,
}

#[derive(Deserialize)]
struct Variant {
    url: VariantUrls,
}

#[derive(Deserialize)]
struct VariantUrls {
    ttf: Option<String>,
}

impl FontsourceFamily {
    /// The ttf url of the latin subset (or the family's first subset) of a variant, pinned
    /// to the record's package version so the url-hashed cache stays valid.
    fn ttf(&self, weight: &str, style: &str) -> Option<String> {
        let subsets = self.variants.get(weight)?.get(style)?;
        let subset = subsets
            .get("latin")
            .or_else(|| self.subsets.iter().find_map(|subset| subsets.get(subset)))?;
        let url = subset.url.ttf.as_ref()?;
        Some(url.replace("@latest/", &format!("@{}/", self.npm_version)))
    }
}

/// Pick the source for `weight` from a resolved Fontsource family: the nearest variant the
/// family has, falling back from bold / italic towards regular. Weights that resolve to the
/// same file share one asset load.
fn fontsource_source(
    family: &FontsourceFamily,
    weight: WeightName,
    ipfas: &IpfsAssetServer,
) -> SlotSource {
    const REGULAR: &[&str] = &["400", "500", "300", "600"];
    const BOLD: &[&str] = &["700", "600", "800", "500", "900"];
    let candidates: &[(&[&str], &str)] = match weight {
        WeightName::Regular => &[(REGULAR, "normal")],
        WeightName::Bold => &[(BOLD, "normal"), (REGULAR, "normal")],
        WeightName::Italic => &[(REGULAR, "italic"), (REGULAR, "normal")],
        WeightName::BoldItalic => &[(BOLD, "italic"), (BOLD, "normal"), (REGULAR, "normal")],
    };
    let url = candidates
        .iter()
        .find_map(|(weights, style)| weights.iter().find_map(|w| family.ttf(w, style)));
    match url {
        Some(url) => {
            let path = IpfsPath::new_from_url(&url, "ttf");
            SlotSource::Loading(ipfas.asset_server().load::<Font>(PathBuf::from(&path)))
        }
        None => SlotSource::Fallback,
    }
}

/// Fontsource id of a family name: "Playfair Display" -> "playfair-display". `None` if the
/// source can't be a family name (e.g. a path or url).
fn fontsource_id(src: &str) -> Option<String> {
    let id = src
        .split_whitespace()
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("-");
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).then_some(id)
}

async fn fetch_fontsource_family(
    client: reqwest::Client,
    id: String,
) -> Result<FontsourceFamily, anyhow::Error> {
    let url = format!("https://api.fontsource.org/v1/fonts/{id}");
    let response = platform::fetch(
        client.get(url).send(),
        Duration::from_secs(30),
        Duration::from_secs(10),
    )
    .await
    .map_err(|e| match e {
        platform::FetchError::Headers => anyhow!("timed out awaiting headers"),
        platform::FetchError::Send(e) => anyhow!(e),
        platform::FetchError::Status(status) if status.as_u16() == 404 => {
            anyhow!("unknown family")
        }
        platform::FetchError::Status(status) => anyhow!("status {status}"),
        platform::FetchError::Stalled => anyhow!("body transfer stalled"),
        platform::FetchError::Body(e) => anyhow!(e),
    })?;
    Ok(serde_json::from_slice(&response.body)?)
}

/// Advances every family's resolution and fills the handles text is waiting on.
fn update_scene_fonts(mut fonts: SceneFontServer, scenes: Query<(), With<RendererSceneContext>>) {
    let SceneFontServer {
        families,
        assets,
        ipfas,
    } = &mut fonts;
    families
        .families
        .retain(|key, _| scenes.contains(key.scene));

    for (key, family) in families.families.iter_mut() {
        // stage transitions
        match &mut family.stage {
            Stage::ContentFile(handle) => match ipfas.load_state(handle.id()) {
                LoadState::Loaded => {
                    debug!("font `{}` loaded as a scene file", key.src);
                    let font = assets.get(handle.id()).unwrap().clone();
                    family.stage = Stage::Single(font);
                }
                LoadState::Failed(_) => match fontsource_id(&key.src) {
                    Some(id) => {
                        let client = ipfas.ipfs().client();
                        family.stage = Stage::Lookup(
                            IoTaskPool::get().spawn_compat(fetch_fontsource_family(client, id)),
                        );
                    }
                    None => {
                        warn!(
                            "font `{}` is neither a scene file nor a family name",
                            key.src
                        );
                        family.stage = Stage::Failed;
                    }
                },
                _ => (),
            },
            Stage::Lookup(task) => {
                if let Some(result) =
                    futures_lite::future::block_on(futures_lite::future::poll_once(task))
                {
                    match result {
                        Ok(record) => {
                            debug!(
                                "font `{}` resolved via fontsource with weights {:?}",
                                key.src,
                                record.variants.keys().collect::<Vec<_>>()
                            );
                            family.stage = Stage::Fontsource(record);
                        }
                        Err(e) => {
                            warn!("font `{}`: fontsource lookup failed: {e}", key.src);
                            family.stage = Stage::Failed;
                        }
                    }
                }
            }
            _ => (),
        }

        // assign pending slots once the stage has settled
        if !matches!(family.stage, Stage::ContentFile(_) | Stage::Lookup(_)) {
            for (weight, slot) in family.slots.iter_mut() {
                if !matches!(slot.source, SlotSource::Pending) {
                    continue;
                }
                slot.source = match &family.stage {
                    Stage::Single(font) => {
                        assets.insert(slot.handle.id(), font.clone());
                        SlotSource::Done
                    }
                    Stage::Fontsource(record) => fontsource_source(record, *weight, ipfas),
                    _ => SlotSource::Fallback,
                };
            }
        }

        // fill handles whose sources have landed
        let mut landed = Vec::default();
        for (weight, slot) in family.slots.iter() {
            match &slot.source {
                SlotSource::Loading(source) => match ipfas.load_state(source.id()) {
                    LoadState::Loaded => {
                        debug!("font `{}` ({weight:?}) loaded", key.src);
                        landed.push((*weight, Ok(source.id())));
                    }
                    LoadState::Failed(e) => {
                        warn!("font `{}` ({weight:?}) failed to load: {e}", key.src);
                        landed.push((*weight, Err(())));
                    }
                    _ => (),
                },
                SlotSource::Fallback => landed.push((*weight, Err(()))),
                _ => (),
            }
        }
        for (weight, source) in landed {
            let font = match source {
                Ok(id) => assets.get(id).cloned(),
                Err(()) => assets.get(user_font(family.fallback, weight).id()).cloned(),
            };
            let Some(font) = font else {
                continue;
            };
            let slot = family.slots.get_mut(&weight).unwrap();
            assets.insert(slot.handle.id(), font);
            slot.source = SlotSource::Done;
        }
    }
}
