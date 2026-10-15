/* PbTextShape

    text: string;
foundation: works
bevy: works

    font?: Font | undefined;
does nothing

    fontSize?: number | undefined;
foundation: works.
bevy: works
- note that width W is specified in meters
- assuming P pixels per meter on the rendered text texture, a given fontSize of F should correspond to a rendered font size (in points) of "F * P / 10"
- e.g. if you use 100 px per meter, a font size of 10 should use underlying font size 100
- e.g. if you use 200 px per meter, a font size of 10 should use underlying font size 200

    fontAutoSize?: boolean | undefined;
foundation and bevy:
- doesn't change font size
- disables textWrapping

    textAlign?: TextAlignMode | undefined;
kinda of works
foundation + bevy: text align left / center / right justify the text
foundation + bevy: - text align top/middle/bottom change the anchor of the text block (so the quad y is (0, 1), (-0.5, 0.5), (-1, 0) respectively

    width?: number | undefined;
foundation + bevy: when used with textWrapping, specifies the wrap size in meters. otherwise no effect

    height?: number | undefined;
does nothing

    paddingTop?: number | undefined;
foundation: changes the y offset by -paddingTop, units unclear
bevy: changes the y offset by -paddingTop in meters

    paddingRight?: number | undefined;
foundation: reduces width
bevy: adds padding

    paddingBottom?: number | undefined;
foundation: changes the y offset by +paddingBottom, units unclear
bevy: changes the y offset by +paddingBottom in meters

    paddingLeft?: number | undefined;
foundation + bevy: adds padding to the left!

    lineSpacing?: number | undefined;
foundation: 100 adds a full line of space, less than 10 does nothing.
bevy: not implemented

    lineCount?: number | undefined;
foundation: when textWrapping is true, works. causes some strange re-interpretation of paddingTop/paddingBottom.
bevy: acts like height

    textWrapping?: boolean | undefined;
foundation + bevy: works

    shadowBlur?: number | undefined;
foundation: flaky when changed. when shadowOffsetX=1, a range around 1-5 is useable. suggest use some standard shadow when != 0
bevy: not implemented

    shadowOffsetX?: number | undefined;
foundation: when 0, disables shadows. otherwise affects shadowBlur in a non-linear way, bigger X -> smaller blur. doesn't change shadow offset at all.
bevy: not implemented

    shadowOffsetY?: number | undefined;
does nothing

    outlineWidth?: number | undefined;
foundation: changes outline thickness. units unclear. 0.15 seems to make something around half the letter size. larger than 0.25 just obscures the whole text. probably just check for non-zero and apply some standard outline in that case.
bevy: not implemented

    shadowColor?: Color3 | undefined;
foundation: works
bevy: not implemented

    outlineColor?: Color3 | undefined;
foundation: works
bevy: not implemented

    textColor?: Color4 | undefined;
foundation: works
bevy: not implemented


*/

use bevy::{
    diagnostic::FrameCount,
    ecs::{relationship::RelatedSpawner, spawn::SpawnWith},
    math::Affine2,
    pbr::NotShadowCaster,
    platform::collections::{HashMap, HashSet},
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        primitives::Aabb,
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        view::VisibilitySystems,
    },
    text::{
        ComputedTextBlock, CosmicBuffer, CosmicFontSystem, FontAtlasSet, FontSmoothing, LineBreak,
        LineHeight, SwashCache, TextBounds, TextPipeline,
    },
    ui::{update::update_clipping_system, widget::text_system, UiSystem},
};
use common::{sets::SceneLoopSets, structs::AppConfig, util::TryPushChildrenEx};
use cosmic_text::{CacheKey, PhysicalGlyph};
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::{
        sdk::components::{common::TextAlignMode, PbTextShape},
        Color4DclToBevy,
    },
    SceneComponentId,
};
use scene_material::{SceneBound, SceneMaterial};
use ui_core::{FontName, WeightName, FONT_SIZE_SCALE};
use unicode_segmentation::UnicodeSegmentation;

use crate::{renderer_context::RendererSceneContext, SceneEntity};

use super::{
    fonts::{FamilyKey, SceneFontServer, TextFontFamily},
    AddCrdtInterfaceExt,
};

pub struct TextShapePlugin;

impl Plugin for TextShapePlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbTextShape, TextShape>(
            SceneComponentId::TEXT_SHAPE,
            ComponentPosition::EntityOnly,
        );
        app.init_resource::<GlyphAtlases>();
        app.init_resource::<TextShapeMaterials>();
        app.init_resource::<SwashCache>();
        app.add_systems(
            Update,
            update_text_shapes.in_set(SceneLoopSets::UpdateWorld),
        );
        app.add_observer(text_shape_removed);

        app.add_systems(
            PostUpdate,
            apply_text_extras
                .after(text_system)
                .after(TransformSystem::TransformPropagate)
                .before(update_clipping_system)
                .before(UiSystem::Stack)
                .before(VisibilitySystems::VisibilityPropagate),
        );
        app.init_resource::<UnrecognisedTags>();
    }
}

#[derive(Component, Clone, PartialEq)]
pub struct TextShape(pub PbTextShape);

impl From<PbTextShape> for TextShape {
    fn from(value: PbTextShape) -> Self {
        Self(value)
    }
}

/// The glyph mesh drawn for a text shape, the component it was built from, and the atlas
/// glyphs it samples (so an atlas rebuild knows what to keep).
#[derive(Component)]
pub struct TextShapeMesh {
    mesh: Entity,
    source: PbTextShape,
    atlas: AtlasKey,
    generation: u32,
    glyphs: Vec<CacheKey>,
}

#[derive(Component)]
pub struct RetryTextShape(u32);

#[derive(Default, Resource, Deref, DerefMut)]
pub struct UnrecognisedTags(HashSet<String>);

/// Materials for text shape meshes, per scene, atlas and atlas generation.
#[derive(Default, Resource)]
struct TextShapeMaterials(HashMap<(Entity, AtlasKey, u32), Handle<SceneMaterial>>);

/// One atlas per font family: a scene family's glyph churn can only exhaust its own atlas,
/// never evict the built-in families' glyphs, and its atlas dies with the scene.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AtlasKey {
    Builtin(FontName),
    Scene(FamilyKey),
}

impl From<&TextFontFamily> for AtlasKey {
    fn from(family: &TextFontFamily) -> Self {
        match family {
            TextFontFamily::Builtin(name) => Self::Builtin(*name),
            TextFontFamily::Scene { key, .. } => Self::Scene(key.clone()),
        }
    }
}

#[derive(Default, Resource)]
pub struct GlyphAtlases(HashMap<AtlasKey, GlyphAtlas>);

