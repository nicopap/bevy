use bevy_ecs::{
    archetype::Archetype,
    bundle::DynamicBundle,
    component::{ComponentId, StorageType},
    prelude::{Bundle, Entity},
    ptr::{OwningPtr, Ptr},
    reflect::{AppTypeRegistry, ReflectComponent, ReflectMapEntities, ReflectResource},
    storage::Table,
    world::World,
};
use bevy_reflect::{TypePath, TypeRegistry, TypeUuid};
use bevy_utils::{HashMap, HashSet};

use crate::{DynamicScene, InstanceInfo, SceneSpawnError};

/// To spawn a scene, you can use either:
/// * [`SceneSpawner::spawn`](crate::SceneSpawner::spawn)
/// * adding the [`SceneBundle`](crate::SceneBundle) to an entity
/// * adding the [`Handle<Scene>`](bevy_asset::Handle) to an entity (the scene will only be
/// visible if the entity already has [`Transform`](bevy_transform::components::Transform) and
/// [`GlobalTransform`](bevy_transform::components::GlobalTransform) components)
#[derive(Debug, TypeUuid, TypePath)]
#[uuid = "c156503c-edd9-4ec7-8d33-dab392df03cd"]
pub struct Scene {
    pub world: World,
}

impl Scene {
    pub fn new(world: World) -> Self {
        Self { world }
    }

    /// Create a new scene from a given dynamic scene.
    pub fn from_dynamic_scene(
        dynamic_scene: &DynamicScene,
        type_registry: &AppTypeRegistry,
    ) -> Result<Scene, SceneSpawnError> {
        let mut world = World::new();
        let mut entity_map = HashMap::default();
        dynamic_scene.write_to_world_with(&mut world, &mut entity_map, type_registry)?;

        Ok(Self { world })
    }

    /// Clone the scene.
    ///
    /// This method will return a [`SceneSpawnError`] if a type either is not registered in the
    /// provided [`AppTypeRegistry`] or doesn't reflect the [`Component`](bevy_ecs::component::Component) trait.
    pub fn clone_with(&self, type_registry: &AppTypeRegistry) -> Result<Scene, SceneSpawnError> {
        let mut new_world = World::new();
        self.write_to_world_with(&mut new_world, type_registry)?;
        Ok(Self { world: new_world })
    }

