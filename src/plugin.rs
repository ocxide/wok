use crate::app::ConfigureMoreWorld;

pub trait Plugin {
    fn setup(self, app: impl ConfigureMoreWorld);
}
