use std::collections::BTreeSet;

use bevy::{
    prelude::*,
    ui::FocusPolicy,
    utils::{HashMap, HashSet},
};
use input_manager::MouseInteractionComponent;

use crate::{
    renderer_context::RendererSceneContext, update_scene::pointer_results::UiPointerTarget,
    ContainingScene, SceneEntity, SceneSets,
};
use common::structs::PrimaryUser;
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::{
        self,
        common::{texture_union, BorderRect, TextureUnion},
        sdk::components::{
            self, PbUiBackground, PbUiDropdown, PbUiDropdownResult, PbUiInput, PbUiInputResult,
            PbUiText, PbUiTransform, YgAlign, YgDisplay, YgFlexDirection, YgJustify, YgOverflow,
            YgPositionType, YgUnit, YgWrap,
        },
    },
    SceneComponentId, SceneEntityId,
};
use ui_core::{
    combo_box::ComboBox,
    nine_slice::{Ui9Slice, Ui9SliceSet},
    textentry::TextEntry,
    ui_actions::{DataChanged, HoverEnter, HoverExit, On},
    ui_builder::SpawnSpacer,
    TITLE_TEXT_STYLE,
};

use super::{material::TextureResolver, pointer_events::PointerEvents, AddCrdtInterfaceExt};

pub struct SceneUiPlugin;

#[derive(Debug, Copy, Clone)]
struct Size {
    width: Val,
    height: Val,
}

// macro helpers to convert proto format to bevy format for val, size, rect
macro_rules! val {
    ($pb:ident, $u:ident, $v:ident, $d:expr) => {
        match $pb.$u() {
            YgUnit::YguUndefined => $d,
            YgUnit::YguAuto => Val::Auto,
            YgUnit::YguPoint => Val::Px($pb.$v),
            YgUnit::YguPercent => Val::Percent($pb.$v),
        }
    };
}

macro_rules! size {
    ($pb:ident, $wu:ident, $w:ident, $hu:ident, $h:ident, $d:expr) => {{
        Size {
            width: val!($pb, $wu, $w, $d),
            height: val!($pb, $hu, $h, $d),
        }
    }};
}

macro_rules! rect {
    ($pb:ident, $lu:ident, $l:ident, $ru:ident, $r:ident, $tu:ident, $t:ident, $bu:ident, $b:ident, $d:expr) => {
        UiRect {
            left: val!($pb, $lu, $l, $d),
            right: val!($pb, $ru, $r, $d),
            top: val!($pb, $tu, $t, $d),
            bottom: val!($pb, $bu, $b, $d),
        }
    };
}

#[derive(Component, Debug, Clone)]
pub struct UiTransform {
    parent: SceneEntityId,
    right_of: SceneEntityId,
    align_content: AlignContent,
    align_items: AlignItems,
    wrap: FlexWrap,
    shrink: f32,
    position_type: PositionType,
    align_self: AlignSelf,
    flex_direction: FlexDirection,
    justify_content: JustifyContent,
    overflow: Overflow,
    display: Display,
    basis: Val,
    grow: f32,
    size: Size,
    min_size: Size,
    max_size: Size,
    position: UiRect,
    margin: UiRect,
    padding: UiRect,
}

