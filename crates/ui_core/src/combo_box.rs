use anyhow::anyhow;
use bevy::{math::Vec3Swizzles, prelude::*, window::PrimaryWindow};
use bevy_dui::{DuiProps, DuiRegistry, DuiTemplate};
use bevy_egui::{egui, EguiContext};

use crate::{
    ui_actions::{DataChanged, On},
    Blocker,
};

#[derive(Component, Debug)]
pub struct ComboBox {
    pub empty_text: String,
    pub options: Vec<String>,
    pub selected: isize,
    pub allow_null: bool,
    pub disabled: bool,
}

impl ComboBox {
    pub fn new(
        empty_text: String,
        options: impl IntoIterator<Item = impl Into<String>>,
        allow_null: bool,
        disabled: bool,
        initial_selection: Option<isize>,
    ) -> Self {
        Self {
            empty_text,
            options: options.into_iter().map(Into::into).collect(),
            selected: initial_selection.unwrap_or(-1),
            allow_null,
            disabled,
        }
    }

    pub fn selected(&self) -> Option<&String> {
        if self.selected == -1 {
            None
        } else {
            self.options.get(self.selected as usize)
        }
    }
}

pub struct ComboBoxPlugin;

impl Plugin for ComboBoxPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, update_comboboxen);
    }
}

fn setup(mut dui: ResMut<DuiRegistry>) {
    dui.register_template("combo-box", DuiComboBoxTemplate);
}

#[allow(clippy::type_complexity)]
fn update_comboboxen(
    mut commands: Commands,
    mut egui_ctx: Query<&mut EguiContext, With<PrimaryWindow>>,
    mut combos: Query<(Entity, &mut ComboBox, &Style, &Node, &GlobalTransform), Without<Blocker>>,
    mut blocker: Local<Option<Entity>>,
    mut blocker_display: Query<&mut Style, With<Blocker>>,
    mut blocker_active: Local<bool>,
) {
    let Ok(mut ctx) = egui_ctx.get_single_mut() else {
        return;
    };
    let ctx = ctx.get_mut();
    let blocker = *blocker.get_or_insert_with(|| {
        commands
            .spawn((
                NodeBundle {
                    style: Style {
                        position_type: PositionType::Absolute,
                        display: Display::None,
                        left: Val::Px(0.0),
                        right: Val::Px(0.0),
                        top: Val::Px(0.0),
                        bottom: Val::Px(0.0),
                        ..Default::default()
                    },
                    focus_policy: bevy::ui::FocusPolicy::Block,
                    z_index: ZIndex::Global(100),
                    ..Default::default()
                },
                Blocker,
            ))
            .id()
    });
    let mut popup_active = false;

    for (entity, mut combo, style, node, transform) in combos.iter_mut() {
        let center = transform.translation().xy() / ctx.zoom_factor();
        let size = node.size() / ctx.zoom_factor();
        let topleft = center - size / 2.0;

        if matches!(style.display, Display::Flex) {
            egui::Window::new(format!("{entity:?}"))
                .fixed_pos(topleft.to_array())
                .fixed_size(size.to_array())
                .frame(egui::Frame::none())
                .title_bar(false)
                .enabled(!combo.disabled)
                .show(ctx, |ui| {
                    let initial_selection = combo.selected;
                    let selected_text = if combo.selected == -1 {
                        &combo.empty_text
                    } else {
                        combo
                            .options
                            .get(combo.selected as usize)
                            .unwrap_or(&combo.empty_text)
                    };

                    let style = ui.style_mut();
                    style.visuals.widgets.active.weak_bg_fill =
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 25);
                    style.visuals.widgets.hovered.weak_bg_fill =
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 50);
                    style.visuals.widgets.inactive.weak_bg_fill =
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128);

                    egui::ComboBox::from_id_source(entity)
                        .selected_text(selected_text)
                        .wrap(false)
                        .width(size.x)
                        .show_ui(ui, |ui| {
                            // split borrow
                            let ComboBox {
                                ref options,
                                ref mut selected,
                                ..
                            } = &mut *combo;

                            for (i, label) in options.iter().enumerate() {
                                ui.selectable_value(selected, i as isize, label);
                            }
                        });

                    if ui.memory(|mem| mem.any_popup_open()) {
                        popup_active = true;
                    }

                    if combo.selected != initial_selection || combo.selected == -1 {
                        if combo.selected == -1 {
                            combo.selected = 0;
                        }
                        commands.entity(entity).try_insert(DataChanged);
                    }
                });
        }
    }

    if popup_active != *blocker_active {
        blocker_display.get_mut(blocker).unwrap().display = if popup_active {
            Display::Flex
        } else {
            Display::None
        };
        *blocker_active = popup_active;
    }
}

pub struct DuiComboBoxTemplate;

impl DuiTemplate for DuiComboBoxTemplate {
    fn render(
        &self,
        commands: &mut bevy::ecs::system::EntityCommands,
        mut props: bevy_dui::DuiProps,
        _: &mut bevy_dui::DuiContext,
    ) -> Result<bevy_dui::NodeMap, anyhow::Error> {
        let combobox = ComboBox {
            empty_text: props.take::<String>("empty-text")?.unwrap_or_default(),
            options: props
                .take::<Vec<String>>("options")?
                .ok_or(anyhow!("no options for combobox"))?,
            selected: props.take::<isize>("selected")?.unwrap_or(-1),
            allow_null: props.take_bool_like("allow-null")?.unwrap_or(false),
            disabled: props.take_bool_like("disabled")?.unwrap_or(false),
        };
        commands.insert(combobox);

        if let Some(onchanged) = props.take::<On<DataChanged>>("onchanged")? {
            commands.insert(onchanged);
        }

        Ok(Default::default())
    }
}

pub trait PropsExt {
    fn take_bool_like(&mut self, label: &str) -> Result<Option<bool>, anyhow::Error>;
}

impl PropsExt for DuiProps {
    fn take_bool_like(&mut self, label: &str) -> Result<Option<bool>, anyhow::Error> {
        if let Ok(value) = self.take::<bool>(label) {
            return Ok(value);
        }

        if let Ok(Some(value)) = self.take::<String>(label) {
            match value.chars().next() {
                Some('f') | Some('F') => Ok(Some(false)),
                Some('t') | Some('T') => Ok(Some(true)),
                Some(_) => Err(anyhow!(
                    "unrecognised bool string value `{value}` for key `{label}`"
                )),
                None => Ok(None),
            }
        } else {
            Err(anyhow!("unrecognised bool-like type for key `{label}`"))
        }
    }
}
