use crate::app::ConfigureApp;

pub trait Plugin {
    fn setup(self, app: impl ConfigureApp);
}
