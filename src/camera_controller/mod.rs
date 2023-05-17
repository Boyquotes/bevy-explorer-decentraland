// copied from bevy/examples/tools/scene_viewer

//! A freecam-style camera controller plugin.
//! To use in your own application:
//! - Copy the code for the `CameraControllerPlugin` and add the plugin to your App.
//! - Attach the `CameraController` component to an entity with a `Camera3dBundle`.

use bevy::{input::mouse::MouseMotion, math::Vec3Swizzles, prelude::*};
use bevy::{input::mouse::MouseWheel, window::CursorGrabMode};
use bevy_console::ConsoleOpen;

use std::f32::consts::*;

use crate::{
    avatar::movement::Velocity,
    scene_runner::{PrimaryUser, SceneSets},
    PrimaryCamera,
};

/// Based on Valorant's default sensitivity, not entirely sure why it is exactly 1.0 / 180.0,
/// but I'm guessing it is a misunderstanding between degrees/radians and then sticking with
/// it because it felt nice.
pub const RADIANS_PER_DOT: f32 = 1.0 / 180.0;

#[derive(Component)]
pub struct CameraController {
    pub enabled: bool,
    pub initialized: bool,
    pub sensitivity: f32,
    pub key_forward: KeyCode,
    pub key_back: KeyCode,
    pub key_left: KeyCode,
    pub key_right: KeyCode,
    pub key_up: KeyCode,
    pub key_down: KeyCode,
    pub key_run: KeyCode,
    pub key_roll_left: KeyCode,
    pub key_roll_right: KeyCode,
    pub mouse_key_enable_mouse: MouseButton,
    pub keyboard_key_enable_mouse: KeyCode,
    pub walk_speed: f32,
    pub run_speed: f32,
    pub friction: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub roll: f32,
    pub velocity: Vec3,
}

impl Default for CameraController {
    fn default() -> Self {
        Self {
            enabled: true,
            initialized: false,
            sensitivity: 1.0,
            key_forward: KeyCode::W,
            key_back: KeyCode::S,
            key_left: KeyCode::A,
            key_right: KeyCode::D,
            key_up: KeyCode::E,
            key_down: KeyCode::Q,
            key_run: KeyCode::LShift,
            key_roll_left: KeyCode::T,
            key_roll_right: KeyCode::Y,
            mouse_key_enable_mouse: MouseButton::Right,
            keyboard_key_enable_mouse: KeyCode::M,
            walk_speed: 1.5,
            run_speed: 6.0,
            friction: 0.5,
            pitch: 0.0,
            yaw: 0.0,
            roll: 0.0,
            velocity: Vec3::ZERO,
        }
    }
}

pub struct CameraControllerPlugin;

impl Plugin for CameraControllerPlugin {
    fn build(&self, app: &mut App) {
        app.add_system(
            camera_controller
                .in_set(SceneSets::Input)
                .run_if(|console_open: Res<ConsoleOpen>| !console_open.open),
        );
        app.add_system(hide_player_in_first_person);
        app.insert_resource(CameraDistance(1.0));
    }
}

