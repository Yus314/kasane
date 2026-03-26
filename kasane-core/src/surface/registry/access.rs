//! Registration, lifecycle, and query methods.

use compact_str::CompactString;

use crate::plugin::PluginId;

use super::*;

impl SurfaceRegistry {
    /// Register a surface after validating its static contract.
    pub fn try_register(
        &mut self,
        surface: Box<dyn Surface>,
    ) -> Result<(), SurfaceRegistrationError> {
        self.try_register_for_owner(surface, None)
    }

    /// Register a surface owned by the given plugin after validating its static contract.
    pub fn try_register_for_owner(
        &mut self,
        surface: Box<dyn Surface>,
        owner_plugin: Option<PluginId>,
    ) -> Result<(), SurfaceRegistrationError> {
        let descriptor = SurfaceDescriptor::from_surface(surface.as_ref())?;

        if let Some(existing) = self.surfaces.get(&descriptor.surface_id) {
            return Err(SurfaceRegistrationError::DuplicateSurfaceId {
                surface_id: descriptor.surface_id,
                existing_surface_key: existing.descriptor.surface_key.clone(),
                new_surface_key: descriptor.surface_key.clone(),
            });
        }
        if self
            .surface_ids_by_key
            .contains_key(descriptor.surface_key.as_str())
        {
            return Err(SurfaceRegistrationError::DuplicateSurfaceKey {
                surface_key: descriptor.surface_key.clone(),
            });
        }
        for slot in &descriptor.declared_slots {
            if let Some(existing_id) = self.slot_owners_by_name.get(slot.name.as_str()) {
                let existing_surface_key = self
                    .surfaces
                    .get(existing_id)
                    .map(|entry| entry.descriptor.surface_key.clone())
                    .unwrap_or_else(|| CompactString::const_new("<unknown>"));
                return Err(SurfaceRegistrationError::DuplicateDeclaredSlot {
                    slot_name: slot.name.clone(),
                    existing_surface_key,
                    new_surface_key: descriptor.surface_key.clone(),
                });
            }
        }

        let surface_id = descriptor.surface_id;
        let surface_key = descriptor.surface_key.clone();
        for slot in &descriptor.declared_slots {
            self.slot_owners_by_name
                .insert(slot.name.clone(), surface_id);
        }
        self.surface_ids_by_key.insert(surface_key, surface_id);
        self.surfaces.insert(
            surface_id,
            RegisteredSurface {
                surface,
                descriptor,
                owner_plugin,
                session_binding: None,
            },
        );
        Ok(())
    }

    /// Register a surface and panic if validation fails.
    #[track_caller]
    pub fn register(&mut self, surface: Box<dyn Surface>) {
        if let Err(err) = self.try_register(surface) {
            panic!("surface registration failed: {err:?}");
        }
    }

    /// Remove a surface by ID. Also cleans up any session binding.
    pub fn remove(&mut self, id: SurfaceId) -> Option<Box<dyn Surface>> {
        let entry = self.surfaces.remove(&id)?;
        self.surface_ids_by_key
            .remove(entry.descriptor.surface_key.as_str());
        for slot in &entry.descriptor.declared_slots {
            self.slot_owners_by_name.remove(slot.name.as_str());
        }
        if let Some(binding) = &entry.session_binding {
            self.session_to_surface.remove(&binding.session_id);
        }
        Some(entry.surface)
    }

    /// Remove every surface owned by a plugin from the registry, preserving workspace nodes.
    pub fn remove_owned_surfaces(&mut self, owner: &PluginId) -> Vec<SurfaceId> {
        let mut surface_ids: Vec<_> = self
            .surfaces
            .iter()
            .filter(|(_, entry)| entry.owner_plugin.as_ref() == Some(owner))
            .map(|(surface_id, _)| *surface_id)
            .collect();
        surface_ids.sort_by_key(|surface_id| surface_id.0);
        for surface_id in &surface_ids {
            let _ = self.remove(*surface_id);
        }
        surface_ids
    }

    /// Get a reference to a surface by ID.
    pub fn get(&self, id: SurfaceId) -> Option<&dyn Surface> {
        self.surfaces.get(&id).map(|entry| entry.surface.as_ref())
    }

    /// Get a mutable reference to a surface by ID.
    pub fn get_mut(&mut self, id: SurfaceId) -> Option<&mut dyn Surface> {
        self.surfaces
            .get_mut(&id)
            .map(|entry| entry.surface.as_mut())
    }

    /// Get a registration-time descriptor by surface ID.
    pub fn descriptor(&self, id: SurfaceId) -> Option<&SurfaceDescriptor> {
        self.surfaces.get(&id).map(|entry| &entry.descriptor)
    }

    /// Get the owning plugin for a surface, if it is plugin-provided.
    pub fn surface_owner_plugin(&self, id: SurfaceId) -> Option<&PluginId> {
        self.surfaces
            .get(&id)
            .and_then(|entry| entry.owner_plugin.as_ref())
    }

    /// Resolve a surface key to its surface ID.
    pub fn surface_id_by_key(&self, surface_key: &str) -> Option<SurfaceId> {
        self.surface_ids_by_key.get(surface_key).copied()
    }

    /// Number of registered surfaces.
    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }

    /// Collect all declared slots across all registered surfaces.
    pub fn all_declared_slots(&self) -> Vec<(SurfaceId, &SlotDeclaration)> {
        let mut result = Vec::new();
        for (id, entry) in &self.surfaces {
            for slot in &entry.descriptor.declared_slots {
                result.push((*id, slot));
            }
        }
        result
    }

    /// Find the surface that declares a given slot name.
    pub fn slot_owner(&self, slot_name: &str) -> Option<SurfaceId> {
        self.slot_owners_by_name.get(slot_name).copied()
    }
}
