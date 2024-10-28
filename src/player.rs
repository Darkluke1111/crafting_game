use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_snap::{
    prediction::{AppPredictionExt, OwnerPredicted, Predict},
    NetworkOwner,
};
use serde::{Deserialize, Serialize};

use crate::MoveEvent;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.replicate::<Player>()
            .add_systems(PreUpdate, init_player.after(ClientSet::Receive))
            .add_systems(Update, animate_player.run_if(client_connected))
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
            player: Player { speed: 100.0 },
            transform: Transform::from_xyz(0.0, 0.0, 1.0),
            replicated: Replicated::default(),
            predicted: OwnerPredicted,
            name: Name::new("Player"),
        }
    }
}

#[derive(Component, Deserialize, Serialize)]
pub struct Player {
    pub speed: f32,
}

#[derive(Debug, Component)]
struct WalkAnimation {
    old_pos: Vec2,
    current_state: PlayerAnimationState,
}

#[derive(Debug)]
enum PlayerAnimationState {
    StandStill(usize),
    WalkRight(usize),
    WalkLeft(usize),
}

#[derive(Component, Deref, DerefMut)]
struct AnimationTimer(Timer);

fn init_player(
    mut commands: Commands,
    mut query: Query<(Entity, &Transform), (With<Player>, Without<GlobalTransform>)>,
    asset_server: Res<AssetServer>,
    mut texture_atlas_layouts: ResMut<Assets<TextureAtlasLayout>>,
) {
    for (entity, transform) in query.iter_mut() {
        let texture = asset_server.load::<Image>("player/global.png");
        let layout = TextureAtlasLayout::from_grid(UVec2::new(32, 32), 8, 10, None, None);
        let layout_handle = texture_atlas_layouts.add(layout);
        commands.entity(entity).insert((
            GlobalTransform::default(),
            VisibilityBundle::default(),
            Sprite {
                custom_size: Some(Vec2::new(32.0, 32.0)),
                ..Default::default()
            },
            TextureAtlas {
                layout: layout_handle,
                index: 0,
            },
            texture,
            WalkAnimation {
                old_pos: transform.translation.xy(),
                current_state: PlayerAnimationState::WalkRight(0),
            },
            AnimationTimer(Timer::from_seconds(0.1, TimerMode::Repeating)),
        ));
    }
}

fn animate_player(
    mut query: Query<
        (
            &mut TextureAtlas,
            &Transform,
            &mut WalkAnimation,
            &mut AnimationTimer,
        ),
        With<Player>,
    >,
    time: Res<Time>,
) {
    const ANIMATION_RIGHT_INDEX : usize = 4;
    const ANIMATION_LEFT_INDEX : usize = 8;
    for (mut atlas, transform, mut animation, mut timer) in query.iter_mut() {
        timer.tick(time.delta());
        if timer.just_finished() {
            let diff = transform.translation.x - animation.old_pos.x;
            let new_state = match animation.current_state {
                PlayerAnimationState::StandStill(_) if (diff < 0.0) => {
                    PlayerAnimationState::WalkLeft(0)
                }
                PlayerAnimationState::StandStill(_) if (diff > 0.0) => {
                    PlayerAnimationState::WalkRight(0)
                }
                PlayerAnimationState::StandStill(old) => {
                    PlayerAnimationState::StandStill(old)
                }
                PlayerAnimationState::WalkLeft(_) if (diff >= 0.0) => {
                    PlayerAnimationState::StandStill(ANIMATION_LEFT_INDEX)
                }
                PlayerAnimationState::WalkLeft(old) => {
                    PlayerAnimationState::WalkLeft((old + 1) % 4)
                }
                PlayerAnimationState::WalkRight(_) if (diff <= 0.0) => {
                    PlayerAnimationState::StandStill(ANIMATION_RIGHT_INDEX)
                }
                PlayerAnimationState::WalkRight(old) => {
                    PlayerAnimationState::WalkRight((old + 1) % 4)
                }
            };
            *animation = WalkAnimation {
                old_pos: transform.translation.xy(),
                current_state: new_state,
            };
            atlas.index = match animation.current_state {
                PlayerAnimationState::StandStill(x) => x,
                PlayerAnimationState::WalkLeft(x) => x + ANIMATION_LEFT_INDEX,
                PlayerAnimationState::WalkRight(x) => x + ANIMATION_RIGHT_INDEX,
            }
        }
    }
}

impl Predict<MoveEvent, Player> for Transform {
    fn apply_event(&mut self, event: &MoveEvent, _delta_time: f32, context: &Player) {
        self.translation += event.input.extend(0.0) * 0.005 * context.speed;
    }
}
