use std::{fs::File, io::{Read, Write}, path::Path};

use bevy::{
    asset::AssetPath, color::palettes::css::{RED, YELLOW}, ecs::{reflect, world::CommandQueue}, prelude::*, scene::{ron, serde::SceneDeserializer}, tasks::{block_on, futures_lite::future, IoTaskPool, Task}, utils::dbg
};
use bevy_ecs_tilemap::{
    map::{TilemapId, TilemapSize, TilemapTexture, TilemapTileSize, TilemapType},
    prelude::*,
    tiles::{TileBundle, TilePos, TileStorage},
    FrustumCulling,
};
use bevy_inspector_egui::egui::load;
use bevy_mod_picking::{
    events::{Click, Pointer},
    prelude::On,
};
use bevy_rand::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_snap::NetworkOwner;
use rand_core::RngCore;
use serde::{de::DeserializeSeed, Deserialize, Deserializer, Serialize};

use crate::{chunk::{Chunk, ComputeTask, LoadChunk, SaveChunk, GRID_SIZE, MAP_SIZE, TILE_LENGTH, TILE_SIZE}, player::Player, ActionEvent, ClickTileEvent};







fn update_ground_texture(
    mut tile_query: Query<
        (
            &mut TileTextureIndex,
            &Ground,
            &mut EntropyComponent<WyRand>,
        ),
        Changed<Ground>,
    >,
) {
    for (mut texture_index, ground, mut rnd) in tile_query.iter_mut() {
        match ground {
            Ground::Grass => {
                texture_index.0 = rnd.next_u32() % 32;
            }
            Ground::Dirt => {
                texture_index.0 = 32;
            }
            _ => {
                texture_index.0 = 4;
            }
        }
    }
}



fn apply_action(
    mut tile_query: Query<(&TilePos, &mut Ground)>,
    player_query: Query<(&NetworkOwner, &Transform)>,
    mut events: EventReader<FromClient<ActionEvent>>,
) -> Option<()> {
    for FromClient { client_id, event } in events.read() {
        if event.action != KeyCode::Space {
            continue;
        }
        if let Some((_, t)) = player_query.iter().find(|p| p.0 .0 == client_id.get()) {
            let tile_pos = TilePos::from_world_pos(
                &t.translation.xy(),
                &MAP_SIZE,
                &GRID_SIZE,
                &TilemapType::Square,
            )?;
            let (_pos, mut ground) = tile_query.iter_mut().find(|(pos, _)| pos == &&tile_pos)?;
            *ground = Ground::Dirt;
        }
    }
    Some(())
}

fn detect_tile_click(
    mut click_events: EventReader<Pointer<Click>>,
    tiles: Query<&TilePos>,
    mut writer: EventWriter<ClickTileEvent>,
) {
    for click in click_events.read() {
        let Some(tile_pos) = tiles.get(click.target).ok() else {
            continue;
        };
        writer.send(ClickTileEvent { tile: click.target });
    }
}

fn handle_tile_click(
    mut reader: EventReader<FromClient<ClickTileEvent>>,
    mut tiles: Query<(&mut Ground), With<TilePos>>,
) {
    for FromClient {
        client_id,
        event: ClickTileEvent { tile },
    } in reader.read()
    {
        match tiles.get_mut(*tile) {
            Ok(mut ground) => *ground = Ground::Dirt,
            Err(_) => {}
        }
    }
}

fn debug_draw_chunk_borders(chunk_query: Query<&Chunk>, mut gizmos: Gizmos) {
    for chunk in chunk_query.iter() {
        let pos = chunk.get_world_coords();
        gizmos.circle_2d(pos, 1.0, RED);
        gizmos.rect_2d(
            chunk.get_world_coords() + chunk.get_size() * 0.5,
            0.0,
            chunk.get_size(),
            RED,
        );
    }
}

fn debug_draw_tile_borders(
    chunk_query: Query<&Chunk>,
    tile_query: Query<(&TilePos, &Parent)>,
    mut gizmos: Gizmos,
) {
    for (tile, parent) in tile_query.iter() {
        let chunk = chunk_query.get(**parent).unwrap();
        let tile_pos = Vec2::new(tile.x as f32 * TILE_LENGTH, tile.y as f32 * TILE_LENGTH);
        let pos = chunk.get_world_coords() + tile_pos;
        let tile_size: Vec2 = GRID_SIZE.into();
        gizmos.rect_2d(pos + tile_size * 0.5, 0.0, tile_size, YELLOW);
    }
}


#[derive(Component, Debug, Reflect, Serialize, Deserialize, Clone)]
#[reflect(Component)]
pub enum Ground {
    Dirt,
    Grass,
    Stone,
    Water,
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(TilemapPlugin)
            
            .replicate_mapped::<TilemapId>()
            .replicate_mapped::<TileStorage>()
            .replicate::<TilePos>()
            .replicate::<Ground>()
            .replicate::<Chunk>()
            .replicate::<TileVisible>()
            .replicate::<TileFlip>()
            .replicate::<TileTextureIndex>()
            .replicate::<TileColor>()
            .replicate::<TilePosOld>()
            .register_type::<Chunk>()
            .register_type::<Ground>()


            .add_systems(
                Update,
                (
                    debug_draw_chunk_borders,
                    debug_draw_tile_borders,
                    detect_tile_click.run_if(client_connected),
                    handle_tile_click.run_if(has_authority),
                ),
            )
            .add_systems(
                Update,
                (
                    apply_action.map(Option::unwrap).run_if(has_authority),
                    update_ground_texture,
                ),
            );
    }
}

pub trait ChunkPosExt {
    fn from_in_chunk_pos(pos: Vec2) -> Option<Self>
    where
        Self: Sized;
    fn get_in_chunk_pos(&self) -> Vec2;
}

impl ChunkPosExt for TilePos {
    fn from_in_chunk_pos(pos: Vec2) -> Option<Self> {
        let tile_pos = (pos / TILE_LENGTH).trunc();
        let tile_pos = TilePos::from_i32_pair(tile_pos.x as i32, tile_pos.y as i32, &MAP_SIZE);
        //dbg!(tile_pos);
        return tile_pos;
    }

    fn get_in_chunk_pos(&self) -> Vec2 {
        todo!()
    }
}