impl GlyphAtlases {
    fn get_or_create(&mut self, key: AtlasKey, images: &mut Assets<Image>) -> &mut GlyphAtlas {
        self.0.entry(key).or_insert_with(|| GlyphAtlas::new(images))
    }
}

/// TMP auto-size point-size clamp used by the reference renderer:
/// defaultFontSize 36 * { minRatio 0.5, maxRatio 2 }.
const AUTO_MIN_PT: f32 = 18.0;
const AUTO_MAX_PT: f32 = 72.0;
/// Pixels-per-meter of laid out text per point of font size, with glyphs rastered at
/// [`CALIBRATION_FONT_SIZE`]. Calibrated against the reference renderer: lower = larger
/// text. Drives the world scale, the auto floor size, and the box<->point-size conversion in
/// the auto fit.
const PIX_PER_M_PER_PT: f32 = 190.0;
const CALIBRATION_FONT_SIZE: f32 = 20.0;
/// The font size text shapes are laid out and rastered at. The atlas is shared, so this only
/// costs glyph memory once per face; the world scale compensates for it.
const RASTER_FONT_SIZE: f32 = 64.0;
/// Shapes laid out per frame; the rest wait their turn, oldest first.
const MAX_BUILDS_PER_FRAME: usize = 64;

/// Pixels per meter of text laid out at [`RASTER_FONT_SIZE`] for a given point size.
fn pix_per_m(effective_pt: f32) -> f32 {
    PIX_PER_M_PER_PT * RASTER_FONT_SIZE / CALIBRATION_FONT_SIZE / effective_pt
}

/// Where a glyph sits in the atlas (pixels), and the raster's offset from the glyph origin.
#[derive(Clone, Copy)]
struct GlyphSlot {
    rect: URect,
    offset: IVec2,
}

/// A row of the atlas; glyphs are appended left to right.
struct Shelf {
    y: u32,
    height: u32,
    next_x: u32,
}

/// One texture holding every glyph a family's text shapes draw, in pixel units. A placed glyph
/// never moves: when the texture fills up it doubles in place (the old pixels stay at the same
/// top left) so existing meshes stay valid, and only the materials' uv scale changes. At
/// [`GlyphAtlas::MAX_SIZE`] it is rebuilt from the glyphs still in use instead, which moves
/// them: every shape built before the rebuild is then re-laid out (its generation is stale),
/// and keeps the old texture alive through its old material until it has.
#[derive(Resource)]
pub struct GlyphAtlas {
    image: Handle<Image>,
    size: u32,
    generation: u32,
    glyphs: HashMap<CacheKey, GlyphSlot>,
    shelves: Vec<Shelf>,
    shelves_bottom: u32,
    /// Changed this frame (growth): materials need their uv scale updated.
    resized: bool,
}

/// The atlas can't fit the glyph: at [`GlyphAtlas::MAX_SIZE`] and no room.
struct AtlasFull;

impl GlyphAtlas {
    fn new(images: &mut Assets<Image>) -> Self {
        let image = images.add(Self::blank_image(Self::INITIAL_SIZE));
        let mut atlas = Self {
            image,
            size: Self::INITIAL_SIZE,
            generation: 0,
            glyphs: HashMap::default(),
            shelves: Vec::default(),
            shelves_bottom: 0,
            resized: false,
        };
        atlas.reset(images);
        atlas
    }

    const INITIAL_SIZE: u32 = 512;
    /// 64MB of rgba; also the size of the upload whenever a glyph is added at this size.
    const MAX_SIZE: u32 = 4096;
    /// Glyph rasters are separated by this much to keep bilinear sampling from bleeding.
    const PADDING: u32 = 1;
    /// A block of white at the top left for the untextured quads (extras) to sample.
    const WHITE: URect = URect {
        min: UVec2::ZERO,
        max: UVec2::splat(4),
    };

