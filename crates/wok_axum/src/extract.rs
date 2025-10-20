pub enum JsonGatedRejection {
    Json(axum::extract::rejection::JsonRejection),
    Valigate(valigate::Error),
}

impl axum::response::IntoResponse for JsonGatedRejection {
    fn into_response(self) -> axum::response::Response {
        match self {
            JsonGatedRejection::Json(e) => e.into_response(),
            JsonGatedRejection::Valigate(e) => {
                (axum::http::StatusCode::BAD_REQUEST, axum::extract::Json(e)).into_response()
            }
        }
    }
}

pub struct JsonG<T>(pub T);

impl<S: Send + Sync + 'static, T> axum::extract::FromRequest<S> for JsonG<T>
where
    T: valigate::Valid,
    T::In: serde::de::DeserializeOwned,
{
    type Rejection = JsonGatedRejection;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        let input = axum::Json::<T::In>::from_request(req, state)
            .await
            .map_err(JsonGatedRejection::Json)?;

        match T::parse(input.0) {
            Ok(value) => Ok(JsonG(value)),
            Err(e) => Err(JsonGatedRejection::Valigate(e)),
        }
    }
}
