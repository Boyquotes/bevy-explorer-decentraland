use std::{collections::VecDeque, time::Duration};

use bevy::{
    animation::RepeatAnimation,
    asset::{LoadState, LoadedFolder},
    gltf::Gltf,
    math::Vec3Swizzles,
    prelude::*,
    utils::HashMap,
};
use bevy_console::ConsoleCommand;
use common::{
    rpc::{RpcCall, RpcEventSender},
    sets::SceneSets,
    structs::PrimaryUser,
};
use comms::{
    chat_marker_things, global_crdt::ChatEvent, profile::{UserProfile, CurrentUserProfile}, NetworkMessage, Transport,
};
use console::DoAddConsoleCommand;
use dcl::interface::ComponentPosition;
use dcl_component::{
    proto_components::{
        kernel::comms::rfc4::{self, Chat},
        sdk::components::{pb_avatar_emote_command::EmoteCommand, PbAvatarEmoteCommand},
    },
    SceneComponentId,
};
use emotes::{AvatarAnimations, AvatarAnimation};
use scene_runner::{
    update_world::{transform_and_parent::ParentPositionSync, AddCrdtInterfaceExt},
    ContainerEntity, ContainingScene,
};

use super::AvatarDynamicState;

use once_cell::sync::Lazy;

struct DefaultAnim {
    male: &'static str,
    female: &'static str,
    repeat: bool,
}

impl DefaultAnim {
    fn new(male: &'static str, female: &'static str, repeat: bool) -> Self {
        Self {
            male,
            female,
            repeat,
        }
    }
}

static DEFAULT_ANIMATION_LOOKUP: Lazy<HashMap<&str, DefaultAnim>> = Lazy::new(|| {
    HashMap::from_iter([
        (
            "handsair",
            DefaultAnim::new("Hands_In_The_Air", "Hands_In_The_Air", false),
        ),
        ("wave", DefaultAnim::new("Wave_Male", "Wave_Female", false)),
        (
            "fistpump",
            DefaultAnim::new("M_FistPump", "F_FistPump", false),
        ),
        (
            "dance",
            DefaultAnim::new("Dance_Male", "Dance_Female", true),
        ),
        (
            "raiseHand",
            DefaultAnim::new("Raise_Hand", "Raise_Hand", false),
        ),
        // "clap" defaults
        (
            "money",
            DefaultAnim::new(
                "Armature|Throw Money-Emote_v02|BaseLayer",
                "Armature|Throw Money-Emote_v02|BaseLayer",
                false,
            ),
        ),
        // "kiss" defaults
        ("headexplode", DefaultAnim::new("explode", "explode", false)),
        // "shrug" defaults
    ])
});

#[derive(Component)]
pub struct AvatarAnimPlayer(pub Entity);

pub struct AvatarAnimationPlugin;

#[derive(Component, Default, Deref, DerefMut, Debug, Clone)]
pub struct EmoteList(VecDeque<PbAvatarEmoteCommand>);

impl EmoteList {
    pub fn new(emote_urn: impl Into<String>) -> Self {
        Self(VecDeque::from_iter([PbAvatarEmoteCommand {
            emote_command: Some(EmoteCommand {
                emote_urn: emote_urn.into(),
                r#loop: false,
            }),
        }]))
    }
}

impl Plugin for AvatarAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.add_crdt_go_component::<PbAvatarEmoteCommand, EmoteList>(
            SceneComponentId::AVATAR_EMOTE_COMMAND,
            ComponentPosition::EntityOnly,
        );
        app.add_systems(
            Update,
            (
                load_animations,
                (read_player_emotes, broadcast_emote, receive_emotes).before(animate),
                animate,
            )
                .in_set(SceneSets::PostLoop),
        );
        app.add_console_command::<EmoteConsoleCommand, _>(emote_console_command);
    }
}

#[derive(Default)]
enum AnimLoadState {
    #[default]
    Init,
    WaitingForFolder(Handle<LoadedFolder>),
    WaitingForGltfs(Vec<Handle<Gltf>>),
    Done,
}

