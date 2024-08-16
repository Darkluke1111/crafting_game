use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    time::SystemTime,
};

use bevy::{
    log::LogPlugin, prelude::*, window::PresentMode,
    winit::WinitSettings,
};
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use bevy_rand::{plugin::EntropyPlugin, prelude::{EntropyComponent, WyRand}};
use bevy_replicon::prelude::*;
use bevy_replicon_renet::RenetChannelsExt;
use bevy_replicon_renet::{
    renet::{
        transport::{
            ClientAuthentication, NetcodeClientTransport, NetcodeServerTransport,
            ServerAuthentication, ServerConfig,
        },
        ConnectionConfig, RenetClient, RenetServer,
    },
    RepliconRenetPlugins,
};
use bevy_replicon_snap::{
    interpolation::AppInterpolationExt,
    NetworkOwner, SnapshotInterpolationPlugin,
};
use clap::Parser;
use inventory_ui::InventoryUIPlugin;
use item::ItemPlugin;
use item_container::ItemContainerPlugin;
use player::{PlayerBundle, PlayerPlugin};
use serde::{Deserialize, Serialize};
use world::WorldPlugin;

mod player;
mod world;
mod item;
mod inventory_ui;
mod item_container;

const PROTOCOL_ID: u64 = 0x1122334455667788;
const MAX_TICK_RATE: u16 = 20;

fn main() {
    App::new()
        .init_resource::<Cli>()
        .insert_resource(WinitSettings {
            focused_mode: bevy::winit::UpdateMode::Continuous,
            unfocused_mode: bevy::winit::UpdateMode::Continuous,
        })
        .add_plugins((
            DefaultPlugins
                .set(LogPlugin {
                    level: bevy::log::Level::DEBUG,
                    filter: "info,wgpu_core=warn,wgpu_hal=warn,replicon_test=debug".into(),
                    ..Default::default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        present_mode: PresentMode::Immediate,
                        ..Default::default()
                    }),
                    ..Default::default()
                })
                .set(ImagePlugin::default_nearest()),
            RepliconPlugins.set(ServerPlugin {
                tick_policy: TickPolicy::MaxTickRate(MAX_TICK_RATE),
                ..Default::default()
            }),
            RepliconRenetPlugins,
            WorldInspectorPlugin::new(),
            SnapshotInterpolationPlugin {
                 max_tick_rate: MAX_TICK_RATE,
            },
            EntropyPlugin::<WyRand>::default(),
            PlayerPlugin,
            WorldPlugin,
            ItemPlugin,
            InventoryUIPlugin,
            ItemContainerPlugin,
        ))
        .add_client_event::<MoveEvent>(ChannelKind::Ordered)
        .add_client_event::<ActionEvent>(ChannelKind::Ordered)
        .add_systems(Startup, (read_cli.map(Result::unwrap), setup_camera))
        .add_systems(
            Update,
            (read_input, handle_connections.run_if(has_authority)),
        )
        .replicate_interpolated::<Transform>()
        .replicate::<Name>()
        .replicate::<EntropyComponent<WyRand>>()
        .run();
}

fn read_cli(
    mut commands: Commands,
    cli: Res<Cli>,
    channels: Res<RepliconChannels>,
) -> Result<(), Box<dyn Error>> {
    match *cli {
        Cli::Server { port } => {
            let server_channels_config = channels.get_server_configs();
            let client_channels_config = channels.get_client_configs();

            let server = RenetServer::new(ConnectionConfig {
                server_channels_config,
                client_channels_config,
                ..Default::default()
            });

            let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
            let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
            let server_config = ServerConfig {
                current_time,
                max_clients: 10,
                protocol_id: PROTOCOL_ID,
                authentication: ServerAuthentication::Unsecure,
                public_addresses: Default::default(),
            };
            let transport = NetcodeServerTransport::new(server_config, socket)?;

            commands.insert_resource(server);
            commands.insert_resource(transport);
        }
        Cli::Client { port, ip } => {
            let server_channels_config = channels.get_server_configs();
            let client_channels_config = channels.get_client_configs();

            let client = RenetClient::new(ConnectionConfig {
                server_channels_config,
                client_channels_config,
                ..Default::default()
            });

            let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?;
            let client_id = current_time.as_millis() as u64;
            let server_addr = SocketAddr::new(ip, port);
            let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
            let authentication = ClientAuthentication::Unsecure {
                client_id,
                protocol_id: PROTOCOL_ID,
                server_addr,
                user_data: None,
            };
            let transport = NetcodeClientTransport::new(current_time, authentication, socket)?;

            commands.insert_resource(client);
            commands.insert_resource(transport);
        }
    }

    Ok(())
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2dBundle {
        transform: Transform::from_translation(Vec3::new(0.0, 0.0, 1.0)),
        ..Default::default()
    });
}

fn handle_connections(
    mut commands: Commands,
    mut server_events: EventReader<ServerEvent>,
    player_query: Query<(Entity, &NetworkOwner)>,
) {
    for event in server_events.read() {
        match event {
            ServerEvent::ClientConnected { client_id } => {
                debug!("Client connected: {:?}", client_id);
                commands.spawn(PlayerBundle::new(*client_id));
            }
            ServerEvent::ClientDisconnected { client_id, reason } => {
                debug!("Client disconnected: {:?} Reason: {:?}", client_id, reason);
                for (entity, owner) in player_query.iter() {
                    if owner.0 == client_id.get() {
                        commands.entity(entity).despawn_recursive();
                    }
                }
            }
        }
    }
}

fn read_input(
    input: Res<ButtonInput<KeyCode>>,
    mut move_ev: EventWriter<MoveEvent>,
    mut action_ev: EventWriter<ActionEvent>,
) {
    let mut direction = Vec2::ZERO;

    if input.pressed(KeyCode::KeyW) {
        direction.y += 1.0;
    }
    if input.pressed(KeyCode::KeyS) {
        direction.y -= 1.0;
    }
    if input.pressed(KeyCode::KeyA) {
        direction.x -= 1.0;
    }
    if input.pressed(KeyCode::KeyD) {
        direction.x += 1.0;
    }
    if direction != Vec2::ZERO {
        move_ev.send(MoveEvent { input: direction });
    }
    for key in input.get_just_pressed() {
        action_ev.send(ActionEvent {
            action: *key,
        });
    }
}

#[derive(Event, Serialize, Deserialize, Debug, Clone)]
struct MoveEvent {
    pub input: Vec2,
}

#[derive(Event, Serialize, Deserialize, Debug, Clone)]
struct ActionEvent {
    pub action: KeyCode,
}

const PORT: u16 = 5000;

#[derive(Parser, Debug, Resource, PartialEq)]
enum Cli {
    Server {
        #[arg(short, long, default_value_t = PORT)]
        port: u16,
    },
    Client {
        #[arg(short, long, default_value_t = Ipv4Addr::LOCALHOST.into())]
        ip: IpAddr,

        #[arg(short, long, default_value_t = PORT)]
        port: u16,
    },
}

impl Default for Cli {
    fn default() -> Self {
        Self::parse()
    }
}
