use std::num::NonZero;

use super::access::SystemLock;

#[derive(Copy, Clone, Debug, Hash, Eq, PartialEq)]
pub struct SystemId(NonZero<usize>);

impl SystemId {
    pub const fn local(self) -> LocalSystemId {
        LocalSystemId(self.0)
    }
}

pub struct LocalSystemId(NonZero<usize>);

impl LocalSystemId {
    /// Asume the system id is valid for this world
    pub fn cast_global(self) -> SystemId {
        SystemId(self.0)
    }
}

#[derive(Debug, Default)]
pub struct SystemsMeta<D>(Vec<D>);

impl<D> SystemsMeta<D> {
    pub fn add(&mut self, meta: D) -> LocalSystemId {
        self.0.push(meta);
        LocalSystemId(NonZero::new(self.0.len()).expect("vec len to be nonzero"))
    }

    pub fn get(&self, id: LocalSystemId) -> Option<&D> {
        self.0.get(id.0.get() - 1)
    }
}

#[derive(Default)]
pub struct SystemsRw(SystemsMeta<SystemLock>);

impl SystemsRw {
    #[inline]
    pub fn add(&mut self, access: SystemLock) -> SystemId {
        self.0.add(access).cast_global()
    }

    #[inline]
    pub fn get(&self, id: SystemId) -> Option<&SystemLock> {
        self.0.get(id.local())
    }
}