#[allow(clippy::type_complexity)]
fn load_animations(
    asset_server: Res<AssetServer>,
    gltfs: Res<Assets<Gltf>>,
    mut state: Local<AnimLoadState>,
    folders: Res<Assets<LoadedFolder>>,
    mut animations: ResMut<AvatarAnimations>,
) {
    match &mut *state {
        AnimLoadState::Init => {
            *state = AnimLoadState::WaitingForFolder(asset_server.load_folder("animations"));
        }
        AnimLoadState::WaitingForFolder(h_folder) => {
            if asset_server.load_state(h_folder.id()) == LoadState::Loaded {
                let folder = folders.get(h_folder.id()).unwrap();
                *state = AnimLoadState::WaitingForGltfs(
                    folder.handles.iter().map(|h| h.clone().typed()).collect(),
                )
            }
        }
        AnimLoadState::WaitingForGltfs(ref mut h_gltfs) => {
            h_gltfs.retain(
                |h_gltf| match gltfs.get(h_gltf).map(|gltf| &gltf.named_animations) {
                    Some(anims) => {
                        for (name, h_clip) in anims {
                            animations.0.insert(name.clone(), AvatarAnimation {
                                name: name.clone(),
                                description: name.clone(),
                                clip: h_clip.clone(),
                                thumbnail: asset_server.load("images/emote_button.png"),
                            });
                            debug!("added animation {name}");
                        }
                        false
                    }
                    None => true,
                },
            );

            if h_gltfs.is_empty() {
                *state = AnimLoadState::Done;
            }
        }
        AnimLoadState::Done => {}
    }
}

// copy emotes from scene-player entities onto main player entity
fn read_player_emotes(
    mut commands: Commands,
    scene_player_emotes: Query<
        (Entity, &EmoteList, &ParentPositionSync, &ContainerEntity),
        Without<PrimaryUser>,
    >,
    mut player_emotes: Query<Entity, With<PrimaryUser>>,
    containing_scene: ContainingScene,
) {
    let Ok(player) = player_emotes.get_single_mut() else {
        return;
    };
    let containing_scenes = containing_scene.get(player);

    for (scene_ent, emotes, parent, container) in &scene_player_emotes {
        if parent.0 == player {
            commands.entity(scene_ent).remove::<EmoteList>();
            if containing_scenes.contains(&container.root) {
                commands.entity(player).insert(emotes.clone());
            }
        }
    }
}

fn broadcast_emote(
    q: Query<&EmoteList, With<PrimaryUser>>,
    transports: Query<&Transport>,
    mut last: Local<Option<String>>,
    mut count: Local<usize>,
    time: Res<Time>,
    mut senders: Local<Vec<RpcEventSender>>,
    mut subscribe_events: EventReader<RpcCall>,
) {
    // gather any event receivers
    for sender in subscribe_events.read().filter_map(|ev| match ev {
        RpcCall::SubscribePlayerExpression { sender } => Some(sender),
        _ => None,
    }) {
        senders.push(sender.clone());
    }

    for list in q.iter() {
        if let Some(PbAvatarEmoteCommand {
            emote_command: Some(EmoteCommand { emote_urn, .. }),
        }) = list.back()
        {
            if last.as_ref() != Some(emote_urn) {
                *count += 1;
                debug!("sending emote: {emote_urn:?} {}", *count);
                let packet = rfc4::Packet {
                    message: Some(rfc4::packet::Message::Chat(Chat {
                        message: format!("{}{} {}", chat_marker_things::EMOTE, emote_urn, *count),
                        timestamp: time.elapsed_seconds_f64(),
                    })),
                };

                for transport in transports.iter() {
                    let _ = transport
                        .sender
                        .blocking_send(NetworkMessage::reliable(&packet));
                }

                *last = Some(emote_urn.to_owned());

                senders.retain(|sender| {
                    let _ = sender.send(format!("{{ \"expressionId\": \"{emote_urn}\" }}"));
                    !sender.is_closed()
                })
            }
            return;
        }

        *last = None;
    }
}

fn receive_emotes(mut commands: Commands, mut chat_events: EventReader<ChatEvent>) {
    for ev in chat_events
        .read()
        .filter(|e| e.message.starts_with(chat_marker_things::EMOTE))
    {
        if let Some(emote_urn) = ev
            .message
            .strip_prefix(chat_marker_things::EMOTE)
            .unwrap()
            .split(' ')
            .next()
        {
            commands
                .entity(ev.sender)
                .try_insert(EmoteList::new(emote_urn));
        }
    }
}

