//! Definitions for [`Component`] reflection.
//!
//! This module exports two types: [`ReflectComponentFns`] and [`ReflectComponent`].
//!
//! # Architecture
//!
//! [`ReflectComponent`] wraps a [`ReflectComponentFns`]. In fact, each method on
//! [`ReflectComponent`] wraps a call to a function pointer field in `ReflectComponentFns`.
//!
//! ## Who creates `ReflectComponent`s?
//!
//! When a user adds the `#[reflect(Component)]` attribute to their `#[derive(Reflect)]`
//! type, it tells the derive macro for `Reflect` to add the following single line to its
//! [`get_type_registration`] method (see the relevant code[^1]).
//!
//! ```ignore
//! registration.insert::<ReflectComponent>(FromType::<Self>::from_type());
//! ```
//!
//! This line adds a `ReflectComponent` to the registration data for the type in question.
//! The user can access the `ReflectComponent` for type `T` through the type registry,
//! as per the `trait_reflection.rs` example.
//!
//! The `FromType::<Self>::from_type()` in the previous line calls the `FromType<C>`
//! implementation of `ReflectComponent`.
//!
//! The `FromType<C>` impl creates a function per field of [`ReflectComponentFns`].
//! In those functions, we call generic methods on [`World`] and [`EntityWorldMut`].
//!
//! The result is a `ReflectComponent` completely independent of `C`, yet capable
//! of using generic ECS methods such as `entity.get::<C>()` to get `&dyn Reflect`
//! with underlying type `C`, without the `C` appearing in the type signature.
//!
//! ## A note on code generation
//!
//! A downside of this approach is that monomorphized code (ie: concrete code
//! for generics) is generated **unconditionally**, regardless of whether it ends
//! up used or not.
//!
//! Adding `N` fields on `ReflectComponentFns` will generate `N × M` additional
//! functions, where `M` is how many types derive `#[reflect(Component)]`.
//!
//! Those functions will increase the size of the final app binary.
//!
//! [^1]: `crates/bevy_reflect/bevy_reflect_derive/src/registration.rs`
//!
//! [`get_type_registration`]: bevy_reflect::GetTypeRegistration::get_type_registration

use crate::{
    change_detection::Mut,
    component::{Component, ComponentId, Components},
    world::{unsafe_world_cell::UnsafeEntityCell, EntityRef, EntityWorldMut},
};
use bevy_ptr::{Ptr, PtrMut};
use bevy_reflect::{FromType, Reflect};

/// A struct used to operate on reflected [`Component`] of a type.
///
/// A [`ReflectComponent`] for type `T` can be obtained via
/// [`bevy_reflect::TypeRegistration::data`].
#[derive(Clone)]
pub struct ReflectComponent(ReflectComponentFns);

/// The raw function pointers needed to make up a [`ReflectComponent`].
///
/// This is used when creating custom implementations of [`ReflectComponent`] with
/// [`ReflectComponent::new()`].
///
/// > **Note:**
/// > Creating custom implementations of [`ReflectComponent`] is an advanced feature that most users
/// > will not need.
/// > Usually a [`ReflectComponent`] is created for a type by deriving [`Reflect`]
/// > and adding the `#[reflect(Component)]` attribute.
/// > After adding the component to the [`TypeRegistry`][bevy_reflect::TypeRegistry],
/// > its [`ReflectComponent`] can then be retrieved when needed.
///
/// Creating a custom [`ReflectComponent`] may be useful if you need to create new component types
/// at runtime, for example, for scripting implementations.
///
/// By creating a custom [`ReflectComponent`] and inserting it into a type's
/// [`TypeRegistration`][bevy_reflect::TypeRegistration],
/// you can modify the way that reflected components of that type will be inserted into the Bevy
/// world.
#[derive(Clone)]
pub struct ReflectComponentFns {
    component_id: fn(&Components) -> ComponentId,
    from_ptr: unsafe fn(Ptr) -> &dyn Reflect,
    from_ptr_mut: unsafe fn(PtrMut) -> &mut dyn Reflect,
}

impl ReflectComponentFns {
    /// Get the default set of [`ReflectComponentFns`] for a specific component type using its
    /// [`FromType`] implementation.
    ///
    /// This is useful if you want to start with the default implementation before overriding some
    /// of the functions to create a custom implementation.
    pub fn new<T: Component + Reflect>() -> Self {
        <ReflectComponent as FromType<T>>::from_type().0
    }
}

impl ReflectComponent {
    /// Create a custom implementation of [`ReflectComponent`].
    ///
    /// This is an advanced feature,
    /// useful for scripting implementations,
    /// that should not be used by most users
    /// unless you know what you are doing.
    ///
    /// Usually you should derive [`Reflect`] and add the `#[reflect(Component)]` component
    /// to generate a [`ReflectComponent`] implementation automatically.
    ///
    /// See [`ReflectComponentFns`] for more information.
    pub fn new(fns: ReflectComponentFns) -> Self {
        Self(fns)
    }