    fn blank_image(size: u32) -> Image {
        let mut image = Image::new_fill(
            Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[0, 0, 0, 0],
            TextureFormat::Rgba8UnormSrgb,
            // cpu copy kept to add glyphs to
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        image.transfer_priority = bevy::asset::RenderAssetTransferPriority::Immediate;
        image
    }

    /// The uv of the white block's centre, in pixels.
    fn white_uv() -> Vec2 {
        Self::WHITE.center().as_vec2()
    }

    /// Start packing afresh into `self.image` (sized `self.size`): the white block first.
    fn reset(&mut self, images: &mut Assets<Image>) {
        self.glyphs.clear();
        self.shelves.clear();
        self.shelves_bottom = 0;
        let white = self
            .allocate(Self::WHITE.width(), Self::WHITE.height())
            .unwrap();
        debug_assert_eq!(white, Self::WHITE.min);
        let image = images.get_mut(&self.image).unwrap();
        for y in 0..Self::WHITE.height() {
            let start = ((y * self.size) * 4) as usize;
            image.data.as_mut().unwrap()[start..start + (Self::WHITE.width() * 4) as usize]
                .fill(255);
        }
    }

    /// Find room for a `width` x `height` raster, growing the atlas if needed.
    fn allocate(&mut self, width: u32, height: u32) -> Option<UVec2> {
        let padded_w = width + Self::PADDING;
        // shelves come in multiples of 8px so similar glyphs share them
        let shelf_h = (height + Self::PADDING).div_ceil(8) * 8;
        if let Some(shelf) = self
            .shelves
            .iter_mut()
            .find(|shelf| shelf.height == shelf_h && shelf.next_x + padded_w <= self.size)
        {
            let pos = UVec2::new(shelf.next_x, shelf.y);
            shelf.next_x += padded_w;
            return Some(pos);
        }
        if self.shelves_bottom + shelf_h <= self.size && padded_w <= self.size {
            let pos = UVec2::new(0, self.shelves_bottom);
            self.shelves.push(Shelf {
                y: self.shelves_bottom,
                height: shelf_h,
                next_x: padded_w,
            });
            self.shelves_bottom += shelf_h;
            return Some(pos);
        }
        None
    }

    /// Double the texture in place, keeping the old pixels where they were.
    fn grow(&mut self, images: &mut Assets<Image>) -> Result<(), AtlasFull> {
        if self.size >= Self::MAX_SIZE {
            return Err(AtlasFull);
        }
        let old_size = self.size;
        let new_size = old_size * 2;
        let image = images.get_mut(&self.image).unwrap();
        let old = image.data.take().unwrap();
        let mut data = vec![0u8; (new_size * new_size * 4) as usize];
        let row = (old_size * 4) as usize;
        for y in 0..old_size as usize {
            data[y * row * 2..y * row * 2 + row].copy_from_slice(&old[y * row..(y + 1) * row]);
        }
        image.data = Some(data);
        image.texture_descriptor.size.width = new_size;
        image.texture_descriptor.size.height = new_size;
        self.size = new_size;
        self.resized = true;
        info!("glyph atlas grown to {new_size}");
        Ok(())
    }

    /// Replace the texture with one holding only `in_use` glyphs, repacked; the old texture
    /// lives on through the old materials until the shapes using it are rebuilt.
    fn rebuild(&mut self, images: &mut Assets<Image>, in_use: &HashSet<CacheKey>) {
        let old_glyphs = std::mem::take(&mut self.glyphs);
        let old_image =
            std::mem::replace(&mut self.image, images.add(Self::blank_image(self.size)));
        self.generation += 1;
        self.reset(images);
        let old_data = images.get(&old_image).unwrap().data.clone().unwrap();
        let old_size = self.size;
        for (key, slot) in old_glyphs {
            if !in_use.contains(&key) {
                continue;
            }
            let (w, h) = (slot.rect.width(), slot.rect.height());
            let Some(pos) = self.allocate(w, h) else {
                // can't happen: the old texture held them all, and it was the same size
                warn!("glyph atlas rebuild overflowed");
                break;
            };
            let image = images.get_mut(&self.image).unwrap();
            let data = image.data.as_mut().unwrap();
            for y in 0..h {
                let src = (((slot.rect.min.y + y) * old_size + slot.rect.min.x) * 4) as usize;
                let dst = (((pos.y + y) * self.size + pos.x) * 4) as usize;
                data[dst..dst + (w * 4) as usize]
                    .copy_from_slice(&old_data[src..src + (w * 4) as usize]);
            }
            self.glyphs.insert(
                key,
                GlyphSlot {
                    rect: URect::from_corners(pos, pos + UVec2::new(w, h)),
                    offset: slot.offset,
                },
            );
        }
        info!(
            "glyph atlas rebuilt (generation {}): {} glyphs kept",
            self.generation,
            self.glyphs.len()
        );
    }

    /// The slot of a glyph, rastering it into the atlas if it isn't there yet. `None` for
    /// glyphs with no ink (spaces).
    fn glyph(
        &mut self,
        physical: &PhysicalGlyph,
        images: &mut Assets<Image>,
        font_system: &mut CosmicFontSystem,
        swash_cache: &mut SwashCache,
    ) -> Result<Option<GlyphSlot>, AtlasFull> {
        if let Some(slot) = self.glyphs.get(&physical.cache_key) {
            return Ok((!slot.rect.is_empty()).then_some(*slot));
        }

        let Ok((raster, offset)) = FontAtlasSet::get_outlined_glyph_texture(
            &mut font_system.0,
            &mut swash_cache.0,
            physical,
            FontSmoothing::AntiAliased,
        ) else {
            // no raster (missing glyph): draw nothing for it
            self.glyphs.insert(
                physical.cache_key,
                GlyphSlot {
                    rect: URect::EMPTY,
                    offset: IVec2::ZERO,
                },
            );
            return Ok(None);
        };

        let (w, h) = (raster.width(), raster.height());
        let mut slot = GlyphSlot {
            rect: URect::EMPTY,
            offset,
        };
        if w > 0 && h > 0 {
            let pos = loop {
                if let Some(pos) = self.allocate(w, h) {
                    break pos;
                }
                self.grow(images)?;
            };
            let image = images.get_mut(&self.image).unwrap();
            let data = image.data.as_mut().unwrap();
            let src = raster.data.as_ref().unwrap();
            for y in 0..h {
                let dst = (((pos.y + y) * self.size + pos.x) * 4) as usize;
                data[dst..dst + (w * 4) as usize]
                    .copy_from_slice(&src[(y * w * 4) as usize..((y + 1) * w * 4) as usize]);
            }
            slot.rect = URect::from_corners(pos, pos + UVec2::new(w, h));
        }
        self.glyphs.insert(physical.cache_key, slot);
        Ok((!slot.rect.is_empty()).then_some(slot))
    }
}

/// With `fontAutoSize`, match unity-explorer: ignore the SDK font size and size
/// the font so the (unwrapped) text block fits the box, clamped to [18, 72]pt.
///
/// The box only applies when `text_wrapping` is set (TMP only sizes its rect
/// when wrapping is requested; otherwise the rect is zero and auto pins to the
/// floor). Because wrapping is force-disabled while auto-sizing, the block
/// scales linearly with point size, so one measurement of the unwrapped block
/// at the raster size gives a closed-form fit — no iterative re-layout.
///
/// Returns `None` if a required font asset isn't loaded yet (caller retries).
fn auto_fit_point_size(
    text_shape: &PbTextShape,
    spans: &[TextSpanData],
    text_pipeline: &mut TextPipeline,
    fonts: &SceneFontServer,
    font_system: &mut CosmicFontSystem,
    measure_entity: Entity,
) -> Option<f32> {
    if !text_shape.text_wrapping() {
        return Some(AUTO_MIN_PT);
    }

    // create_text_measure panics if a span's font hasn't loaded yet.
    if spans
        .iter()
        .any(|(_, font, _, _)| fonts.assets().get(font.font.id()).is_none())
    {
        return None;
    }

    let measure_layout = TextLayout::new(JustifyText::Left, LineBreak::NoWrap);
    let mut computed = ComputedTextBlock::default();
    // `max` is the unbounded (unwrapped) block size in pixels at the raster size.
    let natural = text_pipeline
        .create_text_measure(
            measure_entity,
            fonts.assets(),
            span_iter(measure_entity, spans),
            1.0,
            &measure_layout,
            &mut computed,
            font_system,
        )
        .ok()?
        .max;

    if natural.x <= 0.0 || natural.y <= 0.0 {
        return Some(AUTO_MIN_PT);
    }

    // Unity defaults the missing dimension to a tiny value, so a w-only / h-only
    // box still pins to the floor.
    let w_box = text_shape.width.unwrap_or(1.0);
    let h_box = text_shape.height.unwrap_or(0.2);
    let fit = pix_per_m(1.0) * (w_box / natural.x).min(h_box / natural.y);
    Some(fit.clamp(AUTO_MIN_PT, AUTO_MAX_PT))
}

fn span_iter(
    entity: Entity,
    spans: &[TextSpanData],
) -> impl Iterator<Item = (Entity, usize, &str, &TextFont, Color)> {
    spans
        .iter()
        .enumerate()
        .map(move |(i, (span, font, color, _))| (entity, i, span.0.as_str(), font, color.0))
}

/// An axis-aligned quad in text pixels (y down from the block's top left), with its atlas
/// rect in pixels.
struct TextQuad {
    min: Vec2,
    max: Vec2,
    uv_min: Vec2,
    uv_max: Vec2,
    color: LinearRgba,
}

/// A laid out block: a quad per glyph and per extra (underline etc).
#[derive(Default)]
struct LaidOutText {
    quads: Vec<TextQuad>,
    glyphs: Vec<CacheKey>,
    /// Width of the widest line and height of all the lines.
    size: Vec2,
}

enum LayoutError {
    Text(bevy::text::TextError),
    AtlasFull,
}

impl From<bevy::text::TextError> for LayoutError {
    fn from(value: bevy::text::TextError) -> Self {
        Self::Text(value)
    }
}

/// Lay the spans out and gather a quad per glyph, rastering glyphs into the atlas as needed.
/// `width` wraps (if `linebreak` allows it) and is the width lines are justified within;
/// unbounded text is justified within its widest line.
#[allow(clippy::too_many_arguments)]
fn layout_text(
    entity: Entity,
    spans: &[TextSpanData],
    justify: JustifyText,
    linebreak: LineBreak,
    width: Option<f32>,
    text_pipeline: &mut TextPipeline,
    fonts: &Assets<Font>,
    font_system: &mut CosmicFontSystem,
    swash_cache: &mut SwashCache,
    atlas: &mut GlyphAtlas,
    images: &mut Assets<Image>,
) -> Result<LaidOutText, LayoutError> {
    let mut computed = ComputedTextBlock::default();
    let mut update = |width: Option<f32>, computed: &mut ComputedTextBlock| {
        text_pipeline.update_buffer(
            fonts,
            span_iter(entity, spans),
            linebreak,
            justify,
            TextBounds {
                width,
                height: None,
            },
            1.0,
            computed,
            font_system,
        )
    };
    update(width, &mut computed)?;

    let block_size = |buffer: &CosmicBuffer| {
        buffer.layout_runs().fold(Vec2::ZERO, |size, run| {
            Vec2::new(
                size.x.max(run.line_w),
                size.y.max(run.line_top + run.line_height),
            )
        })
    };
    let mut size = block_size(computed.buffer());
    if width.is_none() && justify != JustifyText::Left && computed.buffer().lines.len() > 1 {
        update(Some(size.x), &mut computed)?;
        size = block_size(computed.buffer());
    }
    if let Some(width) = width {
        size.x = width;
    }

    let mut out = LaidOutText {
        size,
        ..Default::default()
    };

    for run in computed.buffer().layout_runs() {
        // extras cover the run's glyphs of each span, as one segment per span
        let mut segment: Option<(usize, f32, f32)> = None;
        let flush = |segment: Option<(usize, f32, f32)>, quads: &mut Vec<TextQuad>| {
            let Some((span, start, end)) = segment else {
                return;
            };
            let (_, _, color, Some(extras)) = &spans[span] else {
                return;
            };
            let mut bar = |top: f32, height: f32, color: Color| {
                let top = run.line_top + run.line_height * top;
                quads.push(TextQuad {
                    min: Vec2::new(start, top),
                    max: Vec2::new(end, top + (run.line_height * height).max(1.0)),
                    uv_min: GlyphAtlas::white_uv(),
                    uv_max: GlyphAtlas::white_uv(),
                    color: color.to_linear(),
                });
            };
            if let Some(mark) = extras.mark {
                bar(0.1, 1.0, mark);
            }
            if extras.strike {
                bar(0.5, 0.1, color.0);
            }
            if extras.underline {
                bar(0.95, 0.1, color.0);
            }
        };

        for glyph in run.glyphs {
            let span = glyph.metadata;
            match segment.as_mut() {
                Some((seg_span, _, end)) if *seg_span == span => *end = glyph.x + glyph.w,
                _ => {
                    flush(segment.take(), &mut out.quads);
                    segment = Some((span, glyph.x, glyph.x + glyph.w));
                }
            }

            let (_, _, color, _) = &spans[span];

            // raster each glyph once: fold the subpixel offset into the (rounded)
            // position so the cache key lands in the zero bin
            let mut glyph = glyph.clone();
            glyph.x = (glyph.x + glyph.font_size * glyph.x_offset).round();
            glyph.y = (glyph.y - glyph.font_size * glyph.y_offset).trunc();
            glyph.x_offset = 0.0;
            glyph.y_offset = 0.0;
            let physical = glyph.physical((0.0, 0.0), 1.0);

            let slot = match atlas.glyph(&physical, images, font_system, swash_cache) {
                Ok(Some(slot)) => slot,
                Ok(None) => continue,
                Err(AtlasFull) => return Err(LayoutError::AtlasFull),
            };
            out.glyphs.push(physical.cache_key);

            let min = Vec2::new(
                physical.x as f32 + slot.offset.x as f32,
                run.line_y.round() + physical.y as f32 - slot.offset.y as f32,
            );
            out.quads.push(TextQuad {
                min,
                max: min + slot.rect.size().as_vec2(),
                uv_min: slot.rect.min.as_vec2(),
                uv_max: slot.rect.max.as_vec2(),
                color: color.0.to_linear(),
            });
        }
        flush(segment.take(), &mut out.quads);
    }

    Ok(out)
}

/// Build a mesh of the quads, placed in meters with `to_world` (text px -> world xy).
fn quad_mesh(quads: &[TextQuad], to_world: impl Fn(Vec2) -> Vec2) -> (Mesh, Aabb) {
    let mut positions = Vec::with_capacity(quads.len() * 4);
    let mut uvs = Vec::with_capacity(quads.len() * 4);
    let mut colors = Vec::with_capacity(quads.len() * 4);
    let mut indices = Vec::with_capacity(quads.len() * 6);
    let mut min = Vec2::INFINITY;
    let mut max = Vec2::NEG_INFINITY;

    for quad in quads {
        let base = positions.len() as u32;
        for (corner, uv) in [
            (
                Vec2::new(quad.min.x, quad.min.y),
                Vec2::new(quad.uv_min.x, quad.uv_min.y),
            ),
            (
                Vec2::new(quad.max.x, quad.min.y),
                Vec2::new(quad.uv_max.x, quad.uv_min.y),
            ),
            (
                Vec2::new(quad.max.x, quad.max.y),
                Vec2::new(quad.uv_max.x, quad.uv_max.y),
            ),
            (
                Vec2::new(quad.min.x, quad.max.y),
                Vec2::new(quad.uv_min.x, quad.uv_max.y),
            ),
        ] {
            let world = to_world(corner);
            min = min.min(world);
            max = max.max(world);
            positions.push([world.x, world.y, 0.0]);
            uvs.push([uv.x, uv.y]);
            colors.push(quad.color.to_f32_array());
        }
        // counter-clockwise with y up: top-left, bottom-left, bottom-right, top-right
        indices.extend_from_slice(&[base, base + 3, base + 2, base, base + 2, base + 1]);
    }

    let count = positions.len();
    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices));

    let aabb = Aabb::from_min_max(min.extend(-0.01), max.extend(0.01));
    (mesh, aabb)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_text_shapes(
    mut commands: Commands,
    query: Query<
        (
            Entity,
            &SceneEntity,
            &TextShape,
            Option<&TextShapeMesh>,
            Option<&RetryTextShape>,
        ),
        Or<(Changed<TextShape>, With<RetryTextShape>)>,
    >,
    all_meshes: Query<(Entity, &TextShapeMesh)>,
    scenes: Query<&RendererSceneContext>,
    frame: Res<FrameCount>,
    mut unrecognized_tags: ResMut<UnrecognisedTags>,
    mut text_pipeline: ResMut<TextPipeline>,
    mut scene_fonts: SceneFontServer,
    mut font_system: ResMut<CosmicFontSystem>,
    mut swash_cache: ResMut<SwashCache>,
    mut atlases: ResMut<GlyphAtlases>,
    (mut images, mut meshes, mut materials): (
        ResMut<Assets<Image>>,
        ResMut<Assets<Mesh>>,
        ResMut<Assets<SceneMaterial>>,
    ),
    mut material_cache: ResMut<TextShapeMaterials>,
    config: Res<AppConfig>,
) {
    // oldest waiting shapes first
    let mut waiting = query
        .iter()
        .filter_map(|(_, _, _, _, retry)| retry.map(|r| r.0))
        .collect::<Vec<_>>();
    waiting.sort();
    let stop_at = waiting.get(MAX_BUILDS_PER_FRAME).copied();

    let mut built = 0;
    let mut deferred = 0;
    let mut rebuilt = HashSet::<AtlasKey>::default();

    for (ent, scene_ent, text_shape, maybe_prior, maybe_retry) in query.iter() {
        let skip = built >= MAX_BUILDS_PER_FRAME
            || stop_at.is_some_and(|stop_at| maybe_retry.is_none_or(|retry| retry.0 > stop_at));

        if skip {
            if maybe_retry.is_none() {
                commands.entity(ent).try_insert(RetryTextShape(frame.0));
            }
            deferred += 1;
            continue;
        }

        let retry = |commands: &mut Commands| {
            commands.entity(ent).try_insert(RetryTextShape(frame.0));
        };

        let Ok(scene) = scenes.get(scene_ent.root) else {
            warn!("no scene!");
            continue;
        };
        let family = scene_fonts.family(
            scene_ent.root,
            &scene.hash,
            text_shape.0.font(),
            text_shape.0.font_src.as_deref(),
        );

        // wait for the family before taking a build slot. requesting the regular face
        // starts a new family loading; other weights are requested when the spans are built
        scene_fonts.face(&family, WeightName::Regular);
        if !scene_fonts.family_ready(&family) {
            retry(&mut commands);
            continue;
        }

        built += 1;
        debug!("ts: {:?}", text_shape.0);

        let atlas_key = AtlasKey::from(&family);
        let atlas = atlases.get_or_create(atlas_key.clone(), &mut images);

        if let Some(prior) = maybe_prior {
            if prior.source == text_shape.0
                && prior.atlas == atlas_key
                && prior.generation == atlas.generation
            {
                commands.entity(ent).try_remove::<RetryTextShape>();
                continue;
            }
        }

        if text_shape.0.text.is_empty() || text_shape.0.font_size.is_some_and(|size| size <= 0.0) {
            clean_up_text_shape(&mut commands, ent, maybe_prior);
            continue;
        }

        let wrapping = text_shape.0.text_wrapping() && !text_shape.0.font_auto_size();

        let text_align = text_shape
            .0
            .text_align
            .map(|_| text_shape.0.text_align())
            .unwrap_or(TextAlignMode::TamMiddleCenter);

        // vertical anchor: 0 = top of the block at the entity, 0.5 = middle, 1 = bottom
        let valign = match text_align {
            TextAlignMode::TamTopLeft
            | TextAlignMode::TamTopCenter
            | TextAlignMode::TamTopRight => 0.0,
            TextAlignMode::TamMiddleLeft
            | TextAlignMode::TamMiddleCenter
            | TextAlignMode::TamMiddleRight => 0.5,
            TextAlignMode::TamBottomLeft
            | TextAlignMode::TamBottomCenter
            | TextAlignMode::TamBottomRight => 1.0,
        };

        // horizontal anchor, same convention, and the justification within the block
        let (halign, justify) = match text_align {
            TextAlignMode::TamTopLeft
            | TextAlignMode::TamMiddleLeft
            | TextAlignMode::TamBottomLeft => (0.0, JustifyText::Left),
            TextAlignMode::TamTopCenter
            | TextAlignMode::TamMiddleCenter
            | TextAlignMode::TamBottomCenter => (0.5, JustifyText::Center),
            TextAlignMode::TamTopRight
            | TextAlignMode::TamMiddleRight
            | TextAlignMode::TamBottomRight => (1.0, JustifyText::Right),
        };

        // truncate very long text (used for both measuring and rendering)
        let source = if text_shape.0.text.len() > 2048 {
            warn!(
                "textshape text truncated from {} to 2048 chars",
                text_shape.0.text.len()
            );
            let end = text_shape
                .0
                .text
                .as_str()
                .char_indices()
                .find(|(ix, _)| *ix >= 2048)
                .map(|(ix, _)| ix)
                .unwrap_or(text_shape.0.text.len());
            &text_shape.0.text.as_str()[0..end]
        } else {
            text_shape.0.text.as_str()
        };

        let (spans, _) = build_text_spans(
            source,
            RASTER_FONT_SIZE,
            text_shape
                .0
                .text_color
                .map(Color4DclToBevy::convert_srgba)
                .unwrap_or(Color::WHITE),
            &family,
            &mut scene_fonts,
            &mut unrecognized_tags,
        );

        // building the spans may have requested more faces; wait for them
        if !scene_fonts.family_ready(&family) {
            retry(&mut commands);
            continue;
        }

        // With fontAutoSize the SDK font size is ignored; the font is sized to
        // fit the text block into the box (clamped), matching unity-explorer.
        // Otherwise the SDK font size drives the world scale directly.
        let effective_pt = if text_shape.0.font_auto_size() {
            match auto_fit_point_size(
                &text_shape.0,
                &spans,
                &mut text_pipeline,
                &scene_fonts,
                &mut font_system,
                ent,
            ) {
                Some(pt) => pt,
                None => {
                    // required font not loaded yet; retry next frame
                    retry(&mut commands);
                    continue;
                }
            }
        } else {
            text_shape.0.font_size.unwrap_or(10.0)
        };

        let pix_per_m = pix_per_m(effective_pt);
        let padding_left = text_shape.0.padding_left() * pix_per_m;
        let padding_right = text_shape.0.padding_right() * pix_per_m;
        let add_y_pix = (text_shape.0.padding_bottom() - text_shape.0.padding_top()) * pix_per_m;

        // when wrapping, the box is the sdk width (less the side padding for the lines)
        let wrap_width = wrapping.then(|| {
            (text_shape.0.width.unwrap_or(1.0) * pix_per_m - padding_left - padding_right).max(1.0)
        });

        let laid_out = match layout_text(
            ent,
            &spans,
            justify,
            if wrapping {
                LineBreak::WordOrCharacter
            } else {
                LineBreak::NoWrap
            },
            wrap_width,
            &mut text_pipeline,
            scene_fonts.assets(),
            &mut font_system,
            &mut swash_cache,
            atlas,
            &mut images,
        ) {
            Ok(laid_out) => laid_out,
            Err(LayoutError::Text(e)) => {
                debug!("textshape {ent:?} layout failed, retrying: {e}");
                retry(&mut commands);
                continue;
            }
            Err(LayoutError::AtlasFull) => {
                if rebuilt.contains(&atlas_key) {
                    // still full after this frame's rebuild: more live glyphs than fit
                    warn!("glyph atlas {atlas_key:?} full, textshape {ent:?} dropped");
                    clean_up_text_shape(&mut commands, ent, maybe_prior);
                    continue;
                }
                let in_use = all_meshes
                    .iter()
                    .filter(|(_, mesh)| {
                        mesh.atlas == atlas_key && mesh.generation == atlas.generation
                    })
                    .flat_map(|(_, mesh)| mesh.glyphs.iter().copied())
                    .collect::<HashSet<_>>();
                atlas.rebuild(&mut images, &in_use);
                rebuilt.insert(atlas_key);
                retry(&mut commands);
                continue;
            }
        };

        // the block (lines + side padding) is anchored at the entity by the alignment. when
        // wrapping the sdk box replaces the block: it is centred on the entity and the lines
        // are aligned within it (a fixed sdk height applies unless auto-sizing, which only
        // uses the box to pick the font size)
        let block = laid_out.size + Vec2::new(padding_left + padding_right, 0.0);
        let (box_size, anchor, inner_y) = if wrapping {
            let box_h = match text_shape.0.height {
                Some(h) if !text_shape.0.font_auto_size() => h * pix_per_m,
                _ => block.y,
            };
            (
                Vec2::new(block.x, box_h),
                Vec2::splat(0.5),
                (box_h - block.y) * valign,
            )
        } else {
            (block, Vec2::new(halign, valign), 0.0)
        };
        // text px (y down from the box's top left) -> meters (y up), the box's anchor at the
        // entity
        let origin = Vec2::new(-box_size.x * anchor.x, box_size.y * anchor.y);
        let to_world = move |p: Vec2| {
            let p = p + Vec2::new(padding_left, inner_y);
            Vec2::new(origin.x + p.x, origin.y - p.y + add_y_pix) / pix_per_m
        };

        let material = material_cache
            .0
            .entry((scene_ent.root, atlas_key.clone(), atlas.generation))
            .or_insert_with(|| {
                materials.add(SceneMaterial {
                    base: StandardMaterial {
                        base_color: Color::WHITE,
                        base_color_texture: Some(atlas.image.clone()),
                        uv_transform: Affine2::from_scale(Vec2::splat(1.0 / atlas.size as f32)),
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        ..Default::default()
                    },
                    extension: SceneBound::new(&scene.scene_bounds, config.graphics.oob),
                })
            })
            .clone();

        let (mesh, aabb) = quad_mesh(&laid_out.quads, to_world);
        let mesh_ent = commands
            .spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(material),
                aabb,
                NotShadowCaster,
                ChildOf(ent),
            ))
            .id();

        clean_up_text_shape(&mut commands, ent, maybe_prior);
        commands.entity(ent).try_insert(TextShapeMesh {
            mesh: mesh_ent,
            source: text_shape.0.clone(),
            atlas: atlas_key,
            generation: atlas.generation,
            glyphs: laid_out.glyphs,
        });

        debug!("[{}] textshape {ent:?}", frame.0);
    }

    for (key, atlas) in atlases.0.iter_mut() {
        if atlas.resized {
            // the atlas grew in place: glyph rects (in pixels) are unchanged, only the scale.
            // writing the material also re-prepares its bind group against the new, larger
            // texture (same-size glyph additions reuse the texture and need no rebind)
            let scale = Affine2::from_scale(Vec2::splat(1.0 / atlas.size as f32));
            for ((_, mat_key, generation), handle) in material_cache.0.iter() {
                if mat_key == key && *generation == atlas.generation {
                    if let Some(material) = materials.get_mut(handle) {
                        material.base.uv_transform = scale;
                    }
                }
            }
            atlas.resized = false;
        }
    }

    if !rebuilt.is_empty() {
        // everything built against an old texture is stale; it keeps drawing from the old
        // texture (via its old material) until it is rebuilt
        for (ent, mesh) in all_meshes.iter() {
            if rebuilt.contains(&mesh.atlas) && atlases.0[&mesh.atlas].generation != mesh.generation
            {
                commands.entity(ent).try_insert(RetryTextShape(frame.0));
            }
        }
    }

    // scene families' atlases die with their scene (the built-in ones live on); forget the
    // materials of dead scenes, dead atlases and old generations: meshes still using them
    // keep them (and the old texture) alive
    atlases.0.retain(|key, atlas| match key {
        AtlasKey::Builtin(_) => true,
        AtlasKey::Scene(family) => {
            let keep = scenes.contains(family.scene());
            if !keep {
                info!(
                    "glyph atlas {key:?} dropped ({} glyphs, {}px)",
                    atlas.glyphs.len(),
                    atlas.size
                );
            }
            keep
        }
    });
    material_cache.0.retain(|(scene, key, generation), _| {
        scenes.contains(*scene)
            && atlases
                .0
                .get(key)
                .is_some_and(|atlas| atlas.generation == *generation)
    });

    debug!("built: {built}, deferred: {deferred}");
}

