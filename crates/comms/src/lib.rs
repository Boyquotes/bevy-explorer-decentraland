pub mod archipelago;
pub mod broadcast_position;
pub mod global_crdt;
pub mod livekit_room;
pub mod profile;
pub mod signed_login;
#[cfg(test)]
mod test;
pub mod websocket_room;

use std::marker::PhantomData;

use bevy::{ecs::system::SystemParam, prelude::*};
use bimap::BiMap;
use ethers_core::types::Address;
use signed_login::{SignedLoginPlugin, StartSignedLogin};
use tokio::sync::mpsc::Sender;

use dcl_component::{DclWriter, ToDclWriter};
use ipfs::CurrentRealm;
use wallet::Wallet;

use self::{
    archipelago::{ArchipelagoPlugin, StartArchipelago},
    broadcast_position::BroadcastPositionPlugin,
    global_crdt::GlobalCrdtPlugin,
    livekit_room::{LivekitPlugin, StartLivekit},
    profile::UserProfilePlugin,
    websocket_room::{StartWsRoom, WebsocketRoomPlugin},
};

pub mod chat_marker_things {
    pub const EMOTE: char = '␐';

    pub const ALL: [char; 3] = [EMOTE, '␑', '␆'];
}

pub struct CommsPlugin;

impl Plugin for CommsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            WebsocketRoomPlugin,
            SignedLoginPlugin,
            LivekitPlugin,
            ArchipelagoPlugin,
            BroadcastPositionPlugin,
            GlobalCrdtPlugin,
            UserProfilePlugin,
        ));
        app.add_systems(Update, process_realm_change);
    }
}

pub struct TransportAlias {
    pub adapter: Entity,
    pub alias: u32,
}

#[derive(PartialEq, Eq)]
pub enum TransportType {
    WebsocketRoom,
    Livekit,
    Archipelago,
    Island(String),
}

pub struct NetworkMessage {
    pub data: Vec<u8>,
    pub unreliable: bool,
}

impl NetworkMessage {
    pub fn unreliable<D: ToDclWriter>(message: &D) -> Self {
        let mut data = Vec::new();
        let mut writer = DclWriter::new(&mut data);
        message.to_writer(&mut writer);
        Self {
            data,
            unreliable: true,
        }
    }

    pub fn reliable<D: ToDclWriter>(message: &D) -> Self {
        Self {
            unreliable: false,
            ..Self::unreliable(message)
        }
    }
}

#[derive(Component)]
pub struct Transport {
    pub transport_type: TransportType,
    pub sender: Sender<NetworkMessage>,
    pub foreign_aliases: BiMap<u32, Address>,
}

fn process_realm_change(
    mut commands: Commands,
    realm: Res<CurrentRealm>,
    adapters: Query<Entity, With<Transport>>,
    mut manager: AdapterManager,
    wallet: Res<Wallet>,
) {
    if realm.is_changed() {
        for adapter in adapters.iter() {
            commands.entity(adapter).despawn_recursive();
        }

        if wallet.address().is_none() {
            info!("disconnecting comms, no identity");
            return;
        }

        if let Some(comms) = realm.comms.as_ref() {
            if let Some(fixed_adapter) = comms.fixed_adapter.as_ref() {
                manager.connect(fixed_adapter);
            } else {
                // must be archipelago
                manager.connect(&format!("archipelago:{}", realm.address));
            }
        } else {
            debug!("missing comms!");
        }
    }
}

#[derive(SystemParam)]
pub struct AdapterManager<'w, 's> {
    commands: Commands<'w, 's>,
    ws_room_events: EventWriter<'w, StartWsRoom>,
    livekit_events: EventWriter<'w, StartLivekit>,
    archipelago_events: EventWriter<'w, StartArchipelago>,
    // can't use event writer due to conflict on Res<Events>
    pub signed_login_events: ResMut<'w, Events<StartSignedLogin>>,
    #[system_param(ignore)]
    _p: PhantomData<&'s ()>,
}

impl<'w, 's> AdapterManager<'w, 's> {
    pub fn connect(&mut self, adapter: &str) -> Option<Entity> {
        let Some((protocol, address)) = adapter.split_once(':') else {
            warn!("unrecognised fixed adapter string: {adapter}");
            return None;
        };

        match protocol {
            "ws-room" => self.ws_room_events.send(StartWsRoom {
                address: address.to_owned(),
            }),
            "signed-login" => self.signed_login_events.send(StartSignedLogin {
                address: address.to_owned(),
            }),
            "livekit" => {
                let entity = self.commands.spawn_empty().id();
                self.livekit_events.send(StartLivekit {
                    entity,
                    address: address.to_owned(),
                });
                return Some(entity);
            }
            "offline" => {
                info!("comms offline");
            }
            "archipelago" => {
                let ws_url = format!(
                    "{}/archipelago/ws",
                    address
                        .strip_prefix("http")
                        .map(|stripped| format!("ws{stripped}"))
                        .unwrap_or(address.to_owned())
                );
                debug!("arch starting: {ws_url}");
                self.archipelago_events
                    .send(StartArchipelago { address: ws_url });
            }
            _ => {
                warn!("unrecognised adapter protocol: {protocol}");
            }
        }

        None
    }
}
