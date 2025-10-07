use core::panic;
use std::{
    collections::{HashMap, HashSet},
    num::NonZero,
};

use crate::resources::ResourceId;

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub enum AccessMode {
    Read,
    Write,
}

#[derive(Debug)]
pub enum WorldAccess {
    Read(NonZero<usize>),
    Write,
}

#[derive(Default, Debug)]
pub struct WorldLocks {
    resources: HashMap<ResourceId, WorldAccess>,
}

impl WorldLocks {
    /// # Safety
    /// Caller must ensure the access is valid, since it would leave an inconsistent state
    pub unsafe fn do_access(&mut self, access: &SystemLock) {
        for &(resource_id, mode) in access.resources.iter() {
            self.resources
                .entry(resource_id)
                .and_modify(|access| {
                    // We know the access is read because we checked above
                    if let WorldAccess::Read(count) = access {
                        *count = NonZero::new(count.get() + 1).unwrap();
                    }
                })
                .or_insert_with(|| match mode {
                    AccessMode::Read => WorldAccess::Read(NonZero::new(1).unwrap()),
                    AccessMode::Write => WorldAccess::Write,
                });
        }
    }

    pub fn try_access(&mut self, access: &SystemLock) -> Result<(), ()> {
        if !self.can_lock(access) {
            return Err(());
        }

        // Safety: we know the access is valid
        unsafe {
            self.do_access(access);
        }

        Ok(())
    }

    pub fn release_access(&mut self, access: &SystemLock) {
        for (resource_id, mode) in access.resources.iter() {
            let Some(access) = self.resources.get_mut(resource_id) else {
                continue;
            };

            match (access, mode) {
                (WorldAccess::Read(count), AccessMode::Read) => {
                    let value = NonZero::new(count.get() - 1);

                    if let Some(value) = value {
                        *count = value;
                    } else {
                        self.resources.remove(resource_id);
                    }
                }
                (WorldAccess::Write, AccessMode::Write) => {
                    self.resources.remove(resource_id);
                }
                (world, system) => {
                    panic!("access are not compatible {:?} != {:?}", world, system)
                }
            }
        }
    }

    pub(crate) fn can_lock(&self, access: &SystemLock) -> bool {
        for (resource_id, mode) in access.resources.iter() {
            let access = self.resources.get(resource_id);
            let allow = matches!(
                (access, mode),
                (None, _) | (Some(WorldAccess::Read(_)), AccessMode::Read)
            );

            if !allow {
                return false;
            }
        }

        true
    }

    pub fn is_clean(&self) -> bool {
        self.resources.is_empty()
    }
}

#[derive(Default)]
pub struct SystemLock {
    resources: HashSet<(ResourceId, AccessMode)>,
}

pub enum AlreadyRegistered {
    Read,
    Write,
}

impl SystemLock {
    pub fn has_resource_write(&self, resource: ResourceId) -> bool {
        self.resources.contains(&(resource, AccessMode::Write))
    }

    pub fn has_resource_read(&self, resource: ResourceId) -> bool {
        self.resources.contains(&(resource, AccessMode::Read))
    }

    pub fn register_resource_read(
        &mut self,
        resource: ResourceId,
    ) -> Result<(), AlreadyRegistered> {
        if self.has_resource_write(resource) {
            return Err(AlreadyRegistered::Write);
        }

        self.resources.insert((resource, AccessMode::Read));
        Ok(())
    }

    pub fn register_resource_write(
        &mut self,
        resource: ResourceId,
    ) -> Result<(), AlreadyRegistered> {
        if self.has_resource_read(resource) {
            return Err(AlreadyRegistered::Read);
        }

        self.resources.insert((resource, AccessMode::Write));
        Ok(())
    }
}
