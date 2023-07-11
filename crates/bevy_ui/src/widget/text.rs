use crate::{ContentSize, Node, UiScale};
use bevy_asset::Assets;
use bevy_ecs::{
    prelude::{Component, DetectChanges},
    query::With,
    reflect::ReflectComponent,
    system::{Local, Query, Res, ResMut},
    world::{Mut, Ref},
};
use bevy_math::Vec2;
use bevy_reflect::{std_traits::ReflectDefault, Reflect};
use bevy_render::texture::Image;
use bevy_sprite::TextureAtlas;
use bevy_text::{
    BreakLineOn, Font, FontAtlasSet, FontAtlasWarning, Text, TextError, TextLayoutInfo,
    TextMeasureInfo, TextPipeline, TextSettings, YAxisOrientation,
};
use bevy_window::{PrimaryWindow, Window};

/// Text system flags
///
/// Used internally by [`measure_text_system`] and [`text_system`] to schedule text for processing.
#[derive(Component, Debug, Clone, Reflect)]
#[reflect(Component, Default)]
pub struct TextFlags {
    /// If set a new measure function for the text node will be created
    needs_new_measure_func: bool,
    /// If set the text will be recomputed
    needs_recompute: bool,
}

impl Default for TextFlags {
    fn default() -> Self {
        Self {
            needs_new_measure_func: true,
            needs_recompute: true,
        }
    }
}

#[inline(always)]
fn create_text_measure(
    fonts: &Assets<Font>,
    scale_factor: f64,
    text: Ref<Text>,
    mut content_size: Mut<ContentSize>,
    mut text_flags: Mut<TextFlags>,
) {
    match TextMeasureInfo::from_text(&text, fonts, scale_factor) {
        Ok(measure) => {
            if text.linebreak_behavior == BreakLineOn::NoWrap {
                let size = measure.max;
                *content_size = ContentSize::Fixed { size };
            } else {
                *content_size = ContentSize::Text(measure);
            }

            // Text measure func created successfully, so set `TextFlags` to schedule a recompute
            text_flags.needs_new_measure_func = false;
            text_flags.needs_recompute = true;
        }
        Err(TextError::NoSuchFont) => {
            // Try again next frame
            text_flags.needs_new_measure_func = true;
        }
        Err(e @ TextError::FailedToAddGlyph(_)) => {
            panic!("Fatal error when processing text: {e}.");
        }
    };
}

