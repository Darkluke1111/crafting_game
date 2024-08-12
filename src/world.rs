
use bevy::{ecs::entity::MapEntities, prelude::*};
use bevy_ecs_tilemap::{map::{TilemapId, TilemapSize, TilemapTexture, TilemapTileSize, TilemapType}, prelude::*, tiles::{TileBundle, TilePos, TileStorage}, TilemapBundle};
use bevy_replicon::{client::ClientSet, core::Replicated, prelude::{has_authority, AppRuleExt, FromClient, ParentSync}};
use bevy_replicon_snap::NetworkOwner;
use serde::{Deserialize, Serialize};

use crate::{ActionEvent, Cli};


const MAP_SIZE: TilemapSize = TilemapSize { x: 32, y: 32 };
const TILE_SIZE: TilemapTileSize = TilemapTileSize { x: 16.0, y: 16.0 };
const GRID_SIZE: TilemapGridSize = TilemapGridSize { x: 16.0, y: 16.0 };

fn setup_world(
    mut commands: Commands,
    cli: Res<Cli>,
) {
    if let Cli::Client { ip: _, port: _ }  = *cli {return;}
    let map_size = TilemapSize { x: 32, y: 32 };
    let tilemap_entity = commands.spawn_empty().id();
    let mut tile_storage = TileStorage::empty(map_size);
    warn!("Doing startup!");
    commands.entity(tilemap_entity).insert((Replicated, Chunk, Name::new("Chunk")));
    for x in 0..map_size.x {
        for y in 0..map_size.y {
            let tile_pos = TilePos { x, y };
            let tile_entity = commands
                .spawn((TileBundle {
                    position: tile_pos,
                    tilemap_id: TilemapId(tilemap_entity),
                    ..Default::default()
                },
                Replicated,
                Ground::Grass,
                Name::new("Tile"),
                )
            )
                .id();
            commands.entity(tilemap_entity).add_child(tile_entity);
            tile_storage.set(&tile_pos, tile_entity);
        }
    }

    
}

fn init_chunk(
    mut commands: Commands,
    query: Query<Entity,(With<Chunk>, Without<TilemapGridSize>)>,
    asset_server: Res<AssetServer>,
) {
    let texture_handle: Handle<Image> = asset_server.load("tiles.png");
    let tile_storage = TileStorage::empty(MAP_SIZE);
    let map_type = TilemapType::default();
    for entity in query.iter() {
        commands.entity(entity).insert(TilemapBundle {
            grid_size: GRID_SIZE,
            map_type,
            size: MAP_SIZE,
            storage: tile_storage.clone(),
            texture: TilemapTexture::Single(texture_handle.clone()),
            tile_size: TILE_SIZE,
            ..Default::default()
        });
    }
}

fn apply_action(
    mut tile_query: Query<(& TilePos, &mut TileTextureIndex, &mut Ground)>,
    player_query: Query<(&NetworkOwner, &Transform)>,
    mut events: EventReader<FromClient<ActionEvent>>,
) -> Option<()>{
    for FromClient { client_id, event } in events.read() {
        if let Some((_,t)) = player_query.iter().find(|p| p.0.0 == client_id.get()) {
            let tilePos = TilePos::from_world_pos(&t.translation.xy(), &MAP_SIZE, &GRID_SIZE, &TilemapType::Square)?;
            let (pos, mut tex_index, mut ground ) = tile_query.iter_mut().find(|(pos,_,_)| pos == &&tilePos)?;
            *ground = Ground::Dirt;
            tex_index.0 = 2;
        }
    }
    Some(())
}

#[derive(Component, Serialize, Deserialize)]
pub struct Chunk;


#[derive(Component, Serialize, Deserialize)]
pub enum Ground {
    Dirt,
    Grass,
    Stone,
    Water,
}


pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app
            .add_plugins(TilemapPlugin)
            .replicate_mapped::<TilemapId>()
            .replicate::<TilePos>()
            .replicate::<Ground>()
            .replicate::<Chunk>()
            .replicate::<TileVisible>()
            .replicate::<TileFlip>()
            .replicate::<ParentSync>()
            .replicate::<TileTextureIndex>()
            .replicate::<TileColor>()
            .replicate::<TilePosOld>()
            .add_systems(Startup, setup_world)
            .add_systems(PreUpdate, init_chunk.after(ClientSet::Receive))
            .add_systems(Update, apply_action.map(Option::unwrap));
    }
}