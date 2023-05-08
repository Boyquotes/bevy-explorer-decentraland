pub mod broadcast_position;
pub mod global_crdt;
pub mod profile;
pub mod wallet;
pub mod websocket_room;

use bevy::prelude::*;
use bimap::BiMap;
use ethers::types::{Address, H160};
use tokio::sync::mpsc::Sender;

use crate::{
    dcl_component::{DclWriter, ToDclWriter},
    ipfs::CurrentRealm,
};

use self::{
    broadcast_position::BroadcastPositionPlugin,
    global_crdt::GlobalCrdtPlugin,
    profile::UserProfilePlugin,
    websocket_room::{WebsocketRoomPlugin, WebsocketRoomTransport},
};

pub struct CommsPlugin;

impl Plugin for CommsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(WebsocketRoomPlugin);
        app.add_plugin(BroadcastPositionPlugin);
        app.add_plugin(GlobalCrdtPlugin);
        app.add_plugin(UserProfilePlugin);
        app.add_system(process_realm_change);
    }
}

pub struct TransportAlias {
    pub adapter: Entity,
    pub alias: u32,
}

pub enum TransportType {
    WebsocketRoom,
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
    pub user_alias: Option<u32>,
    pub foreign_aliases: BiMap<u32, Address>,
}

fn process_realm_change(
    mut commands: Commands,
    realm: Res<CurrentRealm>,
    adapters: Query<Entity, With<Transport>>,
) {
    if realm.is_changed() {
        for adapter in adapters.iter() {
            commands.entity(adapter).despawn_recursive();
        }

        if let Some(comms) = realm.comms.as_ref() {
            if let Some(fixed_adapter) = comms.fixed_adapter.as_ref() {
                // fixedAdapter	"ws-room:wss://sdk-test-scenes.decentraland.zone/mini-comms/room-1"
                let Some((protocol, address)) = fixed_adapter.split_once(':') else {
                    warn!("unrecognised fixed adapter string: {fixed_adapter}");
                    return;
                };

                match protocol {
                    "ws-room" => {
                        info!("starting ws-room adapter");
                        let (sender, receiver) = tokio::sync::mpsc::channel(1000);

                        commands.spawn((
                            Transport {
                                transport_type: TransportType::WebsocketRoom,
                                sender,
                                user_alias: None,
                                foreign_aliases: Default::default(),
                            },
                            WebsocketRoomTransport {
                                address: address.to_owned(),
                                receiver: Some(receiver),
                                retries: 0,
                            },
                        ));
                    }
                    "offline" => {
                        info!("comms offline");
                    }
                    _ => {
                        warn!("unrecognised fixed adapter protocol: {protocol}");
                    }
                }
            } else {
                warn!("no fixed adapter, i don't understand anything else");
            }
        } else {
            warn!("missing comms!");
        }
    }
}

trait AsH160 {
    fn as_h160(&self) -> Option<H160>;
}

impl AsH160 for &str {
    fn as_h160(&self) -> Option<H160> {
        if self.starts_with("0x") {
            return (&self[2..]).as_h160();
        }

        let Ok(hex_bytes) = hex::decode(self.as_bytes()) else { return None };
        if hex_bytes.len() != H160::len_bytes() {
            return None;
        }

        Some(H160::from_slice(hex_bytes.as_slice()))
    }
}

impl AsH160 for String {
    fn as_h160(&self) -> Option<H160> {
        self.as_str().as_h160()
    }
}
