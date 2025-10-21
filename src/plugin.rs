use crate::app::App;

pub trait Plugin {
    fn setup(self, app: &mut App);
}

impl Plugin for () {
    fn setup(self, _: &mut App) {}
}
