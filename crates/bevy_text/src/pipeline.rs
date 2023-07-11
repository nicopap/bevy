use ab_glyph::{Font as AbFont, PxScale, ScaleFont};
use bevy_asset::{Assets, Handle, HandleId};
use bevy_ecs::component::Component;
use bevy_ecs::system::Resource;
use bevy_math::Vec2;
use bevy_render::texture::Image;
use bevy_sprite::TextureAtlas;
use bevy_utils::HashMap;

use glyph_brush_layout::{FontId, GlyphPositioner, SectionGeometry, SectionText, ToSectionText};

use crate::{
    error::TextError, glyph_brush::GlyphBrush, scale_value, BreakLineOn, Font, FontAtlasSet,
    FontAtlasWarning, PositionedGlyph, Text, TextAlignment, TextSection, TextSettings,
    YAxisOrientation,
};

#[derive(Default, Resource)]
pub struct TextPipeline {
    brush: GlyphBrush,
    map_font_id: HashMap<HandleId, FontId>,
}

/// Render information for a corresponding [`Text`](crate::Text) component.
///
///  Contains scaled glyphs and their size. Generated via [`TextPipeline::queue_text`].
#[derive(Component, Clone, Default, Debug)]
pub struct TextLayoutInfo {
    pub glyphs: Vec<PositionedGlyph>,
    pub size: Vec2,
}

