use crate::Dust;

pub type SystemFuture<S> = Box<dyn Future<Output = <S as System>::Out> + Send + 'static>;

pub trait System: Send + Sync + 'static {
    type In;
    type Out: Send + Sync + 'static;

    fn run(&self, dust: &Dust, input: Self::In) -> SystemFuture<Self>;
}

pub trait IntoSystem<Marker> {
    type System: System;

    fn into_system(self) -> Self::System;
}
