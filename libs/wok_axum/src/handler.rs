use axum::{
    extract::{FromRequest, FromRequestParts},
    response::IntoResponse,
};
use wok::{
    prelude::{BorrowMutParam, In, ProtoTaskSystem, ScopedFut, System, WokUnknownError},
    remote_gateway::RemoteWorldPorts,
};
use wok_core::world::gateway::SystemEntry;

#[derive(Clone)]
pub(crate) struct AxumRouteSystem<S>(pub(crate) SystemEntry<S>);

#[doc(hidden)]
pub struct WithReqInput;
impl<Input, RouteSystem, WokResponseMarker>
    axum::handler::Handler<(WithReqInput, WokResponseMarker, Input), RemoteWorldPorts>
    for AxumRouteSystem<RouteSystem>
where
    RouteSystem: ProtoTaskSystem<Param: BorrowMutParam>,
    RouteSystem: System<In = In<Input>, Out: WokIntoResponse<WokResponseMarker>>,
    Input: FromRequest<RemoteWorldPorts> + Send + Sync + 'static,
{
    type Future = ScopedFut<'static, axum::response::Response>;

    fn call(self, req: axum::extract::Request, state: RemoteWorldPorts) -> Self::Future {
        Box::pin(async move {
            let input = match Input::from_request(req, &state).await {
                Ok(value) => value,
                Err(rejection) => return rejection.into_response(),
            };

            state
                .reserver()
                .reserve(self.0.entry_ref())
                .await
                .task()
                .run(input)
                .await
                .wok_into_response()
        })
    }
}

#[doc(hidden)]
pub struct WithReqPartsInput;
impl<Input, RouteSystem, WokResponseMarker>
    axum::handler::Handler<(WithReqPartsInput, WokResponseMarker, Input), RemoteWorldPorts>
    for AxumRouteSystem<RouteSystem>
where
    RouteSystem: ProtoTaskSystem<Param: BorrowMutParam>,
    RouteSystem: System<In = In<Input>, Out: WokIntoResponse<WokResponseMarker>>,
    Input: FromRequestParts<RemoteWorldPorts> + Send + Sync + 'static,
{
    type Future = ScopedFut<'static, axum::response::Response>;

    fn call(self, req: axum::extract::Request, state: RemoteWorldPorts) -> Self::Future {
        Box::pin(async move {
            let input = match Input::from_request(req, &state).await {
                Ok(value) => value,
                Err(rejection) => return rejection.into_response(),
            };

            state
                .reserver()
                .reserve(self.0.entry_ref())
                .await
                .task()
                .run(input)
                .await
                .wok_into_response()
        })
    }
}

#[doc(hidden)]
pub struct NoInput;
impl<RouteSystem, WokResponseMarker>
    axum::handler::Handler<(NoInput, WokResponseMarker), RemoteWorldPorts>
    for AxumRouteSystem<RouteSystem>
where
    RouteSystem: ProtoTaskSystem<Param: BorrowMutParam>,
    RouteSystem: System<In = (), Out: WokIntoResponse<WokResponseMarker>>,
{
    type Future = ScopedFut<'static, axum::response::Response>;

    fn call(self, _req: axum::extract::Request, state: RemoteWorldPorts) -> Self::Future {
        Box::pin(async move {
            state
                .reserver()
                .reserve(self.0.entry_ref())
                .await
                .task()
                .run(())
                .await
                .wok_into_response()
        })
    }
}

pub trait WokIntoResponse<Marker> {
    fn wok_into_response(self) -> axum::response::Response;
}

#[doc(hidden)]
pub struct IsAxumResponse;

impl<T: IntoResponse> WokIntoResponse<IsAxumResponse> for T {
    fn wok_into_response(self) -> axum::response::Response {
        self.into_response()
    }
}

pub struct IsWokResult;
impl<T: IntoResponse> WokIntoResponse<IsWokResult> for Result<T, WokUnknownError> {
    fn wok_into_response(self) -> axum::response::Response {
        match self {
            Ok(value) => value.into_response(),
            Err(err) => {
                tracing::error!(%err);
                axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}
