use bevy::prelude::*;
use bevy_replicon::prelude::client_connected;
use sickle_ui::prelude::*;

use crate::{item_container::ItemContainer, ActionEvent};


pub struct InventoryUIPlugin;

impl Plugin for InventoryUIPlugin {
    fn build(&self, _app: &mut App) {
        _app
            .add_systems(Update, update_inventory_ui.run_if(client_connected))
            .add_systems(Update, handle_inventory.run_if(client_connected));
    }
}

#[derive(Component, Debug)]
pub struct InventoryUI {
    container: Entity,
}


#[derive(Component, Debug)]
pub struct ItemEntry;

impl InventoryUI {
    fn frame() -> impl Bundle {
        (Name::new("Inventory UI"), NodeBundle::default(),)
    }
}

pub trait UiInventoryUIExt {
    fn inventory(
        &mut self,
        spawn_children: impl FnOnce(&mut UiBuilder<Entity>),
        container: (Entity, &ItemContainer)
    ) -> UiBuilder<Entity>;
}

impl UiInventoryUIExt for UiBuilder<'_, Entity> {
    fn inventory(
        &mut self,
        spawn_children: impl FnOnce(&mut UiBuilder<Entity>),
        container: (Entity, &ItemContainer),
    ) -> UiBuilder<Entity> {
        self.container((InventoryUI::frame(), InventoryUI {container: container.0}), |parent| {
            parent.label(LabelConfig { label: "Inventory Stuff...".to_string(), ..Default::default() });
            for item in container.1.items.iter() {
                parent.label(LabelConfig { label: item.name.to_string(), ..Default::default() }).insert(ItemEntry);
            }
            spawn_children(parent)
        })
    }
}

fn handle_inventory(
    mut commands: Commands,
    mut event_reader: EventReader<ActionEvent>,
    query: Query<Entity, With<InventoryRoot>>,
    container_query: Query<(Entity, &ItemContainer)>,
) {
    for event in event_reader.read() {
        if event.action != KeyCode::KeyE {continue;}
        if query.is_empty() {
            if let Some(container) = container_query.iter().next() {
                commands.ui_builder(UiRoot).column(|column| {
                    column.inventory(|_| {}, container);
                }).insert(InventoryRoot);
            }

        } else {
            for inventories in query.iter() {
                commands.entity(inventories).despawn_recursive();
            }
        }
    }
}



fn update_inventory_ui(
    mut commands: Commands,
    query: Query<(Entity, &InventoryUI)>,
    container_query: Query<(Entity, &ItemContainer), Changed<ItemContainer>>,
    entry_query: Query<(Entity, &Parent), With<ItemEntry>>,
) {
    for (inv_entity, inv) in query.iter() {
        if let Some((_, item_container)) = container_query.get(inv.container).ok() {
            for (entry, parent) in entry_query.iter() {
                if parent.get() == inv_entity {
                    commands.entity(entry).despawn_recursive();
                }
            }
            for item in item_container.items.iter() {
                commands.ui_builder(inv_entity).label(LabelConfig { label: item.name.to_string(), ..Default::default() }).insert(ItemEntry);
            }
        }
    }
}


#[derive(Debug, Component)]
struct InventoryRoot;


