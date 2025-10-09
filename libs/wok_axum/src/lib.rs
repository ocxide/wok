mod layer {
    use std::convert::Infallible;

    use axum::{extract::Request, response::IntoResponse, routing::Route};
    use tower_service::Service;
    use wok::prelude::ResMut;
    use wok_core::schedule::{ConfigureObjects, ScheduleLabel};

    use crate::RouterRoot;

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
            let mut router = world.state.get::<ResMut<'_, RouterRoot>>();
            let router = router.0.as_mut().expect("router");
            take_mut::take(router, move |r| r.layer(layer));
        }
    }
}

pub mod crud;
mod handler;

use wok::{
    plugin::Plugin,
    prelude::*,
    remote_gateway::{RemoteWorldPorts, RemoteWorldRef},
};

pub use layer::*;
pub use nest_route::*;
pub use single_route::*;

type Router = axum::Router<RemoteWorldPorts>;

#[derive(Resource)]
#[resource(mutable = true)]
pub struct RouterRoot(pub Option<Router>);

impl Default for RouterRoot {
    fn default() -> Self {
        Self(Some(Router::default()))
    }
}

/// Axum integration plugin
/// Setups basics resources for Axum integration, use before any other Axum plugins & Route
/// .add_system
/// ```rust
/// use wok::prelude::*;
/// use wok_axum::{AxumPlugin, Route, get};
///
/// App::default()
///     .add_plugin(AxumPlugin)
///     .add_system(Route("/"), get(my_handler));
///
/// async fn my_handler() {}
/// ```
pub struct AxumPlugin;

impl Plugin for AxumPlugin {
    fn setup(self, app: &mut wok::prelude::App) {
        app.init_resource::<RouterRoot>();
    }
}

