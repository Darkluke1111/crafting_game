use bevy::{prelude::*};
use bevy_replicon::{client::ClientSet, core::{ClientId, Replicated}, prelude::{has_authority, AppRuleExt, FromClient, RepliconClient}};
use bevy_replicon_renet::renet::RenetClient;
use serde::{Deserialize, Serialize};

use crate::MoveEvent;


pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {

    fn build(&self, app: &mut App) {        
        app
            .replicate::<Player>()
            .add_systems(PreUpdate, init_player.after(ClientSet::Receive))
            .add_systems(Update, apply_movement.run_if(has_authority));
    }
}


#[derive(Bundle)]
pub struct PlayerBundle {
    player: Player,
    transform: Transform,
    replicated: Replicated,
}

impl PlayerBundle {
    pub fn new(client_id: ClientId) -> Self {
        Self {
            player: Player { client_id, speed: 100.0},
            transform: Transform::default(),
            replicated: Replicated::default(),
        }
    }
}


#[derive(Component, Deserialize, Serialize)]
struct Player {
    client_id: ClientId,
    speed: f32,
}

fn init_player(
    mut commands: Commands,
    mut query: Query<Entity, (With<Player>, Without<GlobalTransform>)>,
    mut asset_server: ResMut<AssetServer>,
) {
    for entity in query.iter_mut() {
        commands.entity(entity).insert((
            GlobalTransform::default(),
            VisibilityBundle::default(),
            Sprite {
                custom_size: Some(Vec2::new(32.0, 32.0)),
                ..Default::default()
            },
            asset_server.load::<Image>("player.png"),
        ));
    }
}

fn apply_movement(
    time: Res<Time>,
    mut query: Query<(&mut Transform, &Player)>,
    mut move_event: EventReader<FromClient<MoveEvent>>,
) {
    for FromClient { client_id, event } in move_event.read() {
        for (mut transform, player) in query.iter_mut().filter(|(_, p)| p.client_id == *client_id) {
            transform.translation += event.input.extend(0.0) * time.delta_seconds() * player.speed;
        }
    }
}