// TODO this function is a POS
// lots of magic numbers that don't even deserve to be constants, needs reworking
#[allow(clippy::type_complexity)]
fn animate(
    mut avatars: Query<(
        Entity,
        &AvatarAnimPlayer,
        &AvatarDynamicState,
        Option<&mut EmoteList>,
        Option<&UserProfile>,
    )>,
    mut players: Query<&mut AnimationPlayer>,
    animations: Res<AvatarAnimations>,
    mut velocities: Local<HashMap<Entity, Vec3>>,
    mut playing: Local<HashMap<Entity, String>>,
    time: Res<Time>,
    anim_assets: Res<Assets<AnimationClip>>,
) {
    let prior_velocities = std::mem::take(&mut *velocities);
    let prior_playing = std::mem::take(&mut *playing);

    let mut play = |anim: &str, speed: f32, ent: Entity, restart: bool, repeat: bool| -> bool {
        if let Some(clip) = animations.0.get(anim) {
            if let Ok(mut player) = players.get_mut(ent) {
                if restart && player.elapsed() == 0.75 {
                    player.start(clip.clip.clone()).repeat();
                } else if Some(anim) != prior_playing.get(&ent).map(String::as_str) || restart {
                    player.play_with_transition(clip.clip.clone(), Duration::from_millis(100));
                    if repeat {
                        player.repeat();
                    } else {
                        player.set_repeat(RepeatAnimation::Never);
                    }
                    playing.insert(ent, anim.to_owned());
                }

                if anim == "Jump" && player.elapsed() >= 0.75 {
                    player.pause();
                } else {
                    player.resume();
                }

                player.set_speed(speed);
                return player.elapsed() > anim_assets.get(&clip.clip).map_or(f32::MAX, |c| c.duration());
            }
        }

        false
    };

    for (avatar_ent, animplayer_ent, dynamic_state, mut emotes, profile) in avatars.iter_mut() {
        // take a copy of the last entry, remove others
        let mut emote = emotes
            .as_mut()
            .map(|e| {
                let last = e.pop_back();
                e.clear();
                if let Some(last) = last.clone() {
                    e.push_back(last);
                }
                last
            })
            .unwrap_or_default();

        let prior_velocity = prior_velocities
            .get(&avatar_ent)
            .copied()
            .unwrap_or(Vec3::ZERO);
        let ratio = time.delta_seconds().clamp(0.0, 0.1) / 0.1;
        let damped_velocity = dynamic_state.velocity * ratio + prior_velocity * (1.0 - ratio);
        let damped_velocity_len = damped_velocity.xz().length();
        velocities.insert(avatar_ent, damped_velocity);

        if damped_velocity_len > prior_velocity.length() {
            // stop emotes on move
            if let Some(emotes) = emotes.as_mut() {
                emotes.clear();
                emote = None;
            }
        }

        if let Some(PbAvatarEmoteCommand {
            emote_command: Some(EmoteCommand { emote_urn, r#loop }),
        }) = emote
        {
            let is_female = profile.map_or(true, UserProfile::is_female);
            let (emote_urn, repeat) = DEFAULT_ANIMATION_LOOKUP
                .get(emote_urn.as_str())
                .map(|anim| (if is_female { anim.female } else { anim.male }, anim.repeat))
                .unwrap_or((emote_urn.as_str(), r#loop));

            if play(emote_urn, 1.0, animplayer_ent.0, false, repeat) && !repeat {
                // emote has finished, remove from the set so will resume default anim after
                emotes.as_mut().unwrap().clear();
            };
            continue;
        }

        if dynamic_state.ground_height > 0.2 {
            play(
                "Jump",
                1.25,
                animplayer_ent.0,
                dynamic_state.velocity.y > 0.0,
                true,
            );
            continue;
        }

        if damped_velocity_len > 0.1 {
            if damped_velocity_len < 2.0 {
                play(
                    "Walk",
                    damped_velocity_len / 1.5,
                    animplayer_ent.0,
                    false,
                    true,
                );
            } else {
                play(
                    "Run",
                    damped_velocity_len / 4.5,
                    animplayer_ent.0,
                    false,
                    true,
                );
            }
        } else {
            play("Idle_Male", 1.0, animplayer_ent.0, false, true);
        }
    }
}

/// emote
#[derive(clap::Parser, ConsoleCommand)]
#[command(name = "/emote")]
struct EmoteConsoleCommand {
    urn: String,
}

fn emote_console_command(
    mut commands: Commands,
    mut input: ConsoleCommand<EmoteConsoleCommand>,
    player: Query<Entity, With<PrimaryUser>>,
    profile: Res<CurrentUserProfile>,
) {
    if let Some(Ok(command)) = input.take() {
        if let Ok(player) = player.get_single() {
            let mut urn = &command.urn;
            if let Ok(slot) = command.urn.parse::<u32>() {
                if let Some(emote) = profile.profile.as_ref().and_then(|p| p.content.avatar.emotes.as_ref()).and_then(|es| es.iter().find(|e| e.slot == slot)) {
                    urn = &emote.urn;
                }
            }
            
            info!("anim {} -> {}", command.urn, urn);

            commands
                .entity(player)
                .try_insert(EmoteList::new(urn.clone()));
        };
        input.ok();
    }
}