/// Axum scope plugin
/// Nest routes within a route scope
/// ```rust
/// use wok::prelude::*;
/// use wok_axum::{AxumPlugin, AxumNestPlugin, Route, get};
///
/// App::default()
///     .add_plugin(AxumPlugin)
///     .add_plugin(MyRoutes)
///     .add_plugin(AxumNestPlugin("/api", MyRoutes));
///
/// struct MyRoutes;
/// impl Plugin for MyRoutes {
///     fn setup(self, app: &mut wok::prelude::App) {
///         app.add_system(Route("/"), get(my_handler));
///     }
/// }
///
/// async fn my_handler() {}
pub struct AxumNestPlugin<P: Plugin>(pub &'static str, pub P);

impl<P: Plugin> Plugin for AxumNestPlugin<P> {
    fn setup(self, app: &mut wok::prelude::App) {
        let parent = app
            .world_mut()
            .state
            .take_resource::<RouterRoot>()
            .expect("missing previous `AxumPlugin`");

        app.init_resource::<RouterRoot>();
        app.add_plugin(self.1);

        let child = app
            .world_mut()
            .state
            .take_resource::<RouterRoot>()
            .expect("Unexpected state, not recovering `RouterRoot` is not allowed");

        let (Some(parent_router), Some(child_router)) = (parent.0, child.0) else {
            panic!("Unexpected state, `RouterRoot` is not defined");
        };

        let new_router = parent_router.nest(self.0, child_router);

        app.insert_resource(RouterRoot(Some(new_router)));
    }
}

mod single_route {
    use axum::routing::{MethodFilter, MethodRouter};
    use wok::prelude::ResMut;
    use wok::{
        prelude::{IntoSystem, System},
        remote_gateway::RemoteWorldPorts,
    };
    use wok_core::schedule::{ScheduleConfigure, ScheduleLabel};
    use wok_core::world::WorldCenter;

    use crate::RouterRoot;
    use crate::handler::AxumRouteSystem;

    pub struct MethodRouterMut<'m>(&'m mut MethodRouter<RemoteWorldPorts>);

    impl<'m> MethodRouterMut<'m> {
        pub fn mutate(
            &mut self,
            f: impl FnOnce(MethodRouter<RemoteWorldPorts>) -> MethodRouter<RemoteWorldPorts>,
        ) {
            take_mut::take(self.0, f);
        }

        pub fn on<T: 'static>(
            &mut self,
            method: MethodFilter,
            handler: impl axum::handler::Handler<T, RemoteWorldPorts>,
        ) {
            self.mutate(move |router| router.on(method, handler));
        }
    }

    macro_rules! method_filter_fn {
        (self, $name:ident : $method:ident) => {
            fn $name<S, SMarker, HMarker, OMarker>(self, system: S) -> impl ConfigureRoute
            where
                OnRoute<S::System, HMarker>: ConfigureRoute,
                S: IntoSystem<SMarker>,
                S::System: System<Out: crate::handler::WokIntoResponse<OMarker>>,
            {
                let route = OnRoute {
                    system: system.into_system(),
                    _marker: std::marker::PhantomData,
                    method: MethodFilter::$method,
                };

                (self, route)
            }
        };

        ($name:ident : $method:ident) => {
            pub fn $name<S, SMarker, HMarker, OMarker>(system: S) -> impl ConfigureRoute
            where
                OnRoute<S::System, HMarker>: ConfigureRoute,
                S: IntoSystem<SMarker>,
                S::System: System<Out: crate::handler::WokIntoResponse<OMarker>>,
            {
                OnRoute {
                    system: system.into_system(),
                    _marker: std::marker::PhantomData,
                    method: MethodFilter::$method,
                }
            }
        };
    }

    pub trait ConfigureRoute: Sized {
        fn cfg(self, router: &mut MethodRouterMut<'_>, world: &mut WorldCenter);
        fn into_route(self, world: &mut WorldCenter) -> MethodRouter<RemoteWorldPorts> {
            let mut router = MethodRouter::new();
            self.cfg(&mut MethodRouterMut(&mut router), world);

            router
        }

        method_filter_fn!(self, get: GET);
        method_filter_fn!(self, post: POST);
        method_filter_fn!(self, put: PUT);
        method_filter_fn!(self, patch: PATCH);
        method_filter_fn!(self, delete: DELETE);
        method_filter_fn!(self, head: HEAD);
        method_filter_fn!(self, options: OPTIONS);
        method_filter_fn!(self, trace: TRACE);
    }

    pub struct OnRoute<S, Marker> {
        system: S,
        _marker: std::marker::PhantomData<fn(Marker)>,
        method: MethodFilter,
    }

    impl<Marker: 'static, S> ConfigureRoute for OnRoute<S, Marker>
    where
        S: System,
        AxumRouteSystem<S>: axum::handler::Handler<Marker, RemoteWorldPorts>,
    {
        fn cfg(self, router: &mut MethodRouterMut<'_>, world: &mut WorldCenter) {
            let system = world.register_system(self.system);
            let handler = AxumRouteSystem(system);

            router.on(self.method, handler);
        }
    }

    macro_rules! impl_configure_route {
        ($($name:ident),*) => {
            impl<$($name: ConfigureRoute),*> ConfigureRoute for ($($name,)*) {
                fn cfg(self, router: &mut MethodRouterMut<'_>, world: &mut WorldCenter) {
                    #[allow(non_snake_case)]
                    let ($($name,)*) = self;
                    $(
                        $name.cfg(router, world);
                    )*
                }
            }
        };
    }

    impl_configure_route!(R1, R2);
    impl_configure_route!(R1, R2, R3);
    impl_configure_route!(R1, R2, R3, R4);
    impl_configure_route!(R1, R2, R3, R4, R5);
    impl_configure_route!(R1, R2, R3, R4, R5, R6);
    impl_configure_route!(R1, R2, R3, R4, R5, R6, R7);
    impl_configure_route!(R1, R2, R3, R4, R5, R6, R7, R8);
    impl_configure_route!(R1, R2, R3, R4, R5, R6, R7, R8, R9);
    impl_configure_route!(R1, R2, R3, R4, R5, R6, R7, R8, R9, R10);

    method_filter_fn!(get: GET);
    method_filter_fn!(head: HEAD);
    method_filter_fn!(options: OPTIONS);
    method_filter_fn!(post: POST);
    method_filter_fn!(put: PUT);
    method_filter_fn!(delete: DELETE);
    method_filter_fn!(patch: PATCH);

    pub struct Route<'r>(pub &'r str);
    impl<'r> ScheduleLabel for Route<'r> {}

    impl<'r, R: ConfigureRoute> ScheduleConfigure<R, ()> for Route<'r> {
        fn add(self, world: &mut wok_core::world::World, thing: R) {
            let mut router = world.state.get::<ResMut<'_, RouterRoot>>();
            let router = router.0.as_mut().expect("router");

            let route = thing.into_route(&mut world.center);
            take_mut::take(router, move |r| r.route(self.0, route));
        }
    }
}

mod nest_route {
    use wok::prelude::ResMut;
    use wok_core::{schedule::ScheduleConfigure, world::WorldCenter};

    use crate::{RouterRoot, single_route::ConfigureRoute};

