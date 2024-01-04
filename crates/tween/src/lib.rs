use bevy::prelude::*;
use common::sets::SceneSets;
use dcl::interface::{ComponentPosition, CrdtType};
use dcl_component::{
    proto_components::sdk::components::{
        pb_tween::Mode, EasingFunction, PbTween, PbTweenState, TweenStateStatus,
    },
    transform_and_parent::DclTransformAndParent,
    SceneComponentId,
};

use scene_runner::{
    renderer_context::RendererSceneContext, update_world::AddCrdtInterfaceExt, ContainerEntity,
    SceneEntity,
};

#[derive(Component, Debug)]
pub struct Tween(PbTween);

impl From<PbTween> for Tween {
    fn from(value: PbTween) -> Self {
        Self(value)
    }
}

impl Tween {
    fn apply(&self, time: f32, transform: &mut Transform) {
        use simple_easing::*;
        use EasingFunction::*;
        let f = match self.0.easing_function() {
            EfLinear => linear,
            EfEaseinquad => quad_in,
            EfEaseoutquad => quad_out,
            EfEasequad => quad_in_out,
            EfEaseinsine => sine_in,
            EfEaseoutsine => sine_out,
            EfEasesine => sine_in_out,
            EfEaseinexpo => expo_in,
            EfEaseoutexpo => expo_out,
            EfEaseexpo => expo_in_out,
            EfEaseinelastic => elastic_in,
            EfEaseoutelastic => elastic_out,
            EfEaseelastic => elastic_in_out,
            EfEaseinbounce => bounce_in,
            EfEaseoutbounce => bounce_out,
            EfEasebounce => bounce_in_out,
            EfEaseincubic => cubic_in,
            EfEaseoutcubic => cubic_out,
            EfEasecubic => cubic_in_out,
            EfEaseinquart => quart_in,
            EfEaseoutquart => quart_out,
            EfEasequart => quart_in_out,
            EfEaseinquint => quint_in,
            EfEaseoutquint => quint_out,
            EfEasequint => quint_in_out,
            EfEaseincirc => circ_in,
            EfEaseoutcirc => circ_out,
            EfEasecirc => circ_in_out,
            EfEaseinback => back_in,
            EfEaseoutback => back_out,
            EfEaseback => back_in_out,
        };

        let ease_value = f(time);

        match &self.0.mode {
            Some(Mode::Move(data)) => {
                let start = data.start.unwrap_or_default().world_vec_to_vec3();
                let end = data.end.unwrap_or_default().world_vec_to_vec3();

                if data.face_direction == Some(true) && time == 0.0 {
                    let direction = end - start;
                    if direction == Vec3::ZERO {
                        // can't look nowhere
                    } else if Vec3::new(1.0, 0.0, 1.0) != Vec3::ZERO {
                        // randomly assume +z is up for a vertical movement
                        transform.look_at(end - start, Vec3::Z);
                    } else {
                        transform.look_at(end - start, Vec3::Y);
                    }
                }

                transform.translation = start + (end - start) * ease_value;
            }
            Some(Mode::Rotate(data)) => {
                let start: Quat = data.start.unwrap_or_default().into();
                let end = data.end.unwrap_or_default().into();
                transform.rotation = start.slerp(end, ease_value);
            }
            Some(Mode::Scale(data)) => {
                let start = data.start.unwrap_or_default().abs_vec_to_vec3();
                let end = data.end.unwrap_or_default().abs_vec_to_vec3();
                transform.scale = start + ((end - start) * ease_value);
            }
            _ => {}
        }
    }
}

#[derive(Component, Debug, PartialEq)]
pub struct TweenState(PbTweenState);

pub struct TweenPlugin;

impl Plugin for TweenPlugin {
    fn build(&self, app: &mut bevy::prelude::App) {
        app.add_crdt_lww_component::<PbTween, Tween>(
            SceneComponentId::TWEEN,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(Update, update_tween.in_set(SceneSets::PostLoop));
    }
}

#[allow(clippy::type_complexity)]
pub fn update_tween(
    mut commands: Commands,
    time: Res<Time>,
    mut tweens: Query<(
        Entity,
        &ContainerEntity,
        &Parent,
        Ref<Tween>,
        &mut Transform,
        Option<&mut TweenState>,
    )>,
    mut scenes: Query<&mut RendererSceneContext>,
    parents: Query<&SceneEntity>,
) {
    for (ent, scene_ent, parent, tween, mut transform, state) in tweens.iter_mut() {
        let playing = tween.0.playing.unwrap_or(true);
        let delta = if playing {
            time.delta_seconds() * 1000.0 / tween.0.duration
        } else {
            0.0
        };

        let updated_time = if tween.is_changed() {
            tween.0.current_time.unwrap_or(0.0)
        } else {
            state
                .as_ref()
                .map(|state| state.0.current_time + delta)
                .unwrap_or(0.0)
                .min(1.0)
        };

        let updated_status = if playing && updated_time == 1.0 {
            TweenStateStatus::TsCompleted
        } else if playing {
            TweenStateStatus::TsActive
        } else {
            TweenStateStatus::TsPaused
        };

        let updated_state = TweenState(PbTweenState {
            state: updated_status as i32,
            current_time: updated_time,
        });

        if state.as_deref() != Some(&updated_state) {
            let Ok(mut scene) = scenes.get_mut(scene_ent.root) else {
                continue;
            };

            scene.update_crdt(
                SceneComponentId::TWEEN_STATE,
                CrdtType::LWW_ENT,
                scene_ent.container_id,
                &updated_state.0,
            );

            if let Some(mut state) = state {
                state.0 = updated_state.0;
            } else {
                commands.entity(ent).try_insert(updated_state);
            }

            tween.apply(updated_time, &mut transform);

            let Ok(parent) = parents.get(parent.get()) else {
                warn!("no parent for tweened ent");
                continue;
            };

            scene.update_crdt(
                SceneComponentId::TRANSFORM,
                CrdtType::LWW_ENT,
                scene_ent.container_id,
                &DclTransformAndParent::from_bevy_transform_and_parent(&transform, parent.id),
            );
        }
    }
}