/// Creates a `Measure` for text nodes that allows the UI to determine the appropriate amount of space
/// to provide for the text given the fonts, the text itself and the constraints of the layout.
pub fn measure_text_system(
    mut last_scale_factor: Local<f64>,
    fonts: Res<Assets<Font>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    ui_scale: Res<UiScale>,
    mut text_query: Query<(Ref<Text>, &mut ContentSize, &mut TextFlags), With<Node>>,
) {
    let window_scale_factor = windows
        .get_single()
        .map(|window| window.resolution.scale_factor())
        .unwrap_or(1.);

    let scale_factor = ui_scale.scale * window_scale_factor;

    #[allow(clippy::float_cmp)]
    if *last_scale_factor == scale_factor {
        // scale factor unchanged, only create new measure funcs for modified text
        for (text, content_size, text_flags) in text_query.iter_mut() {
            if text.is_changed() || text_flags.needs_new_measure_func {
                create_text_measure(&fonts, scale_factor, text, content_size, text_flags);
            }
        }
    } else {
        // scale factor changed, create new measure funcs for all text
        *last_scale_factor = scale_factor;

        for (text, content_size, text_flags) in text_query.iter_mut() {
            create_text_measure(&fonts, scale_factor, text, content_size, text_flags);
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn queue_text(
    fonts: &Assets<Font>,
    text_pipeline: &mut TextPipeline,
    font_atlas_warning: &mut FontAtlasWarning,
    font_atlas_set_storage: &mut Assets<FontAtlasSet>,
    texture_atlases: &mut Assets<TextureAtlas>,
    textures: &mut Assets<Image>,
    text_settings: &TextSettings,
    scale_factor: f64,
    text: &Text,
    node: Ref<Node>,
    mut text_flags: Mut<TextFlags>,
    mut text_layout_info: Mut<TextLayoutInfo>,
) {
    // Skip the text node if it is waiting for a new measure func
    if !text_flags.needs_new_measure_func {
        let physical_node_size = if text.linebreak_behavior == BreakLineOn::NoWrap {
            // With `NoWrap` set, no constraints are placed on the width of the text.
            Vec2::splat(f32::INFINITY)
        } else {
            // `scale_factor` is already multiplied by `UiScale`
            node.physical_size(scale_factor, 1.)
        };

        match text_pipeline.queue_text(
            fonts,
            &text.sections,
            scale_factor,
            text.alignment,
            text.linebreak_behavior,
            physical_node_size,
            font_atlas_set_storage,
            texture_atlases,
            textures,
            text_settings,
            font_atlas_warning,
            YAxisOrientation::TopToBottom,
        ) {
            Err(TextError::NoSuchFont) => {
                // There was an error processing the text layout, try again next frame
                text_flags.needs_recompute = true;
            }
            Err(e @ TextError::FailedToAddGlyph(_)) => {
                panic!("Fatal error when processing text: {e}.");
            }
            Ok(info) => {
                *text_layout_info = info;
                text_flags.needs_recompute = false;
            }
        }
    }
}

/// Updates the layout and size information whenever the text or style is changed.
/// This information is computed by the `TextPipeline` on insertion, then stored.
///
/// ## World Resources
///
/// [`ResMut<Assets<Image>>`](Assets<Image>) -- This system only adds new [`Image`] assets.
/// It does not modify or observe existing ones.
#[allow(clippy::too_many_arguments)]
pub fn text_system(
    mut textures: ResMut<Assets<Image>>,
    mut last_scale_factor: Local<f64>,
    fonts: Res<Assets<Font>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    text_settings: Res<TextSettings>,
    mut font_atlas_warning: ResMut<FontAtlasWarning>,
    ui_scale: Res<UiScale>,
    mut texture_atlases: ResMut<Assets<TextureAtlas>>,
    mut font_atlas_set_storage: ResMut<Assets<FontAtlasSet>>,
    mut text_pipeline: ResMut<TextPipeline>,
    mut text_query: Query<(Ref<Node>, &Text, &mut TextLayoutInfo, &mut TextFlags)>,
) {
    // TODO: Support window-independent scaling: https://github.com/bevyengine/bevy/issues/5621
    let window_scale_factor = windows
        .get_single()
        .map(|window| window.resolution.scale_factor())
        .unwrap_or(1.);

    let scale_factor = ui_scale.scale * window_scale_factor;

    if *last_scale_factor == scale_factor {
        // Scale factor unchanged, only recompute text for modified text nodes
        for (node, text, text_layout_info, text_flags) in text_query.iter_mut() {
            if node.is_changed() || text_flags.needs_recompute {
                queue_text(
                    &fonts,
                    &mut text_pipeline,
                    &mut font_atlas_warning,
                    &mut font_atlas_set_storage,
                    &mut texture_atlases,
                    &mut textures,
                    &text_settings,
                    scale_factor,
                    text,
                    node,
                    text_flags,
                    text_layout_info,
                );
            }
        }
    } else {
        // Scale factor changed, recompute text for all text nodes
        *last_scale_factor = scale_factor;

        for (node, text, text_layout_info, text_flags) in text_query.iter_mut() {
            queue_text(
                &fonts,
                &mut text_pipeline,
                &mut font_atlas_warning,
                &mut font_atlas_set_storage,
                &mut texture_atlases,
                &mut textures,
                &text_settings,
                scale_factor,
                text,
                node,
                text_flags,
                text_layout_info,
            );
        }
    }
}
