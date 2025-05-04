use crate::Dust;

pub trait System: Send + Sync + 'static {
    type Input;
    type Output: Send + Sync + 'static;

    fn run(
        &self,
        dust: &Dust,
        input: Self::Input,
    ) -> impl Future<Output = Self::Output> + Send + 'static;
}

pub trait IntoSystem<Marker> {
    type In;
    type Out;
    type System: System<Input = Self::In, Output = Self::Out>;

    fn into_system(self) -> Self::System;
}

