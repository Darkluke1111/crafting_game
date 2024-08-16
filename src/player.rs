use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_snap::{prediction::{AppPredictionExt, OwnerPredicted, Predict}, NetworkOwner};
use serde::{Deserialize, Serialize};

use crate::MoveEvent;


pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {

    fn build(&self, app: &mut App) {        
        app
            .replicate::<Player>()
            .add_systems(PreUpdate, init_player.after(ClientSet::Receive))
            .add_client_predicted_event::<MoveEvent>(ChannelKind::Ordered)
            .predict_event_for_component::<MoveEvent, Player, Transform>();
    }
}


#[derive(Bundle)]
pub struct PlayerBundle {
    owner: NetworkOwner,
    player: Player,
    transform: Transform,
    predicted: OwnerPredicted,
    replicated: Replicated,
    name: Name,
}

impl PlayerBundle {
    pub fn new(client_id: ClientId) -> Self {
        Self {
            owner: NetworkOwner(client_id.get()),
            player: Player {speed: 100.0},
            transform: Transform::from_xyz(0.0,0.0,1.0),
            replicated: Replicated::default(),
            predicted: OwnerPredicted,
            name: Name::new("Player"),
        }
    }
}


#[derive(Component, Deserialize, Serialize)]
struct Player {
    speed: f32,
}

fn init_player(
    mut commands: Commands,
    mut query: Query<Entity, (With<Player>, Without<GlobalTransform>)>,
    asset_server: Res<AssetServer>,
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

impl Predict<MoveEvent, Player> for Transform {
    fn apply_event(&mut self, event: &MoveEvent, delta_time: f32, context: &Player) {
        self.translation += event.input.extend(0.0) * delta_time * context.speed;
    }
}