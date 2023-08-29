use crate::{
    bundle::{Bundle, BundleSpawner, DynamicBundle},
    component::ComponentId,
    entity::Entity,
    world::World,
};
use std::iter::FusedIterator;

/// An iterator that spawns a series of entities and returns the [ID](Entity) of
/// each spawned entity.
///
/// If this iterator is not fully exhausted, any remaining entities will be spawned when this type is dropped.
pub struct SpawnBatchIter<'w, I>
where
    I: Iterator,
    I::Item: DynamicBundle,
{
    inner: I,
    spawner: BundleSpawner<'w, 'w>,
}

impl<'w, I> SpawnBatchIter<'w, I>
where
    I: Iterator,
    I::Item: DynamicBundle,
{
    pub(crate) fn new_dynamic(world: &'w mut World, component_ids: &[ComponentId], iter: I) -> Self
    where
        I::Item: DynamicBundle,
    {
        // Ensure all entity allocations are accounted for so `self.entities` can realloc if
        // necessary
        world.flush();

        let change_tick = world.change_tick();

        let (lower, upper) = iter.size_hint();
        let length = upper.unwrap_or(lower);

        let (bundle_info, storages) = world
            .bundles
            .init_dynamic_info(&mut world.components, component_ids);
        world.entities.reserve(length as u32);
        let mut spawner = bundle_info.get_bundle_spawner(
            &mut world.entities,
            &mut world.archetypes,
            &mut world.components,
            &mut world.storages,
            change_tick,
        );
        spawner.reserve_storage(length);

        Self {
            inner: iter,
            spawner,
        }
    }
    #[inline]
    pub(crate) fn new(world: &'w mut World, iter: I) -> Self
    where
        I::Item: Bundle,
    {
        // Ensure all entity allocations are accounted for so `self.entities` can realloc if
        // necessary
        world.flush();

        let change_tick = world.change_tick();

        let (lower, upper) = iter.size_hint();
        let length = upper.unwrap_or(lower);

        let bundle_info = world
            .bundles
            .init_info::<I::Item>(&mut world.components, &mut world.storages);
        world.entities.reserve(length as u32);
        let mut spawner = bundle_info.get_bundle_spawner(
            &mut world.entities,
            &mut world.archetypes,
            &mut world.components,
            &mut world.storages,
            change_tick,
        );
        spawner.reserve_storage(length);

        Self {
            inner: iter,
            spawner,
        }
    }
}

impl<I> Drop for SpawnBatchIter<'_, I>
where
    I: Iterator,
    I::Item: DynamicBundle,
{
    fn drop(&mut self) {
        for _ in self {}
    }
}

impl<I> Iterator for SpawnBatchIter<'_, I>
where
    I: Iterator,
    I::Item: DynamicBundle,
{
    type Item = Entity;

    fn next(&mut self) -> Option<Entity> {
        let bundle = self.inner.next()?;
        let entity = self.spawner.entities.alloc();
        // SAFETY: entity is allocated (but non-existent), `T` matches this BundleInfo's type
        unsafe {
            self.spawner.spawn_non_existent(entity, bundle);
        };
        Some(entity)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<I, T> ExactSizeIterator for SpawnBatchIter<'_, I>
where
    I: ExactSizeIterator<Item = T>,
    I::Item: DynamicBundle,
{
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<I, T> FusedIterator for SpawnBatchIter<'_, I>
where
    I: FusedIterator<Item = T>,
    I::Item: DynamicBundle,
{
}