#[derive(Resource, Default)]
pub struct CameraDistance(pub f32);

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn camera_controller(
    time: Res<Time>,
    mut windows: Query<&mut Window>,
    mut mouse_events: EventReader<MouseMotion>,
    mut wheel_events: EventReader<MouseWheel>,
    mouse_button_input: Res<Input<MouseButton>>,
    key_input: Res<Input<KeyCode>>,
    mut move_toggled: Local<bool>,
    mut camera: Query<(&mut Transform, &mut CameraController), With<PrimaryCamera>>,
    mut player: Query<(&mut Transform, &mut Velocity), (With<PrimaryUser>, Without<PrimaryCamera>)>,
    mut camera_distance: ResMut<CameraDistance>,
) {
    let dt = time.delta_seconds();

    if let (
        Ok((mut player_transform, mut player_velocity)),
        Ok((mut camera_transform, mut options)),
    ) = (player.get_single_mut(), camera.get_single_mut())
    {
        if !options.initialized {
            let (yaw, pitch, roll) = camera_transform.rotation.to_euler(EulerRot::YXZ);
            options.yaw = yaw;
            options.pitch = pitch;
            options.roll = roll;
            options.initialized = true;
        }
        if !options.enabled {
            return;
        }

        // Handle key input
        let mut axis_input = Vec3::ZERO;
        if key_input.pressed(options.key_forward) {
            axis_input.z += 1.0;
        }
        if key_input.pressed(options.key_back) {
            axis_input.z -= 1.0;
        }
        if key_input.pressed(options.key_right) {
            axis_input.x += 1.0;
        }
        if key_input.pressed(options.key_left) {
            axis_input.x -= 1.0;
        }
        if key_input.pressed(options.key_up) {
            axis_input.y += 1.0;
        }
        if key_input.pressed(options.key_down) {
            axis_input.y -= 1.0;
        }
        if key_input.just_pressed(options.keyboard_key_enable_mouse) {
            *move_toggled = !*move_toggled;
        }

        if key_input.pressed(options.key_roll_left) {
            options.roll += dt * 1.0;
        } else if key_input.pressed(options.key_roll_right) {
            options.roll -= dt * 1.0;
        } else if options.roll > 0.0 {
            options.roll = (options.roll - dt * 0.25).max(0.0);
        } else {
            options.roll = (options.roll + dt * 0.25).min(0.0);
        }

        // Apply movement update
        if axis_input != Vec3::ZERO {
            let max_speed = if key_input.pressed(options.key_run) {
                options.run_speed
            } else {
                options.walk_speed
            };
            options.velocity = axis_input.normalize() * max_speed;
        } else {
            let friction = options.friction.clamp(0.0, 1.0);
            options.velocity *= 1.0 - friction;
            if options.velocity.length_squared() < 1e-6 {
                options.velocity = Vec3::ZERO;
            }
        }

        let ground = Vec3::X + Vec3::Z;
        let forward = (camera_transform.forward() * ground).normalize();
        let right = (camera_transform.right() * ground).normalize();
        if options.velocity.length() > 0.0 {
            let direction_vector = options.velocity.x * right + options.velocity.z * forward;
            player_transform.translation += direction_vector * dt;
            if direction_vector.length() > 0.0 {
                let target_direction = Transform::default()
                    .looking_at(direction_vector, Vec3::Y)
                    .rotation;
                player_transform.rotation =
                    player_transform.rotation.lerp(target_direction, dt * 10.0);
            }
        }

        player_velocity.0 = options.velocity.xz().length();

        // Handle mouse input
        let mut mouse_delta = Vec2::ZERO;
        if mouse_button_input.pressed(options.mouse_key_enable_mouse) || *move_toggled {
            for mut window in &mut windows {
                if !window.focused {
                    continue;
                }

                window.cursor.grab_mode = CursorGrabMode::Locked;
                window.cursor.visible = false;
            }

            for mouse_event in mouse_events.iter() {
                mouse_delta += mouse_event.delta;
            }
        }
        if mouse_button_input.just_released(options.mouse_key_enable_mouse)
            || (key_input.just_pressed(options.keyboard_key_enable_mouse) && !*move_toggled)
        {
            for mut window in &mut windows {
                window.cursor.grab_mode = CursorGrabMode::None;
                window.cursor.visible = true;
            }
        }

        if let Some(event) = wheel_events.iter().last() {
            if event.y > 0.0 {
                camera_distance.0 = 0f32.max((camera_distance.0 - 0.05) * 0.9);
            } else if event.y < 0.0 {
                camera_distance.0 = 1f32.min((camera_distance.0 / 0.9) + 0.05);
            }
        }

        // if mouse_delta != Vec2::ZERO {
        // Apply look update
        options.pitch = (options.pitch - mouse_delta.y * RADIANS_PER_DOT * options.sensitivity)
            .clamp(-PI / 2., PI / 2.);
        options.yaw -= mouse_delta.x * RADIANS_PER_DOT * options.sensitivity;
        camera_transform.rotation =
            Quat::from_euler(EulerRot::YXZ, options.yaw, options.pitch, options.roll);
        // }

        camera_transform.translation = player_transform.translation
            + Vec3::Y * 2.0
            + camera_transform
                .rotation
                .mul_vec3(Vec3::Z * 5.0 * camera_distance.0);
    }
}

fn hide_player_in_first_person(
    distance: Res<CameraDistance>,
    mut player: Query<&mut Visibility, With<PrimaryUser>>,
) {
    if let Ok(mut vis) = player.get_single_mut() {
        if distance.0 < 0.1 && *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        } else if distance.0 > 0.1 && *vis != Visibility::Inherited {
            *vis = Visibility::Inherited;
        }
    }
}