fn text_shape_removed(
    trigger: Trigger<OnRemove, TextShape>,
    mut commands: Commands,
    meshes: Query<&TextShapeMesh>,
) {
    let entity = trigger.target();
    debug!("Removing TextShape");
    clean_up_text_shape(&mut commands, entity, meshes.get(entity).ok());
}

fn clean_up_text_shape(commands: &mut Commands, entity: Entity, prior: Option<&TextShapeMesh>) {
    if let Some(prior) = prior {
        commands.entity(prior.mesh).try_despawn();
    }
    commands
        .entity(entity)
        .try_remove::<(TextShapeMesh, RetryTextShape)>();
}

#[derive(Component)]
pub struct TextExtraMarker;

#[inline]
fn round_ties_up(value: f32) -> f32 {
    if value.fract() != -0.5 {
        value.round()
    } else {
        value.ceil()
    }
}

#[inline]
fn round_layout_coords(value: Vec2) -> Vec2 {
    Vec2 {
        x: round_ties_up(value.x),
        y: round_ties_up(value.y),
    }
}
fn apply_text_extras(
    mut commands: Commands,
    text: Query<(
        Entity,
        &Children,
        &ChildOf,
        &GlobalTransform,
        Ref<TextLayout>,
        &ComputedTextBlock,
        &ComputedNode,
        &ComputedNodeTarget,
    )>,
    changed_spans: Query<
        &ChildOf,
        (
            With<TextExtras>,
            Or<(Changed<TextSpan>, Changed<TextExtras>)>,
        ),
    >,
    spans: Query<(&TextSpan, &TextColor, Option<&TextExtras>)>,
    parents: Query<(&GlobalTransform, &ComputedNode)>,
    existing: Query<(), With<TextExtraMarker>>,
    mut removed: RemovedComponents<TextExtras>,
) {
    let find_bounds = |buffer: &CosmicBuffer, text: &str, section: (usize, usize)| -> Vec<Vec4> {
        let mut segments = Vec::default();

        let char_position = |text: &str| -> (usize, usize) {
            let line_count = text.chars().filter(|c| *c == '\n').count();
            let enclosing_line_char_index = text
                .char_indices()
                .rfind(|(_, c)| *c == '\n')
                .map(|(ix, _)| ix + 1)
                .unwrap_or(0);
            let char_index = text
                .char_indices()
                .last()
                .map(|(ix, _)| ix + 1)
                .unwrap_or(0)
                - enclosing_line_char_index;
            (line_count, char_index)
        };

        let (start_line, start_char) = char_position(&text[..section.0]);
        let (end_line, end_char) = char_position(&text[..section.1]);

        let mut segment_y = f32::NEG_INFINITY;
        let runs = buffer
            .layout_runs()
            .skip_while(|run| run.line_i < start_line)
            .take_while(|run| run.line_i <= end_line);

        for run in runs {
            let glyphs = run
                .glyphs
                .iter()
                .skip_while(|g| run.line_i == start_line && g.start < start_char)
                .take_while(|g| run.line_i < end_line || g.end <= end_char);

            for glyph in glyphs {
                debug!("g: {},{}", glyph.x, glyph.y);
                if run.line_top + glyph.y != segment_y {
                    segments.push(Vec4::new(
                        glyph.x,
                        run.line_top + glyph.y,
                        glyph.w,
                        run.line_height,
                    ));
                    segment_y = run.line_top + glyph.y;
                } else {
                    let segment = segments.last_mut().unwrap();
                    segment.z = glyph.x + glyph.w - segment.x;
                }
            }
        }

        segments
    };

    let removed_extras = removed.read().collect::<HashSet<_>>();
    let changed = changed_spans
        .iter()
        .map(|c| c.parent())
        .collect::<HashSet<_>>();

    for (entity, children, parent, gt, layout, computed_text, computed_node, node_target) in
        text.iter()
    {
        let mut requires_update = layout.is_changed();
        requires_update |= changed.contains(&entity);
        requires_update |= children.iter().any(|child| removed_extras.contains(&child));

        if !requires_update {
            continue;
        }

        // remove prior
        for child in children {
            if existing.get(*child).is_ok() {
                commands.entity(*child).despawn();
            }
        }

        // the marks are children of the text node's parent, and their insets resolve
        // against its edge, so account for the text node's offset within it (padding)
        let parent_tl = gt.translation().truncate() - computed_node.size * 0.5;
        let parent_offset = parents
            .get(parent.parent())
            .map(|(parent_gt, parent_node)| {
                parent_tl - (parent_gt.translation().truncate() - parent_node.size * 0.5)
            })
            .unwrap_or(Vec2::ZERO);

        let mut make_mark = |bound: Vec4, color: Color, top: f32, height: f32| -> Entity {
            // because we make marks based on calculated text positions, we have to run after the ui layout functions
            // but that means our marks won't be positioned until next frame. if text is deleted/replaced every frame
            // then it never shows.
            // so we do the layouting ourselves, copying bevy's `update_uinode_geometry_recursive`, and set the visibility explicitly.
            // we have to also add the equivalent style settings, or it will be overwritten next frame.
            let mut view_visibility = ViewVisibility::default();
            view_visibility.set();
            let height = (bound.w * height).max(1.0);
            let size = Vec2::new(bound.z, height);
            let my_tl = parent_tl + Vec2::new(bound.x, bound.y + bound.w * top);
            let my_global_translation = round_layout_coords(my_tl) + size * 0.5;
            let mut cmds = commands.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(bound.x + parent_offset.x),
                    top: Val::Px(bound.y + bound.w * top + parent_offset.y),
                    width: Val::Px(bound.z),
                    height: Val::Px(height),
                    ..Default::default()
                },
                BackgroundColor(color),
                ZIndex(-1),
                view_visibility,
                ComputedNode {
                    stack_index: computed_node.stack_index + 1,
                    size: round_layout_coords(size),
                    outline_width: 0.0,
                    outline_offset: 0.0,
                    unrounded_size: size,
                    content_size: size,
                    border: BorderRect::ZERO,
                    border_radius: ResolvedBorderRadius::ZERO,
                    padding: BorderRect::ZERO,
                    inverse_scale_factor: computed_node.inverse_scale_factor,
                },
                GlobalTransform::from_translation(my_global_translation.extend(0.0)),
                *node_target,
                TextExtraMarker,
            ));

            if let Some(camera) = node_target.camera() {
                cmds.try_insert(UiTargetCamera(camera));
            }

            cmds.id()
        };

        let mut text = String::default();
        let mut spanned_extras = Vec::default();
        for &child in children {
            let Ok((span, color, maybe_extras)) = spans.get(child) else {
                continue;
            };

            if let Some(extras) = maybe_extras {
                spanned_extras.push((extras, color.0, text.len(), text.len() + span.0.len()));
            }
            text.push_str(span.0.as_str());
        }

        let buffer = computed_text.buffer();
        let mut ents = Vec::default();

        for (extras, text_color, start, end) in spanned_extras {
            let bounds = find_bounds(buffer, &text, (start, end));
            for bound in bounds {
                if let Some(color) = extras.mark {
                    ents.push(make_mark(bound, color, 0.1, 1.0));
                }
                if extras.strike {
                    ents.push(make_mark(bound, text_color, 0.5, 0.1));
                }
                if extras.underline {
                    ents.push(make_mark(bound, text_color, 0.95, 0.1));
                }
            }
        }

        commands
            .entity(parent.parent())
            .try_push_children(ents.as_slice());
    }
}

