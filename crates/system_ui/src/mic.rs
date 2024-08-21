use av::microphone::MicState;
use bevy::prelude::*;
use common::structs::ToolTips;
use comms::{Transport, TransportType};
use ui_core::ui_actions::{Click, HoverEnter, HoverExit, On};

use crate::chat::BUTTON_SCALE;

pub struct MicUiPlugin;

#[derive(Component)]
pub struct MicUiMarker;

#[derive(Resource)]
pub struct MicImages {
    inactive: Handle<Image>,
    on: Handle<Image>,
    off: Handle<Image>,
}

impl Plugin for MicUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
        app.add_systems(Update, update_mic_ui);

        let asset_server = app.world().resource::<AssetServer>();
        app.insert_resource(MicImages {
            inactive: asset_server.load("images/mic_button_inactive.png"),
            on: asset_server.load("images/mic_button_on.png"),
            off: asset_server.load("images/mic_button_off.png"),
        });
    }
}

fn setup(mut commands: Commands, images: Res<MicImages>) {
    // profile button
    commands.spawn((
        ImageBundle {
            image: images.inactive.clone_weak().into(),
            style: Style {
                position_type: PositionType::Absolute,
                top: Val::VMin(BUTTON_SCALE * 1.5),
                right: Val::VMin(BUTTON_SCALE * 0.5),
                width: Val::VMin(BUTTON_SCALE),
                height: Val::VMin(BUTTON_SCALE),
                ..Default::default()
            },
            focus_policy: bevy::ui::FocusPolicy::Block,
            ..Default::default()
        },
        Interaction::default(),
        On::<Click>::new(|mut mic_state: ResMut<MicState>| mic_state.enabled = !mic_state.enabled),
        On::<HoverEnter>::new(
            |mut tooltip: ResMut<ToolTips>, transport: Query<&Transport>, state: Res<MicState>| {
                let transport_available = transport
                    .iter()
                    .any(|t| t.transport_type == TransportType::Livekit);
                tooltip.0.insert(
                    "mic",
                    vec![(
                        "LCtrl : Push to talk".to_owned(),
                        transport_available && state.available,
                    )],
                );
            },
        ),
        On::<HoverExit>::new(|mut tooltip: ResMut<ToolTips>| {
            tooltip.0.remove("mic");
        }),
        MicUiMarker,
    ));
}

fn update_mic_ui(
    mut mic_state: ResMut<MicState>,
    transport: Query<&Transport>,
    mut button: Query<&mut UiImage, With<MicUiMarker>>,
    mut pressed: Local<bool>,
    input: Res<ButtonInput<KeyCode>>,
    mic_images: Res<MicImages>,
) {
    let mic_available = mic_state.available;
    let transport_available = transport
        .iter()
        .any(|t| t.transport_type == TransportType::Livekit);

    if mic_available && transport_available {
        if mic_state.enabled {
            *button.single_mut() = mic_images.on.clone_weak().into();
        } else {
            *button.single_mut() = mic_images.off.clone_weak().into();
        }
    } else {
        *button.single_mut() = mic_images.inactive.clone_weak().into();
    }

    if input.pressed(KeyCode::ControlLeft) != *pressed {
        mic_state.enabled = !mic_state.enabled;
        *pressed = !*pressed;
    }
}
