
use std::{fs::File, io::{Read, Write}, path::Path};

use bevy::{ecs::world::CommandQueue, prelude::*, tasks::{block_on, futures_lite::{future, FutureExt}, ComputeTaskPool, IoTaskPool, Task}};
use bevy_ecs_tilemap::{map::TilemapSize, tiles::{TilePos, TileStorage}};
use bevy_replicon::{core::Replicated, prelude::{server_running, ParentSync}};
use serde::{Deserialize, Serialize};

use crate::world::{Chunk, Ground, MAP_SIZE, TILES_PER_CHUNK};


pub struct ChunkPlugin;

impl Plugin for ChunkPlugin {
    fn build(&self, app: &mut App) {
        app
            .observe(load_chunk)
            .observe(save_chunk)
            .add_systems(Update, task_poll.run_if(server_running))
            .add_event::<LoadChunk>()
            .add_event::<SaveChunk>();
    }
}


#[derive(Debug,Serialize, Deserialize)]
struct ChunkData {
    chunk_index: IVec2,
    tiles: Vec<TileData>,

}

#[derive(Debug,Serialize, Deserialize)]
struct TileData {
    tile_index: UVec2,
    ground: Ground,
}


fn load_chunk(
    trigger: Trigger<LoadChunk>,
    mut commands: Commands,
) {
    let IVec2 { x, y } = trigger.event().index;
    let mut entity = commands.spawn_empty();
    let entity_id = entity.id();
    let task = IoTaskPool::get().spawn(async move {
        let mut path = File::open(&format!("world/{x}_{y}.ron")).expect("Error while opening file");
        let mut bytes =  Vec::<u8>::new();
        path.read_to_end(&mut bytes);
        let chunk_data = ron::de::from_bytes::<ChunkData>(&bytes).ok().expect("Error while deserializing");

        let mut command_queue = CommandQueue::default();

        command_queue.push(move |world: &mut World| {
            let mut tile_storage = TileStorage::empty(MAP_SIZE);
            let tilemap_entity = world.entity_mut(entity_id).insert((Chunk {chunk_index: chunk_data.chunk_index}, Replicated)).with_children(|parent| {
                for tile_data in chunk_data.tiles {
                    let tile_pos: TilePos = tile_data.tile_index.into();
                    let ground = tile_data.ground;
                    let tile_entity = parent.spawn((tile_pos, ground, Replicated, ParentSync::default())).id();
                    tile_storage.set(&tile_pos, tile_entity);
                }
            }).remove::<ComputeTask>().id();
            world.entity_mut(tilemap_entity).insert(tile_storage);
        });
        return command_queue;
    });
    entity.insert(ComputeTask(trigger.event().index,task));
}

fn save_chunk(
    trigger: Trigger<SaveChunk>,
    mut commands: Commands,
    chunks_q: Query<(&Chunk, &Children)>,
    tiles_q: Query<(&TilePos, &Ground)>,
) {
    let index = trigger.event().index;
    let (chunk, children) =  chunks_q.iter().find(|(x,_)| {x.chunk_index == index}).expect("Chunk to save does not exist!");

    let entities = children.into_iter();
   
    let tile_data: Vec<TileData> = children.iter()
        .map(|&e| {tiles_q.get(e)})
        .filter_map(|x| {x.ok()})
        .map(|x| {TileData{tile_index: UVec2::new(x.0.x, x.0.y), ground: x.1.clone()}})
        .collect();

    let chunk_data =  ChunkData {chunk_index: index, tiles: tile_data};
    
    let IVec2 { x, y } = trigger.event().index;
    IoTaskPool::get().spawn(async move {
        let mut path = File::create(&format!("world/{x}_{y}.ron")).expect("Error at file creation!");
        let chunk_data = ron::to_string(&chunk_data).expect("Error at serialisation");
        path.write(chunk_data.as_bytes()).expect("Error while writing chunk data to file");
    }).detach();
}

fn task_poll(
    mut commands: Commands,
    mut tasks_q: Query<&mut ComputeTask>
) {
    for mut task in &mut tasks_q {
        if let Some(mut commands_queue) = block_on(future::poll_once(&mut task.1)) {
            // append the returned command queue to have it execute later
            commands.append(&mut commands_queue);
        }
    }
}

#[derive(Event)]
pub struct LoadChunk {
    pub index: IVec2,
}

#[derive(Event)]
pub struct SaveChunk {
    pub index: IVec2,
}

#[derive(Component)]
pub struct ComputeTask(pub IVec2,pub Task<CommandQueue>);