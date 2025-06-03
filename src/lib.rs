pub mod prelude {
    pub use lump_core::error::LumpUnknownError;
    pub use lump_core::prelude::*;
}

pub mod schedules {
    use lump_core::{error::LumpUnknownError, schedule::ScheduleLabel};

    #[derive(Copy, Clone)]
    pub struct Startup;
    impl ScheduleLabel for Startup {
        type SystenIn = ();
        type SystemOut = Result<(), LumpUnknownError>;
    }
}
