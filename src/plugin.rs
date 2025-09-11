use crate::app::App;

pub trait Plugin {
    fn setup(self, app: &mut App);
}
