use std::convert::Infallible;

use axum::{
    RequestExt,
    extract::{FromRequestParts, Request},
    response::IntoResponse,
    routing::Route,
};
use tower_service::Service;
use wok::{
    prelude::{
        BorrowMutParam, ConfigureWorld, IntoSystem, ProtoSystem, ResMut, System, SystemInput,
    },
    remote_gateway::RemoteWorldPorts,
};
use wok_core::schedule::{ConfigureObjects, ScheduleConfigure, ScheduleLabel};

use crate::{RouterRoot, handler::WokIntoResponse};

pub struct Layer;

impl ScheduleLabel for Layer {}

impl<L> ConfigureObjects<L, ()> for Layer
where
    L: tower_layer::Layer<Route> + Clone + Send + Sync + 'static,
    L::Service: Service<Request> + Clone + Send + Sync + 'static,
    <L::Service as Service<Request>>::Response: IntoResponse + 'static,
    <L::Service as Service<Request>>::Error: Into<Infallible> + 'static,
    <L::Service as Service<Request>>::Future: Send + 'static,
{
    fn add_objs(self, world: &mut wok_core::world::World, layer: L) {
        let mut router = world.get::<ResMut<'_, RouterRoot>>();
        let router = router.0.as_mut().expect("router");
        take_mut::take(router, move |r| r.layer(layer));
    }
}

impl<S, P, Marker, WMarker> ScheduleConfigure<S, (Marker, WMarker, P)> for Layer
where
    P: FromRequestParts<()> + Send + 'static,
    S: IntoSystem<Marker>,
    S::System: System<In = MiddlPartsIn<P>, Out: WokIntoResponse<WMarker>>
        + ProtoSystem<Param: BorrowMutParam>,
{
    fn add(self, world: &mut wok_core::world::World, system: S) {
        let system = system.into_system();
        let system = world.register_system(system);

        let handler = move |world: axum::extract::Extension<RemoteWorldPorts>,
                            mut req: axum::extract::Request,
                            next: axum::middleware::Next| {
            let system = system.clone();
            async move {
                let parts = match req.extract_parts::<P>().await {
                    Ok(parts) => parts,
                    Err(err) => {
                        return err.into_response();
                    }
                };

                let permit = world.reserver().reserve(system.entry_ref()).await;

                let out = permit.task().run(MiddlPartsIn(parts, req, next)).await;

                out.wok_into_response()
            }
        };

        world.add_objs(Layer, axum::middleware::from_fn(handler));
    }
}

pub struct MiddlPartsIn<P: FromRequestParts<()> + Send>(
    pub P,
    pub axum::extract::Request,
    pub axum::middleware::Next,
);

impl<P: FromRequestParts<()> + Send> SystemInput for MiddlPartsIn<P> {
    type Inner<'i> = MiddlPartsIn<P>;
    type Wrapped<'i> = MiddlPartsIn<P>;

    fn wrap(this: Self::Inner<'_>) -> Self::Wrapped<'_> {
        this
    }
}
