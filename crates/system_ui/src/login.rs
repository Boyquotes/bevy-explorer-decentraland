use bevy::{
    prelude::*,
    tasks::{IoTaskPool, Task},
};
use common::{
    profile::{LambdaProfiles, SerializedProfile},
    structs::{AppConfig, ChainLink, PreviousLogin},
    util::TaskExt,
};
use comms::profile::{CurrentUserProfile, UserProfile};
use ethers_core::types::Address;
use ethers_signers::LocalWallet;
use ipfs::IpfsAssetServer;
use isahc::ReadResponseExt;
use scene_runner::Toaster;
use ui_core::dialog::{ButtonDisabledText, ButtonText, IntoDialogBody, SpawnButton, SpawnDialog};
use wallet::{browser_auth::try_create_remote_ephemeral, Wallet};

pub struct LoginPlugin;

impl Plugin for LoginPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, connect_wallet);
    }
}

enum LoginType {
    ExistingRemote,
    NewRemote,
    Guest,
}

struct LoginDialog {
    sender: tokio::sync::mpsc::Sender<LoginType>,
    previous_login: Option<PreviousLogin>,
}

impl IntoDialogBody for LoginDialog {
    fn body(self, commands: &mut ChildBuilder) {
        let sender = self.sender.clone();
        if self.previous_login.is_some() {
            commands
                .spawn_empty()
                .spawn_button(ButtonText("Reuse Last Login"), move || {
                    let _ = sender.blocking_send(LoginType::ExistingRemote);
                });
        } else {
            commands
                .spawn_empty()
                .spawn_button(ButtonDisabledText("Reuse Last Login"), move || {});
        }
        let sender = self.sender.clone();
        commands
            .spawn_empty()
            .spawn_button(ButtonText("Connect External Wallet"), move || {
                let _ = sender.blocking_send(LoginType::NewRemote);
            });
        let sender = self.sender.clone();
        commands
            .spawn_empty()
            .spawn_button(ButtonText("Play as Guest"), move || {
                let _ = sender.blocking_send(LoginType::Guest);
            });
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn connect_wallet(
    mut commands: Commands,
    ipfas: IpfsAssetServer,
    mut wallet: ResMut<Wallet>,
    mut current_profile: ResMut<CurrentUserProfile>,
    mut task: Local<
        Option<
            Task<
                Result<(Address, LocalWallet, Vec<ChainLink>, Option<UserProfile>), anyhow::Error>,
            >,
        >,
    >,
    mut receiver: Local<Option<tokio::sync::mpsc::Receiver<LoginType>>>,
    mut dialog: Local<Option<Entity>>,
    mut toaster: Toaster,
) {
    // cleanup if we're done
    if wallet.address().is_some() {
        if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
            commands.despawn_recursive();
        }
        *dialog = None;
        *receiver = None;
        *task = None;
        return;
    }

    // create dialog
    if dialog.is_none() && task.is_none() {
        let (sx, rx) = tokio::sync::mpsc::channel(1);
        *receiver = Some(rx);

        let previous_login = std::fs::read("config.json")
            .ok()
            .and_then(|f| serde_json::from_slice::<AppConfig>(&f).ok())
            .unwrap_or_default()
            .previous_login;

        *dialog = Some(commands.spawn_dialog(
            "Login".to_string(),
            LoginDialog {
                sender: sx,
                previous_login,
            },
            "Quit",
            || {
                std::process::exit(0);
            },
        ));
        return;
    }

    // handle task results
    if let Some(mut t) = task.take() {
        match t.complete() {
            Some(Ok((root_address, local_wallet, auth, profile))) => {
                let ephemeral_key = local_wallet.signer().to_bytes().to_vec();

                // store to app config
                let mut config: AppConfig = std::fs::read("config.json")
                    .ok()
                    .and_then(|f| serde_json::from_slice(&f).ok())
                    .unwrap_or_default();
                config.previous_login = Some(PreviousLogin {
                    root_address,
                    ephemeral_key,
                    auth: auth.clone(),
                });
                if let Err(e) =
                    std::fs::write("config.json", serde_json::to_string(&config).unwrap())
                {
                    warn!("failed to write to config: {e}");
                }

                wallet.finalize(root_address, local_wallet, auth);
                if let Some(profile) = profile {
                    toaster.add_toast("login profile", "Profile loaded");
                    current_profile.0 = Some(profile);
                } else {
                    toaster.add_toast("login profile", "Failed to load profile, using default");
                    current_profile.0 = Some(UserProfile {
                        version: 0,
                        content: SerializedProfile::default(),
                        base_url: "https://peer.decentraland.org/content".to_owned(),
                    });
                }
            }
            Some(Err(e)) => {
                error!("{e}");
            }
            None => {
                *task = Some(t);
            }
        }
    }

    // handle click
    if let Ok(login) = receiver.as_mut().unwrap().try_recv() {
        if let Some(commands) = dialog.and_then(|d| commands.get_entity(d)) {
            commands.despawn_recursive();
        }

        match login {
            LoginType::ExistingRemote => {
                info!("existing remote");
                let ipfs = ipfas.ipfs().clone();
                let previous_login = std::fs::read("config.json")
                    .ok()
                    .and_then(|f| serde_json::from_slice::<AppConfig>(&f).ok())
                    .unwrap()
                    .previous_login
                    .unwrap();

                *task = Some(IoTaskPool::get().spawn(async move {
                    let PreviousLogin {
                        root_address,
                        ephemeral_key,
                        auth,
                    } = previous_login;

                    let profile = ipfs
                        .lambda_endpoint()
                        .and_then(|endpoint| {
                            isahc::get(format!("{endpoint}/profiles/{root_address:#x}")).ok()
                        })
                        .and_then(|mut response| response.json::<LambdaProfiles>().ok())
                        .and_then(|profiles| profiles.avatars.into_iter().next())
                        .map(|content| UserProfile {
                            version: content.version as u32,
                            content,
                            base_url: "https://peer.decentraland.org/content".to_owned(),
                        });

                    let local_wallet = LocalWallet::from_bytes(&ephemeral_key).unwrap();

                    Ok((previous_login.root_address, local_wallet, auth, profile))
                }));
            }
            LoginType::NewRemote => {
                info!("new remote");
                let ipfs = ipfas.ipfs().clone();
                *task = Some(IoTaskPool::get().spawn(async move {
                    let (root_address, local_wallet, auth, _) =
                        try_create_remote_ephemeral().await?;

                    let profile = ipfs
                        .lambda_endpoint()
                        .and_then(|endpoint| {
                            isahc::get(format!("{endpoint}/profiles/{root_address:#x}")).ok()
                        })
                        .and_then(|mut response| response.json::<LambdaProfiles>().ok())
                        .and_then(|profiles| profiles.avatars.into_iter().next())
                        .map(|content| UserProfile {
                            version: content.version as u32,
                            content,
                            base_url: "https://peer.decentraland.org/content".to_owned(),
                        });

                    Ok((root_address, local_wallet, auth, profile))
                }));
            }
            LoginType::Guest => {
                info!("guest");
                toaster.add_toast(
                    "login profile",
                    "Warning: Guest profile will not persist beyond the current session",
                );
                wallet.finalize_as_guest();
                current_profile.0 = Some(UserProfile {
                    version: 0,
                    content: SerializedProfile::default(),
                    base_url: "https://peer.decentraland.org/content".to_owned(),
                })
            }
        }
    }
}