    pub fn very_fast_write_to_world_with(
        &self,
        world: &mut World,
        type_registry: &AppTypeRegistry,
    ) -> Result<InstanceInfo, SceneSpawnError> {
        let mut instance_info = InstanceInfo {
            entity_map: HashMap::default(),
        };

        let type_registry = type_registry.read();

        // Resources archetype
        for (component_id, _) in self.world.storages().resources.iter() {
            let component_info = self
                .world
                .components()
                .get_info(component_id)
                .expect("component_ids in archetypes should have ComponentInfo");

            let type_id = component_info
                .type_id()
                .expect("reflected resources must have a type_id");

            let registration =
                type_registry
                    .get(type_id)
                    .ok_or_else(|| SceneSpawnError::UnregisteredType {
                        type_name: component_info.name().to_string(),
                    })?;
            let reflect_resource = registration.data::<ReflectResource>().ok_or_else(|| {
                SceneSpawnError::UnregisteredResource {
                    type_name: component_info.name().to_string(),
                }
            })?;
            reflect_resource.copy(&self.world, world);
        }

        struct ErasedBundle<'bn>(Box<[Ptr<'bn>]>);
        impl<'bn> DynamicBundle for ErasedBundle<'bn> {
            fn get_components(
                self,
                func: &mut impl FnMut(StorageType, bevy_ecs::ptr::OwningPtr<'_>),
            ) {
                for ptr in self.0.iter() {
                    // SAFETY: This is unsafe, yet sound right now:
                    // this relies on `func` not calling any unsafe methods of OnwingPtr,
                    // which is currently the case in bevy. It only calls `as_ptr`
                    let owning = unsafe { ptr.assert_unique().promote() };
                    func(StorageType::Table, owning);
                }
            }
        }
        struct TableCursor<'a> {
            table: &'a Table,
            current: usize,
            columns: Box<[Ptr<'a>]>,
        }
        fn is_copy(_: ComponentId, _: &TypeRegistry) -> bool {
            todo!("Tell if is copy")
        }
        impl<'a> TableCursor<'a> {
            fn new(table: &'a Table, reg: &TypeRegistry) -> (Self, Vec<ComponentId>) {
                let (ids, columns): (Vec<_>, Vec<_>) = table
                    .iter_ids()
                    // TODO(perf): std::ptr::copy_nonoverlapping metions that memory
                    // safety is _only_ violated when reading from both data
                    // So technically we should be able to blindly copy all the data
                    // and overwrite it later. Need benchmarking.
                    .filter(|(&id, _)| is_copy(id, reg))
                    .map(|(&id, column)| (id, column.get_data_ptr()))
                    .unzip();
                let cursor = TableCursor {
                    table,
                    columns: columns.into(),
                    current: 0,
                };
                (cursor, ids)
            }
        }
        impl<'a> Iterator for TableCursor<'a> {
            type Item = ErasedBundle<'a>;
            fn next(&mut self) -> Option<Self::Item> {
                if self.current >= self.table.entity_count() {
                    return None;
                }
                let ret = self.columns.clone();
                self.current += 1;
                for (ptr, column) in self.columns.iter_mut().zip(self.table.iter()) {
                    let size = column.item_layout().size();
                    // SAFETY: we are using the very same column's layout size
                    // so it better be correct
                    unsafe {
                        *ptr = ptr.byte_add(size);
                    }
                }
                Some(ErasedBundle(ret))
            }
        }

        let mut entities: HashSet<Entity> = HashSet::default();
        // TODO(bug): Currently broken:
        // - spawn all tables separately
        // - it's not complete yet
        for table in self.world.storages().tables.iter() {
            let (table_cursor, ids) = TableCursor::new(table, &type_registry);
            // SAFETY: By construction, `ids` contains all ComponentIds in the spawned bundles
            entities.extend(unsafe { world.spawn_batch_dynamic(&ids, table_cursor) });
            for scene_entity in archetype.entities() {
                let entity = *instance_info
                    .entity_map
                    .entry(scene_entity.entity())
                    .or_insert_with(|| world.spawn_empty().id());
                for component_id in archetype.components() {
                    let component_info = self
                        .world
                        .components()
                        .get_info(component_id)
                        .expect("component_ids in archetypes should have ComponentInfo");

                    let reflect_component = type_registry
                        .get(component_info.type_id().unwrap())
                        .ok_or_else(|| SceneSpawnError::UnregisteredType {
                            type_name: component_info.name().to_string(),
                        })
                        .and_then(|registration| {
                            registration.data::<ReflectComponent>().ok_or_else(|| {
                                SceneSpawnError::UnregisteredComponent {
                                    type_name: component_info.name().to_string(),
                                }
                            })
                        })?;
                    reflect_component.copy(&self.world, world, scene_entity.entity(), entity);
                }
            }
        }

        for registration in type_registry.iter() {
            if let Some(map_entities_reflect) = registration.data::<ReflectMapEntities>() {
                map_entities_reflect.map_all_entities(world, &mut instance_info.entity_map);
            }
        }

        Ok(instance_info)
    }
    /// Write the entities and their corresponding components to the given world.
    ///
    /// This method will return a [`SceneSpawnError`] if a type either is not registered in the
    /// provided [`AppTypeRegistry`] or doesn't reflect the [`Component`](bevy_ecs::component::Component) trait.
    pub fn write_to_world_with(
        &self,
        world: &mut World,
        type_registry: &AppTypeRegistry,
    ) -> Result<InstanceInfo, SceneSpawnError> {
        let mut instance_info = InstanceInfo {
            entity_map: HashMap::default(),
        };

        let type_registry = type_registry.read();

        // Resources archetype
        for (component_id, _) in self.world.storages().resources.iter() {
            let component_info = self
                .world
                .components()
                .get_info(component_id)
                .expect("component_ids in archetypes should have ComponentInfo");

            let type_id = component_info
                .type_id()
                .expect("reflected resources must have a type_id");

            let registration =
                type_registry
                    .get(type_id)
                    .ok_or_else(|| SceneSpawnError::UnregisteredType {
                        type_name: component_info.name().to_string(),
                    })?;
            let reflect_resource = registration.data::<ReflectResource>().ok_or_else(|| {
                SceneSpawnError::UnregisteredResource {
                    type_name: component_info.name().to_string(),
                }
            })?;
            reflect_resource.copy(&self.world, world);
        }

        for archetype in self.world.archetypes().iter() {
            for scene_entity in archetype.entities() {
                let entity = *instance_info
                    .entity_map
                    .entry(scene_entity.entity())
                    .or_insert_with(|| world.spawn_empty().id());
                for component_id in archetype.components() {
                    let component_info = self
                        .world
                        .components()
                        .get_info(component_id)
                        .expect("component_ids in archetypes should have ComponentInfo");

                    let reflect_component = type_registry
                        .get(component_info.type_id().unwrap())
                        .ok_or_else(|| SceneSpawnError::UnregisteredType {
                            type_name: component_info.name().to_string(),
                        })
                        .and_then(|registration| {
                            registration.data::<ReflectComponent>().ok_or_else(|| {
                                SceneSpawnError::UnregisteredComponent {
                                    type_name: component_info.name().to_string(),
                                }
                            })
                        })?;
                    reflect_component.copy(&self.world, world, scene_entity.entity(), entity);
                }
            }
        }

        for registration in type_registry.iter() {
            if let Some(map_entities_reflect) = registration.data::<ReflectMapEntities>() {
                map_entities_reflect.map_all_entities(world, &mut instance_info.entity_map);
            }
        }

        Ok(instance_info)
    }
}