#[derive(Component, Default, Clone, Copy, Debug)]
pub struct TextExtras {
    strike: bool,
    underline: bool,
    mark: Option<Color>,
}

type TextSpanData = (TextSpan, TextFont, TextColor, Option<TextExtras>);

fn build_text_spans(
    text: &str,
    font_size: f32,
    color: Color,
    family: &TextFontFamily,
    fonts: &mut SceneFontServer,
    unrecognized_tags: &mut UnrecognisedTags,
) -> (Vec<TextSpanData>, Vec<(usize, String)>) {
    let mut links = Vec::default();

    // The reference renderer treats CR/LF as teletype ops (CR returns to the
    // line start, LF advances a line) which we can't reproduce; strip CRs so
    // CRLF renders as a single line break and a lone CR collapses onto one line
    // (the realistic cases). A lone LF is left as a normal line break.
    let text = text.replace("\\n", "\n").replace('\r', "");

    let line_height = LineHeight::RelativeToFont(fonts.metrics(family).line_height);

    // split by <b>s and <i>s
    let mut b_count = 0usize;
    let mut i_count = 0usize;
    let mut u_count = 0usize;
    let mut s_count = 0usize;
    let mut marks = Vec::<Color>::default();
    let mut override_colors = Vec::<Color>::default();
    let mut section_start = 0usize;
    let mut link_data = Vec::<String>::default();

    let mut spans = Vec::default();
    let mut span_index = 0;

    while section_start < text.len() {
        // read initial tags
        while text[section_start..].starts_with('<') {
            if let Some((close, _)) = text[section_start..]
                .char_indices()
                .find(|(_, c)| *c == '>')
            {
                let tag = text[section_start + 1..section_start + close]
                    .trim()
                    .to_ascii_lowercase();
                match tag.as_str() {
                    "b" => b_count += 1,
                    "i" => i_count += 1,
                    "s" => s_count += 1,
                    "u" => u_count += 1,
                    "/b" => b_count = b_count.saturating_sub(1),
                    "/i" => i_count = i_count.saturating_sub(1),
                    "/s" => s_count = s_count.saturating_sub(1),
                    "/u" => u_count = u_count.saturating_sub(1),
                    i if i.get(0..4) == Some("mark") => {
                        marks.push(
                            i.get(5..)
                                .and_then(|color| Srgba::hex(color).map(Color::from).ok())
                                .unwrap_or_else(|| {
                                    warn!("unrecognised mark color `{i}`");
                                    let mut mark_color = color;
                                    mark_color.set_alpha(color.alpha() * 0.5);
                                    mark_color
                                }),
                        );
                    }
                    "/mark" => {
                        marks.pop();
                    }
                    i if i.get(0..5) == Some("color") => {
                        override_colors.push(
                            i.get(6..)
                                .and_then(|color| Srgba::hex(color).map(Color::from).ok())
                                .unwrap_or_else(|| {
                                    warn!("unrecognised text color `{i}`");
                                    color
                                }),
                        );
                    }
                    "/color" => {
                        override_colors.pop();
                    }
                    i if i.get(0..4) == Some("link") => match i.get(5..) {
                        Some(label) => link_data.push(label.to_owned()),
                        None => warn!("text contains `link` tag without label"),
                    },
                    "/link" => {
                        link_data.pop();
                    }
                    other => {
                        let tag = if let Some((tag, _)) = other.split_once("=") {
                            tag
                        } else {
                            other
                        };
                        if !unrecognized_tags.contains(tag) {
                            unrecognized_tags.insert(tag.to_owned());
                            warn!("unrecognised text tag `{tag}`");
                        }
                    }
                }
                section_start = section_start + close + 1;
            } else {
                break;
            }
        }

        let weight = match (b_count, i_count) {
            (0, 0) => WeightName::Regular,
            (0, _) => WeightName::Italic,
            (_, 0) => WeightName::Bold,
            (_, _) => WeightName::BoldItalic,
        };

        let mut maybe_extras: Option<TextExtras> = None;

        if s_count > 0 {
            maybe_extras.get_or_insert_default().strike = true;
        }
        if u_count > 0 {
            maybe_extras.get_or_insert_default().underline = true;
        }
        if let Some(mark) = marks.last().as_ref() {
            maybe_extras.get_or_insert_default().mark = Some(**mark);
        }

        let font = TextFont {
            font: fonts.face(family, weight),
            font_size: font_size * FONT_SIZE_SCALE,
            line_height,
            ..Default::default()
        };

        let color = TextColor(override_colors.last().copied().unwrap_or(color));

        let section_end = text[section_start..]
            .char_indices()
            .find(|(_, c)| *c == '<')
            .map(|(ix, _)| section_start + ix.max(1))
            .unwrap_or(text.len());

        // gather emoji ranges
        let mut span_start = section_start;
        let mut emoji_span = None;

        for (char_index, grapheme) in text[section_start..section_end].grapheme_indices(true) {
            let char_is_emoji = emojis::get(grapheme).is_some();

            match emoji_span {
                Some(span_is_emoji) if span_is_emoji == char_is_emoji => continue,
                None => {
                    emoji_span = Some(char_is_emoji);
                    continue;
                }
                Some(span_is_emoji) => {
                    let use_color = match span_is_emoji {
                        true => TextColor(Color::WHITE),
                        _ => color,
                    };

                    spans.push((
                        TextSpan::new(&text[span_start..section_start + char_index]),
                        font.clone(),
                        use_color,
                        maybe_extras,
                    ));
                    span_start = section_start + char_index;
                    emoji_span = Some(char_is_emoji);
                }
            }
        }

        // add last range
        let last_color = match emoji_span {
            Some(true) => TextColor(Color::WHITE),
            _ => color,
        };
        spans.push((
            TextSpan::new(&text[span_start..section_end]),
            font,
            last_color,
            maybe_extras,
        ));

        if let Some(link) = link_data.last().cloned() {
            links.push((span_index, link));
        }

        section_start = section_end;
        span_index += 1;
    }

    (spans, links)
}

pub fn make_text_section(
    text: &str,
    font_size: f32,
    color: Color,
    family: &TextFontFamily,
    fonts: &mut SceneFontServer,
    justify: JustifyText,
    wrapping: bool,
    unrecognized_tags: &mut UnrecognisedTags,
) -> (impl Bundle, Vec<(usize, String)>) {
    let (spans, links) = build_text_spans(text, font_size, color, family, fonts, unrecognized_tags);

    let f = move |parent: &mut RelatedSpawner<ChildOf>| {
        for (span, font, color, maybe_extras) in spans {
            let mut child = parent.spawn((span, font, color));
            if let Some(extras) = maybe_extras {
                child.insert(extras);
            }
        }
    };

    (
        (
            Text::default(),
            TextLayout::new(
                justify,
                if wrapping {
                    LineBreak::WordOrCharacter
                } else {
                    LineBreak::NoWrap
                },
            ),
            Children::spawn(SpawnWith(f)),
        ),
        links,
    )
}