impl TextPipeline {
    pub fn get_or_insert_font_id(&mut self, handle: &Handle<Font>, font: &Font) -> FontId {
        let brush = &mut self.brush;
        *self
            .map_font_id
            .entry(handle.id())
            .or_insert_with(|| brush.add_font(handle.clone(), font.font.clone()))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn queue_text(
        &mut self,
        fonts: &Assets<Font>,
        sections: &[TextSection],
        scale_factor: f64,
        text_alignment: TextAlignment,
        linebreak_behavior: BreakLineOn,
        bounds: Vec2,
        font_atlas_set_storage: &mut Assets<FontAtlasSet>,
        texture_atlases: &mut Assets<TextureAtlas>,
        textures: &mut Assets<Image>,
        text_settings: &TextSettings,
        font_atlas_warning: &mut FontAtlasWarning,
        y_axis_orientation: YAxisOrientation,
    ) -> Result<TextLayoutInfo, TextError> {
        let mut scaled_fonts = Vec::with_capacity(sections.len());
        let sections = sections
            .iter()
            .map(|section| {
                let font = fonts
                    .get(&section.style.font)
                    .ok_or(TextError::NoSuchFont)?;
                let font_id = self.get_or_insert_font_id(&section.style.font, font);
                let font_size = scale_value(section.style.font_size, scale_factor);

                scaled_fonts.push(ab_glyph::Font::as_scaled(&font.font, font_size));

                let section = SectionText {
                    font_id,
                    scale: PxScale::from(font_size),
                    text: &section.value,
                };

                Ok(section)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let section_glyphs =
            self.brush
                .compute_glyphs(&sections, bounds, text_alignment, linebreak_behavior)?;

        if section_glyphs.is_empty() {
            return Ok(TextLayoutInfo::default());
        }

        let mut min_x: f32 = std::f32::MAX;
        let mut min_y: f32 = std::f32::MAX;
        let mut max_x: f32 = std::f32::MIN;
        let mut max_y: f32 = std::f32::MIN;

        for sg in &section_glyphs {
            let scaled_font = scaled_fonts[sg.section_index];
            let glyph = &sg.glyph;
            // The fonts use a coordinate system increasing upwards so ascent is a positive value
            // and descent is negative, but Bevy UI uses a downwards increasing coordinate system,
            // so we have to subtract from the baseline position to get the minimum and maximum values.
            min_x = min_x.min(glyph.position.x);
            min_y = min_y.min(glyph.position.y - scaled_font.ascent());
            max_x = max_x.max(glyph.position.x + scaled_font.h_advance(glyph.id));
            max_y = max_y.max(glyph.position.y - scaled_font.descent());
        }

        let size = Vec2::new(max_x - min_x, max_y - min_y);

        let glyphs = self.brush.process_glyphs(
            section_glyphs,
            &sections,
            font_atlas_set_storage,
            fonts,
            texture_atlases,
            textures,
            text_settings,
            font_atlas_warning,
            y_axis_orientation,
        )?;

        Ok(TextLayoutInfo { glyphs, size })
    }
}

#[derive(Debug, Clone)]
pub struct TextMeasureSection {
    pub text: Box<str>,
    pub scale: f32,
    pub font_id: FontId,
}

#[derive(Debug, Clone, Default)]
pub struct TextMeasureInfo {
    pub fonts: Box<[ab_glyph::FontArc]>,
    pub sections: Box<[TextMeasureSection]>,
    pub text_alignment: TextAlignment,
    pub linebreak_behavior: glyph_brush_layout::BuiltInLineBreaker,
    pub min: Vec2,
    pub max: Vec2,
}

impl TextMeasureInfo {
    pub fn from_text(
        text: &Text,
        fonts: &Assets<Font>,
        scale_factor: f64,
    ) -> Result<TextMeasureInfo, TextError> {
        let sections = &text.sections;
        for section in sections {
            if !fonts.contains(&section.style.font) {
                return Err(TextError::NoSuchFont);
            }
        }
        let (auto_fonts, sections) = sections
            .iter()
            .enumerate()
            .map(|(i, section)| {
                // SAFETY: we exited early earlier in this function if
                // one of the fonts was missing.
                let font = unsafe { fonts.get(&section.style.font).unwrap_unchecked() };
                (
                    font.font.clone(),
                    TextMeasureSection {
                        font_id: FontId(i),
                        scale: scale_value(section.style.font_size, scale_factor),
                        text: section.value.clone().into_boxed_str(),
                    },
                )
            })
            .unzip();

        Ok(Self::new(
            auto_fonts,
            sections,
            text.alignment,
            text.linebreak_behavior.into(),
        ))
    }
    fn new(
        fonts: Vec<ab_glyph::FontArc>,
        sections: Vec<TextMeasureSection>,
        text_alignment: TextAlignment,
        linebreak_behavior: glyph_brush_layout::BuiltInLineBreaker,
    ) -> Self {
        let mut info = Self {
            fonts: fonts.into_boxed_slice(),
            sections: sections.into_boxed_slice(),
            text_alignment,
            linebreak_behavior,
            min: Vec2::ZERO,
            max: Vec2::ZERO,
        };

        let min = info.compute_size(Vec2::new(0.0, f32::INFINITY));
        let max = info.compute_size(Vec2::INFINITY);
        info.min = min;
        info.max = max;
        info
    }

    pub fn compute_size(&self, bounds: Vec2) -> Vec2 {
        let sections = &self.sections;
        let geom = SectionGeometry {
            bounds: (bounds.x, bounds.y),
            ..Default::default()
        };
        let section_glyphs = glyph_brush_layout::Layout::default()
            .h_align(self.text_alignment.into())
            .line_breaker(self.linebreak_behavior)
            .calculate_glyphs(&self.fonts, &geom, sections);

        let mut min_x: f32 = std::f32::MAX;
        let mut min_y: f32 = std::f32::MAX;
        let mut max_x: f32 = std::f32::MIN;
        let mut max_y: f32 = std::f32::MIN;

        for sg in section_glyphs {
            let font = &self.fonts[sg.section_index];
            let font_size = self.sections[sg.section_index].scale;
            let scaled_font = font.into_scaled(font_size);

            let glyph = &sg.glyph;
            // The fonts use a coordinate system increasing upwards so ascent is a positive value
            // and descent is negative, but Bevy UI uses a downwards increasing coordinate system,
            // so we have to subtract from the baseline position to get the minimum and maximum values.
            min_x = min_x.min(glyph.position.x);
            min_y = min_y.min(glyph.position.y - scaled_font.ascent());
            max_x = max_x.max(glyph.position.x + scaled_font.h_advance(glyph.id));
            max_y = max_y.max(glyph.position.y - scaled_font.descent());
        }

        Vec2::new(max_x - min_x, max_y - min_y)
    }
}
impl ToSectionText for TextMeasureSection {
    #[inline(always)]
    fn to_section_text(&self) -> SectionText<'_> {
        SectionText {
            text: &self.text,
            scale: PxScale::from(self.scale),
            font_id: self.font_id,
        }
    }
}