    /// The underlying function pointers implementing methods on `ReflectComponent`.
    ///
    /// This is useful when you want to keep track locally of an individual
    /// function pointer.
    ///
    /// Calling [`TypeRegistry::get`] followed by
    /// [`TypeRegistration::data::<ReflectComponent>`] can be costly if done several
    /// times per frame. Consider cloning [`ReflectComponent`] and keeping it
    /// between frames, cloning a `ReflectComponent` is very cheap.
    ///
    /// If you only need a subset of the methods on `ReflectComponent`,
    /// use `fn_pointers` to get the underlying [`ReflectComponentFns`]
    /// and copy the subset of function pointers you care about.
    ///
    /// [`TypeRegistration::data::<ReflectComponent>`]: bevy_reflect::TypeRegistration::data
    /// [`TypeRegistry::get`]: bevy_reflect::TypeRegistry::get
    pub fn fn_pointers(&self) -> &ReflectComponentFns {
        &self.0
    }
    /// Gets the value of this [`Component`] type from the entity as a reflected reference.
    pub fn reflect<'a>(&self, entity: EntityRef<'a>) -> Option<&'a dyn Reflect> {
        let id: ComponentId = (self.0.component_id)(entity.world_components());
        let ptr = entity.get_by_id(id)?;
        // SAFETY:
        // - `id` is the component id for type `C`.
        // - `ptr` points to something of type `C`.
        Some(unsafe { (self.0.from_ptr)(ptr) })
    }
    /// Gets the value of this [`Component`] type from the entity as a mutable reflected reference.
    pub fn reflect_mut<'a>(&self, entity: &'a mut EntityWorldMut) -> Option<Mut<'a, dyn Reflect>> {
        let id: ComponentId = (self.0.component_id)(entity.world_components());
        let ptr = entity.get_mut_by_id(id)?;
        // SAFETY:
        // - `id` is the component id for type `C`.
        // - `ptr` points to something of type `C`.
        let reflect = ptr.map_unchanged(|ptr| unsafe { (self.0.from_ptr_mut)(ptr) });
        Some(reflect)
    }
    /// # Safety
    /// This method does not prevent you from having two mutable pointers to the same data,
    /// violating Rust's aliasing rules. To avoid this:
    /// * Only call this method with a [`UnsafeEntityCell`] that may be used to mutably access the component on the entity `entity`
    /// * Don't call this method more than once in the same scope for a given [`Component`].
    pub unsafe fn reflect_unchecked_mut<'a>(
        &self,
        entity: UnsafeEntityCell<'a>,
    ) -> Option<Mut<'a, dyn Reflect>> {
        let id: ComponentId = (self.0.component_id)(entity.world().components());
        let ptr = unsafe { entity.get_mut_by_id(id)? };
        // SAFETY:
        // - `id` is the component id for type `C`.
        // - `ptr` points to something of type `C`.
        let reflect = ptr.map_unchanged(|ptr| unsafe { (self.0.from_ptr_mut)(ptr) });
        Some(reflect)
    }

    pub fn apply(&self, entity: &mut EntityWorldMut, field: &dyn Reflect) {
        let mut component = self.reflect_mut(entity).unwrap();
        component.apply(field);
    }
    pub fn apply_or_insert(&self, entity: &mut EntityWorldMut, field: &dyn Reflect) {
        // TODO(bug): this doesn't insert
        self.apply(entity, field);
    }
    pub fn insert(&self, entity: &mut EntityWorldMut, field: &dyn Reflect) {
        // TODO(bug): this doesn't insert
        self.apply(entity, field);
    }
    pub fn remove(&self, _entity: &mut EntityWorldMut) {
        todo!("TODO(bug): this doesn't remove anything");
    }
}

impl<C: Component + Reflect> FromType<C> for ReflectComponent {
    fn from_type() -> Self {
        ReflectComponent(ReflectComponentFns {
            component_id: |components| components.component_id::<C>().unwrap(),

            from_ptr: |ptr| {
                // SAFE: only called from `as_reflect`, where the `ptr` is guaranteed to be of type `C`,
                // and `as_reflect_ptr`, where the caller promises to call it with type `C`
                unsafe { ptr.deref::<C>() as &dyn Reflect }
            },
            from_ptr_mut: |ptr| {
                // SAFE: only called from `as_reflect_mut`, where the `ptr` is guaranteed to be of type `C`,
                // and `as_reflect_ptr_mut`, where the caller promises to call it with type `C`
                unsafe { ptr.deref_mut::<C>() as &mut dyn Reflect }
            },
        })
    }
}