impl From<PbUiTransform> for UiTransform {
    fn from(value: PbUiTransform) -> Self {
        Self {
            parent: SceneEntityId::from_proto_u32(value.parent as u32),
            right_of: SceneEntityId::from_proto_u32(value.right_of as u32),
            align_content: match value.align_content() {
                YgAlign::YgaAuto |
                YgAlign::YgaBaseline | // baseline is invalid for align content
                YgAlign::YgaFlexStart => AlignContent::FlexStart,
                YgAlign::YgaCenter => AlignContent::Center,
                YgAlign::YgaFlexEnd => AlignContent::FlexEnd,
                YgAlign::YgaStretch => AlignContent::Stretch,
                YgAlign::YgaSpaceBetween => AlignContent::SpaceBetween,
                YgAlign::YgaSpaceAround => AlignContent::SpaceAround,
            },
            align_items: match value.align_items() {
                YgAlign::YgaAuto |
                YgAlign::YgaSpaceBetween | // invalid
                YgAlign::YgaSpaceAround | // invalid
                YgAlign::YgaStretch => AlignItems::Stretch,
                YgAlign::YgaFlexStart => AlignItems::FlexStart,
                YgAlign::YgaCenter => AlignItems::Center,
                YgAlign::YgaFlexEnd => AlignItems::FlexEnd,
                YgAlign::YgaBaseline => AlignItems::Baseline,
            },
            wrap: match value.flex_wrap() {
                YgWrap::YgwNoWrap => FlexWrap::NoWrap,
                YgWrap::YgwWrap => FlexWrap::Wrap,
                YgWrap::YgwWrapReverse => FlexWrap::WrapReverse,
            },
            shrink: value.flex_shrink.unwrap_or(1.0),
            position_type: match value.position_type() {
                YgPositionType::YgptRelative => PositionType::Relative,
                YgPositionType::YgptAbsolute => PositionType::Absolute,
            },
            align_self: match value.align_self() {
                YgAlign::YgaSpaceBetween | // invalid
                YgAlign::YgaSpaceAround | // invalid
                YgAlign::YgaAuto => AlignSelf::Auto,
                YgAlign::YgaFlexStart => AlignSelf::FlexStart,
                YgAlign::YgaCenter => AlignSelf::Center,
                YgAlign::YgaFlexEnd => AlignSelf::FlexEnd,
                YgAlign::YgaStretch => AlignSelf::Stretch,
                YgAlign::YgaBaseline => AlignSelf::Baseline,
            },
            flex_direction: match value.flex_direction() {
                YgFlexDirection::YgfdRow => FlexDirection::Row,
                YgFlexDirection::YgfdColumn => FlexDirection::Column,
                YgFlexDirection::YgfdColumnReverse => FlexDirection::ColumnReverse,
                YgFlexDirection::YgfdRowReverse => FlexDirection::RowReverse,
            },
            justify_content: match value.justify_content() {
                YgJustify::YgjFlexStart => JustifyContent::FlexStart,
                YgJustify::YgjCenter => JustifyContent::Center,
                YgJustify::YgjFlexEnd => JustifyContent::FlexEnd,
                YgJustify::YgjSpaceBetween => JustifyContent::SpaceBetween,
                YgJustify::YgjSpaceAround => JustifyContent::SpaceAround,
                YgJustify::YgjSpaceEvenly => JustifyContent::SpaceEvenly,
            },
            overflow: match value.overflow() {
                YgOverflow::YgoVisible => Overflow::DEFAULT,
                YgOverflow::YgoHidden => Overflow::clip(),
                YgOverflow::YgoScroll => {
                    // TODO: map to scroll area
                    warn!("ui overflow scroll not implemented");
                    Overflow::clip()
                }
            },
            display: match value.display() {
                YgDisplay::YgdFlex => Display::Flex,
                YgDisplay::YgdNone => Display::None,
            },
            basis: val!(value, flex_basis_unit, flex_basis, Val::Auto),
            grow: value.flex_grow,
            size: size!(value, width_unit, width, height_unit, height, Val::Auto),
            min_size: size!(
                value,
                min_width_unit,
                min_width,
                min_height_unit,
                min_height,
                Val::Auto
            ),
            max_size: size!(
                value,
                max_width_unit,
                max_width,
                max_height_unit,
                max_height,
                Val::Auto
            ),
            position: rect!(
                value,
                position_left_unit,
                position_left,
                position_right_unit,
                position_right,
                position_top_unit,
                position_top,
                position_bottom_unit,
                position_bottom,
                Val::Auto
            ),
            margin: rect!(
                value,
                margin_left_unit,
                margin_left,
                margin_right_unit,
                margin_right,
                margin_top_unit,
                margin_top,
                margin_bottom_unit,
                margin_bottom,
                Val::Px(0.0)
            ),
            padding: rect!(
                value,
                padding_left_unit,
                padding_left,
                padding_right_unit,
                padding_right,
                padding_top_unit,
                padding_top,
                padding_bottom_unit,
                padding_bottom,
                Val::Px(0.0)
            ),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum BackgroundTextureMode {
    NineSlices(BorderRect),
    Stretch(BorderRect),
    Center,
}

#[derive(Component, Clone, Debug)]
pub struct BackgroundTexture {
    tex: TextureUnion,
    mode: BackgroundTextureMode,
}

#[derive(Component, Clone)]
pub struct UiBackground {
    color: Option<Color>,
    texture: Option<BackgroundTexture>,
}

impl From<PbUiBackground> for UiBackground {
    fn from(value: PbUiBackground) -> Self {
        let texture_mode = value.texture_mode();
        Self {
            color: value.color.map(Into::into),
            texture: value.texture.map(|tex| {
                let mode = match texture_mode {
                    components::BackgroundTextureMode::NineSlices => {
                        BackgroundTextureMode::NineSlices(value.texture_slices.unwrap_or(
                            BorderRect {
                                top: 1.0 / 3.0,
                                bottom: 1.0 / 3.0,
                                left: 1.0 / 3.0,
                                right: 1.0 / 3.0,
                            },
                        ))
                    }
                    components::BackgroundTextureMode::Center => BackgroundTextureMode::Center,
                    components::BackgroundTextureMode::Stretch => {
                        // the uvs array seems to contain [x-, y-, x-, y+, x+, y+, x+, y-]
                        // let's pick ... [1 - 4]              ^^^^^^^^^^^^^^
                        let mut uvs = BorderRect::default();
                        let mut iter = value.uvs.iter().skip(1);
                        if let Some(ymin) = iter.next() {
                            uvs.bottom = ymin.clamp(0.0, 1.0);
                        }
                        if let Some(xmin) = iter.next() {
                            uvs.left = xmin.clamp(0.0, 1.0);
                        }
                        if let Some(ymax) = iter.next() {
                            uvs.top = 1.0 - ymax.clamp(uvs.bottom, 1.0);
                        }
                        if let Some(xmax) = iter.next() {
                            uvs.right = 1.0 - xmax.clamp(uvs.left, 1.0);
                        }

                        BackgroundTextureMode::Stretch(uvs)
                    }
                };

                BackgroundTexture { tex, mode }
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

#[derive(Component, Clone)]
pub struct UiText {
    pub text: String,
    pub color: Color,
    pub h_align: TextAlignment,
    pub v_align: VAlign,
    pub font: proto_components::sdk::components::common::Font,
    pub font_size: f32,
}

impl From<PbUiText> for UiText {
    fn from(value: PbUiText) -> Self {
        let text_align = value
            .text_align
            .map(|_| value.text_align())
            .unwrap_or(components::common::TextAlignMode::TamMiddleCenter);

        Self {
            text: value.value.clone(),
            color: value.color.map(Into::into).unwrap_or(Color::WHITE),
            h_align: match text_align {
                components::common::TextAlignMode::TamTopLeft
                | components::common::TextAlignMode::TamMiddleLeft
                | components::common::TextAlignMode::TamBottomLeft => TextAlignment::Left,
                components::common::TextAlignMode::TamTopCenter
                | components::common::TextAlignMode::TamMiddleCenter
                | components::common::TextAlignMode::TamBottomCenter => TextAlignment::Center,
                components::common::TextAlignMode::TamTopRight
                | components::common::TextAlignMode::TamMiddleRight
                | components::common::TextAlignMode::TamBottomRight => TextAlignment::Right,
            },
            v_align: match text_align {
                components::common::TextAlignMode::TamTopLeft
                | components::common::TextAlignMode::TamTopCenter
                | components::common::TextAlignMode::TamTopRight => VAlign::Top,
                components::common::TextAlignMode::TamMiddleLeft
                | components::common::TextAlignMode::TamMiddleCenter
                | components::common::TextAlignMode::TamMiddleRight => VAlign::Middle,
                components::common::TextAlignMode::TamBottomLeft
                | components::common::TextAlignMode::TamBottomCenter
                | components::common::TextAlignMode::TamBottomRight => VAlign::Bottom,
            },
            font: value.font(),
            font_size: value.font_size.unwrap_or(10) as f32,
        }
    }
}

#[derive(Component)]
pub struct UiInput(PbUiInput);

impl From<PbUiInput> for UiInput {
    fn from(value: PbUiInput) -> Self {
        Self(value)
    }
}

#[derive(Component)]
pub struct UiInputPersistentState {
    content: String,
}

#[derive(Component)]
pub struct UiDropdown(PbUiDropdown);

impl From<PbUiDropdown> for UiDropdown {
    fn from(value: PbUiDropdown) -> Self {
        Self(value)
    }
}

#[derive(Component)]
pub struct UiDropdownPersistentState(isize);

impl Plugin for SceneUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_lww_component::<PbUiTransform, UiTransform>(
            SceneComponentId::UI_TRANSFORM,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbUiBackground, UiBackground>(
            SceneComponentId::UI_BACKGROUND,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbUiText, UiText>(
            SceneComponentId::UI_TEXT,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbUiInput, UiInput>(
            SceneComponentId::UI_INPUT,
            ComponentPosition::EntityOnly,
        );
        app.add_crdt_lww_component::<PbUiDropdown, UiDropdown>(
            SceneComponentId::UI_DROPDOWN,
            ComponentPosition::EntityOnly,
        );

        app.add_systems(Update, init_scene_ui_root.in_set(SceneSets::PostInit));
        app.add_systems(
            Update,
            (update_scene_ui_components, layout_scene_ui)
                .chain()
                .in_set(SceneSets::PostLoop),
        );

        // we need to make sure commands are run before 9slice layouting
        app.add_systems(
            Update,
            apply_deferred
                .after(SceneSets::PostLoop)
                .before(Ui9SliceSet),
        );
    }
}

#[derive(Component, Default)]
pub struct SceneUiData {
    nodes: BTreeSet<Entity>,
    relayout: bool,
    current_node: Option<Entity>,
}

fn init_scene_ui_root(
    mut commands: Commands,
    scenes: Query<Entity, (With<RendererSceneContext>, Without<SceneUiData>)>,
) {
    for scene_ent in scenes.iter() {
        commands
            .entity(scene_ent)
            .try_insert(SceneUiData::default());
    }
}

#[allow(clippy::type_complexity)]
fn update_scene_ui_components(
    new_entities: Query<
        (Entity, &SceneEntity),
        Or<(Changed<UiTransform>, Changed<UiText>, Changed<UiBackground>)>,
    >,
    mut ui_roots: Query<&mut SceneUiData>,
) {
    for (ent, scene_id) in new_entities.iter() {
        let Ok(mut ui_data) = ui_roots.get_mut(scene_id.root) else {
            warn!("scene root missing for {:?}", scene_id.root);
            continue;
        };

        ui_data.nodes.insert(ent);
        ui_data.relayout = true;
    }
}

#[derive(Component)]
pub struct SceneUiRoot(Entity);

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn layout_scene_ui(
    mut commands: Commands,
    mut scene_uis: Query<(Entity, &mut SceneUiData)>,
    player: Query<Entity, With<PrimaryUser>>,
    containing_scene: ContainingScene,
    ui_nodes: Query<(
        &SceneEntity,
        &UiTransform,
        Option<&UiBackground>,
        Option<&UiText>,
        Option<&PointerEvents>,
        Option<&UiInput>,
        Option<&UiDropdown>,
    )>,
    mut ui_target: ResMut<UiPointerTarget>,
    current_uis: Query<(Entity, &SceneUiRoot)>,
    ui_input_state: Query<&UiInputPersistentState>,
    ui_dropdown_state: Query<&UiDropdownPersistentState>,
    resolver: TextureResolver,
) {
    let current_scenes = player
        .get_single()
        .ok()
        .map(|p| containing_scene.get(p))
        .unwrap_or_default();

    // remove any non-current uis
    for (ent, ui_root) in &current_uis {
        if !current_scenes.contains(&ui_root.0) {
            commands.entity(ent).despawn_recursive();
        }
    }

    for (ent, mut ui_data) in scene_uis.iter_mut() {
        if current_scenes.contains(&ent) {
            if ui_data.relayout || ui_data.current_node.is_none() {
                // clear any existing ui target
                *ui_target = UiPointerTarget::None;

                // remove any old instance of the ui
                if let Some(node) = ui_data.current_node.take() {
                    commands.entity(node).despawn_recursive();
                }

                // collect ui data
                let mut deleted_nodes = HashSet::default();
                let mut unprocessed_uis =
                    HashMap::from_iter(ui_data.nodes.iter().flat_map(|node| {
                        match ui_nodes.get(*node) {
                            Ok((
                                scene_entity,
                                transform,
                                maybe_background,
                                maybe_text,
                                maybe_pointer_events,
                                maybe_ui_input,
                                maybe_dropdown,
                            )) => Some((
                                scene_entity.id,
                                (
                                    *node,
                                    transform.clone(),
                                    maybe_background,
                                    maybe_text,
                                    maybe_pointer_events,
                                    maybe_ui_input,
                                    maybe_dropdown,
                                ),
                            )),
                            Err(_) => {
                                // remove this node
                                deleted_nodes.insert(*node);
                                None
                            }
                        }
                    }));

                // remove any dead nodes
                ui_data.nodes.retain(|node| !deleted_nodes.contains(node));

                let mut processed_nodes = HashMap::new();

                let root = commands
                    .spawn((
                        NodeBundle {
                            style: Style {
                                position_type: PositionType::Absolute,
                                left: Val::Px(0.0),
                                right: Val::Px(0.0),
                                top: Val::Px(0.0),
                                bottom: Val::Px(0.0),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        SceneUiRoot(ent),
                    ))
                    .id();
                processed_nodes.insert(SceneEntityId::ROOT, root);

                let mut modified = true;
                while modified && !unprocessed_uis.is_empty() {
                    modified = false;
                    unprocessed_uis.retain(
                        |scene_id,
                         (
                            node,
                            ui_transform,
                            maybe_background,
                            maybe_text,
                            maybe_pointer_events,
                            maybe_ui_input,
                            maybe_dropdown,
                        )| {
                            // if our rightof is not added, we can't process this node
                            if !processed_nodes.contains_key(&ui_transform.right_of) {
                                debug!(
                                    "can't place {} with ro {}",
                                    scene_id, ui_transform.right_of
                                );
                                return true;
                            }

                            // if our parent is not added, we can't process this node
                            let Some(parent) = processed_nodes.get(&ui_transform.parent) else {
                                debug!("can't place {} with parent {}", scene_id, ui_transform.parent);
                                return true;
                            };

                            // we can process this node
                            let mut style = Style {
                                align_content: ui_transform.align_content,
                                align_items: ui_transform.align_items,
                                flex_wrap: ui_transform.wrap,
                                position_type: ui_transform.position_type,
                                flex_shrink: ui_transform.shrink,
                                align_self: ui_transform.align_self,
                                flex_direction: ui_transform.flex_direction,
                                justify_content: ui_transform.justify_content,
                                overflow: ui_transform.overflow,
                                display: ui_transform.display,
                                flex_basis: ui_transform.basis,
                                flex_grow: ui_transform.grow,
                                width: ui_transform.size.width,
                                height: ui_transform.size.height,
                                min_width: ui_transform.min_size.width,
                                min_height: ui_transform.min_size.height,
                                max_width: ui_transform.max_size.width,
                                max_height: ui_transform.max_size.height,
                                left: ui_transform.position.left,
                                right: ui_transform.position.right,
                                top: ui_transform.position.top,
                                bottom: ui_transform.position.bottom,
                                margin: ui_transform.margin,
                                padding: ui_transform.padding,
                                ..Default::default()
                            };
                            debug!("{:?} style: {:?}", scene_id, style);
                            commands.entity(*parent).with_children(|commands| {
                                let mut ent_cmds = &mut commands.spawn(NodeBundle::default());

                                if let Some(background) = maybe_background {
                                    if let Some(color) = background.color {
                                        ent_cmds = ent_cmds.insert(BackgroundColor(color));
                                    }

                                    if let Some(texture) = background.texture.as_ref() {
                                        let image = texture.tex.tex.as_ref().and_then(|tex| resolver.resolve_texture(ent, tex).ok());

                                        let texture_mode = match texture.tex.tex {
                                            Some(texture_union::Tex::Texture(_)) => texture.mode,
                                            _ => BackgroundTextureMode::Stretch(BorderRect::default()),
                                        };

                                        if let Some(image) = image {
                                            match texture_mode {
                                                BackgroundTextureMode::NineSlices(rect) => {
                                                    ent_cmds.insert(Ui9Slice{
                                                        image: image.image,
                                                        center_region: rect.into(),
                                                    });
                                                },
                                                BackgroundTextureMode::Stretch(ref uvs) => {
                                                    let mid_width = 1.0 - uvs.right - uvs.left;
                                                    let mid_height = 1.0 - uvs.top - uvs.bottom;

                                                    ent_cmds.with_children(|c| {
                                                        c.spawn(NodeBundle {
                                                            style: Style {
                                                                position_type: PositionType::Absolute,
                                                                width: Val::Percent(100.0),
                                                                height: Val::Percent(100.0),
                                                                overflow: Overflow::clip(),
                                                                ..Default::default()
                                                            },
                                                            ..Default::default()
                                                        }).with_children(|c| {
                                                            c.spawn(ImageBundle{
                                                                style: Style {
                                                                    position_type: PositionType::Absolute,
                                                                    left: Val::Percent(-100.0 * uvs.left / mid_width),
                                                                    right: Val::Percent(-100.0 * uvs.right / mid_width),
                                                                    top: Val::Percent(-100.0 * uvs.top / mid_height),
                                                                    bottom: Val::Percent(-100.0 * uvs.bottom / mid_height),
                                                                    ..Default::default()
                                                                },
                                                                image: UiImage {
                                                                    texture: image.image,
                                                                    flip_x: false,
                                                                    flip_y: false,
                                                                },
                                                                ..Default::default()
                                                            });
                                                        });
                                                    });
                                                }
                                                BackgroundTextureMode::Center => {
                                                    ent_cmds = ent_cmds.with_children(|c| {
                                                        // make a stretchy grid
                                                        c.spawn(NodeBundle {
                                                            style: Style {
                                                                position_type: PositionType::Absolute,
                                                                left: Val::Px(0.0),
                                                                right: Val::Px(0.0),
                                                                top: Val::Px(0.0),
                                                                bottom: Val::Px(0.0),
                                                                justify_content: JustifyContent::Center,
                                                                overflow: Overflow::clip(),
                                                                width: Val::Percent(100.0),
                                                                ..Default::default()
                                                            },
                                                            ..Default::default()
                                                        })
                                                        .with_children(|c| {
                                                            c.spacer();
                                                            c.spawn(NodeBundle {
                                                                style: Style {
                                                                    flex_direction:
                                                                        FlexDirection::Column,
                                                                    justify_content:
                                                                        JustifyContent::Center,
                                                                    overflow: Overflow::clip(),
                                                                    height: Val::Percent(100.0),
                                                                    ..Default::default()
                                                                },
                                                                ..Default::default()
                                                            })
                                                            .with_children(|c| {
                                                                c.spacer();
                                                                c.spawn(ImageBundle {
                                                                    style: Style {
                                                                        overflow: Overflow::clip(),
                                                                        ..Default::default()
                                                                    },
                                                                    image: UiImage {
                                                                        texture: image.image,
                                                                        flip_x: false,
                                                                        flip_y: false,
                                                                    },
                                                                    ..Default::default()
                                                                });
                                                                c.spacer();
                                                            });
                                                            c.spacer();
                                                        });
                                                    });
                                                }
                                            }
                                        } else {
                                            warn!(
                                                "failed to load ui image from content map: {:?}",
                                                texture
                                            );
                                        }
                                    }
                                }

                                if let Some(text) = maybe_text {
                                    ent_cmds = ent_cmds.with_children(|c| {
                                        c.spawn(NodeBundle {
                                            style: Style {
                                                flex_direction: FlexDirection::Column,
                                                width: Val::Percent(100.0),
                                                height: Val::Percent(100.0),
                                                ..Default::default()
                                            },
                                            ..Default::default()
                                        })
                                            .with_children(|c| {
                                                if text.v_align != VAlign::Top {
                                                    c.spacer();
                                                }

                                                c.spawn(NodeBundle {
                                                    style: Style {
                                                        flex_direction: FlexDirection::Row,
                                                        width: Val::Percent(100.0),
                                                        ..Default::default()
                                                    },
                                                    ..Default::default()
                                                }).with_children(|c| {
                                                    if text.h_align != TextAlignment::Left {
                                                        c.spacer();
                                                    }

                                                    c.spawn(TextBundle {
                                                        text: Text {
                                                            sections: vec![TextSection::new(
                                                                text.text.clone(),
                                                                TextStyle {
                                                                    font: TITLE_TEXT_STYLE
                                                                        .get()
                                                                        .unwrap()
                                                                        .clone()
                                                                        .font, // TODO fix this
                                                                    font_size: text.font_size,
                                                                    color: text.color,
                                                                },
                                                            )],
                                                            alignment: text.h_align,
                                                            linebreak_behavior:
                                                                bevy::text::BreakLineOn::NoWrap,
                                                        },
                                                        z_index: ZIndex::Local(1),
                                                        ..Default::default()
                                                    });

                                                    if text.h_align != TextAlignment::Right {
                                                        c.spacer();
                                                    }
                                                });

                                                if text.v_align != VAlign::Bottom {
                                                    c.spacer();
                                                }
                                            },
                                        );
                                    });
                                }

                                if maybe_pointer_events.is_some() {
                                    let node = *node;

                                    ent_cmds = ent_cmds.insert((
                                        MouseInteractionComponent,
                                        FocusPolicy::Block,
                                        Interaction::default(),
                                        On::<HoverEnter>::new(move |mut ui_target: ResMut<UiPointerTarget>| {
                                            *ui_target = UiPointerTarget::Some(node);
                                        }),
                                        On::<HoverExit>::new(move |mut ui_target: ResMut<UiPointerTarget>| {
                                            if *ui_target == UiPointerTarget::Some(node) {
                                                *ui_target = UiPointerTarget::None;
                                            };
                                        }),
                                    ));
                                }

                                if let Some(input) = maybe_ui_input {
                                    debug!("input: {:?}", input.0);

                                    let node = *node;
                                    let ui_node = ent_cmds.id();
                                    let scene_id = *scene_id;

                                    let content = match ui_input_state.get(node) {
                                        Ok(state) => state.content.clone(),
                                        Err(_) => input.0.value.clone().unwrap_or_default(),
                                    };
                                    let font_size = input.0.font_size.unwrap_or(12);

                                    //ensure we use max width if not given
                                    if style.width == Val::Px(0.0) {
                                        style.width = Val::Percent(100.0);
                                    }
                                    //and some size if not given
                                    if style.height == Val::Px(0.0) {
                                        style.height = Val::Px(font_size as f32 * 1.3);
                                    }

                                    ent_cmds = ent_cmds.insert((
                                        FocusPolicy::Block,
                                        Interaction::default(),
                                        TextEntry {
                                            hint_text: input.0.placeholder.to_owned(),
                                            enabled: !input.0.disabled,
                                            content,
                                            accept_line: false,
                                            font_size,
                                            id_entity: Some(node),
                                            ..Default::default()
                                        },
                                        On::<DataChanged>::new(move |
                                            mut commands: Commands,
                                            entry: Query<&TextEntry>,
                                            mut context: Query<&mut RendererSceneContext>,
                                            time: Res<Time>,
                                        | {
                                            let Ok(entry) = entry.get(ui_node) else {
                                                warn!("failed to get text node on UiInput update");
                                                return;
                                            };
                                            let Ok(mut context) = context.get_mut(ent) else {
                                                warn!("failed to get context on UiInput update");
                                                return;
                                            };

                                            context.update_crdt(SceneComponentId::UI_INPUT_RESULT, CrdtType::LWW_ENT, scene_id, &PbUiInputResult {
                                                value: entry.content.clone(),
                                                is_submit: None,
                                            });
                                            context.last_action_event = Some(time.elapsed_seconds());
                                            // store persistent state to the scene entity
                                            commands.entity(node).try_insert(UiInputPersistentState{content: entry.content.clone()});
                                        }),
                                    ))
                                }

                                if let Some(dropdown) = maybe_dropdown {
                                    let node = *node;
                                    let ui_node = ent_cmds.id();
                                    let scene_id = *scene_id;

                                    let initial_selection = match (ui_dropdown_state.get(node), dropdown.0.accept_empty) {
                                        (Ok(state), _) => Some(state.0),
                                        (_, false) => Some(dropdown.0.selected_index.unwrap_or(0) as isize),
                                        (_, true) => None,
                                    };

                                    ent_cmds.insert((
                                        ComboBox::new(dropdown.0.empty_label.clone().unwrap_or_default(), &dropdown.0.options, dropdown.0.accept_empty, dropdown.0.disabled, initial_selection),
                                        On::<DataChanged>::new(move |
                                            mut commands: Commands,
                                            combo: Query<(Entity, &ComboBox)>,
                                            mut context: Query<&mut RendererSceneContext>,
                                            time: Res<Time>,
                                        | {
                                            let Ok((_, combo)) = combo.get(ui_node) else {
                                                warn!("failed to get combo node on UiDropdown update");
                                                return;
                                            };
                                            let Ok(mut context) = context.get_mut(ent) else {
                                                warn!("failed to get context on UiInput update");
                                                return;
                                            };

                                            context.update_crdt(SceneComponentId::UI_DROPDOWN_RESULT, CrdtType::LWW_ENT, scene_id, &PbUiDropdownResult {
                                                value: combo.selected as i32,
                                            });
                                            context.last_action_event = Some(time.elapsed_seconds());
                                            // store persistent state to the scene entity
                                            commands.entity(node).try_insert(UiDropdownPersistentState(combo.selected));
                                        }),
                                    ));
                                }

                                ent_cmds.insert(style);
                                processed_nodes.insert(*scene_id, ent_cmds.id());
                            });

                            // mark to continue and remove from unprocessed
                            modified = true;
                            false
                        },
                    );
                }

                debug!(
                    "made ui; placed: {}, unplaced: {}",
                    processed_nodes.len(),
                    unprocessed_uis.len()
                );
                ui_data.relayout = false;
                ui_data.current_node = Some(root);
            }
        } else {
            ui_data.current_node = None;
        }
    }
}
