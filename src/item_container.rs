use bevy::prelude::*;
use bevy_replicon::{core::Replicated, prelude::AppRuleExt};
use serde::{Deserialize, Serialize};

use crate::{item::Item, read_cli, Cli};


pub struct ItemContainerPlugin;


impl Plugin for ItemContainerPlugin {
    fn build(&self, _app: &mut App) {
        _app
            .register_type::<ItemContainer>()
            .replicate::<ItemContainer>()
            .add_systems(Startup, insert_dummy_container.after(read_cli));
    }
}


#[derive(Debug, Component, Serialize, Deserialize, Reflect)]
pub struct ItemContainer {pub items: Vec<Item>,}

fn insert_dummy_container(
    mut commands: Commands,
    cli: Res<Cli>,
) {
    if let Cli::Server {.. } = *cli {
        let items = vec![Item::new("Bread", "bread", 1)];
        commands.spawn((Name::new("item container"), ItemContainer {
            items
        }, Replicated));
    }
}