    pub trait ConfigureRoutesSet: Sized {
        fn route(self, path: &'static str, routes: impl ConfigureRoute) -> impl ConfigureRoutesSet {
            RouteCfLayer {
                prev_layer: self,
                path,
                into_route: routes,
            }
        }

        fn cfg(self, router: super::Router, world: &mut WorldCenter) -> super::Router;
    }

    pub fn routes(path: &'static str, routes: impl ConfigureRoute) -> impl ConfigureRoutesSet {
        RouteCfLayer {
            prev_layer: Empty,
            path,
            into_route: routes,
        }
    }

    struct RouteCfLayer<L1, L2> {
        prev_layer: L1,
        path: &'static str,
        into_route: L2,
    }

    struct Empty;
    impl ConfigureRoutesSet for Empty {
        fn cfg(self, router: super::Router, _world: &mut WorldCenter) -> super::Router {
            router
        }
    }

    impl<L1: ConfigureRoutesSet, L2: ConfigureRoute> ConfigureRoutesSet for RouteCfLayer<L1, L2> {
        fn cfg(self, router: crate::Router, world: &mut WorldCenter) -> crate::Router {
            let router = self.prev_layer.cfg(router, world);
            let route = self.into_route.into_route(world);

            router.route(self.path, route)
        }
    }

    pub struct NestRoutes(pub &'static str);

    impl<L: ConfigureRoutesSet> ScheduleConfigure<L, ()> for NestRoutes {
        fn add(self, world: &mut wok_core::world::World, thing: L) {
            let mut router = world.state.get::<ResMut<'_, RouterRoot>>();
            let router = router.0.as_mut().expect("router");

            let axum_router = thing.cfg(axum::Router::new(), &mut world.center);
            take_mut::take(router, move |r| r.nest(self.0, axum_router));
        }
    }
}

#[derive(Debug, serde::Deserialize, Clone)]
#[serde(untagged)]
pub enum Addr {
    // try to deserialize plain socket addr first
    Plain(std::net::SocketAddr),
    // then allow custom hosts
    Unresolved(String),
    HostPort(String, u16),
}

#[derive(wok::prelude::Resource, serde::Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum SocketAddrs {
    Single(Addr),
    Multiple(Vec<Addr>),
}

/// Main runtime for wok_axum
/// Requires tthe he `AxumPlugin` & a `SocketAddrs` resource
pub async fn serve(
    world: RemoteWorldRef<'_>,
    addrs: Res<'_, SocketAddrs>,
    mut router: ResMut<'_, RouterRoot>,
) -> Result<(), WokUnknownError> {
    let world = world.upgrade().expect("the app to be active");

    let router = router
        .0
        .take()
        .expect("to have `AxumPlugin`")
        .with_state(world);

    async fn lookup_addr(addr: &Addr) -> std::io::Result<Vec<std::net::SocketAddr>> {
        match addr {
            Addr::Plain(addr) => Ok(vec![*addr]),
            Addr::Unresolved(host) => tokio::net::lookup_host(host)
                .await
                .map(|addrs| addrs.collect()),
            Addr::HostPort(host, port) => tokio::net::lookup_host((host.as_str(), *port))
                .await
                .map(|addrs| addrs.collect()),
        }
    }

    let addrs = match addrs.as_ref() {
        SocketAddrs::Single(addr) => lookup_addr(addr).await?,
        SocketAddrs::Multiple(addrs) => {
            let mut res = vec![];
            let set = addrs.iter().map(lookup_addr);
            for addrs in futures::future::join_all(set).await {
                res.extend(addrs?);
            }

            res
        }
    };

    println!("listening on: {addrs:?}");

    let listener = tokio::net::TcpListener::bind(addrs.as_slice()).await?;
    axum::serve(listener, router).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(dead_code)]
    use super::*;

    struct TestPlugin;
    impl Plugin for TestPlugin {
        fn setup(self, app: &mut wok::prelude::App) {
            app.add_systems(Route("/hello"), get(simple_route).post(parse_req))
                .add_systems(Route("/hello/{data}"), get(parse_req_part))
                .add_systems(Route("/error"), get(input_less_err));
        }
    }

    async fn simple_route() -> &'static str {
        "hello"
    }

    async fn input_less_err() -> Result<&'static str, wok::prelude::WokUnknownError> {
        Ok("hello")
    }

    async fn parse_req(_: In<String>) {}

    async fn parse_req_part(_: In<axum::extract::Path<String>>) {}
}
