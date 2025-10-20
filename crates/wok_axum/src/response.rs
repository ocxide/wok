use axum::response::{IntoResponse, NoContent};

#[derive(Debug, Clone)]
pub enum Created<T> {
    Created(T),
    BadRequest,
    Conflict,
}

impl<T: IntoResponse> IntoResponse for Created<T> {
    fn into_response(self) -> axum::response::Response {
        match self {
            Created::Created(t) => (axum::http::StatusCode::CREATED, t).into_response(),
            Created::BadRequest => axum::http::StatusCode::BAD_REQUEST.into_response(),
            Created::Conflict => axum::http::StatusCode::CONFLICT.into_response(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Maybe<T>(pub Option<T>);

impl<T> From<Option<T>> for Maybe<T> {
    fn from(value: Option<T>) -> Self {
        Maybe(value)
    }
}

impl<T> From<Maybe<T>> for Option<T> {
    fn from(value: Maybe<T>) -> Self {
        value.0
    }
}

impl<T: IntoResponse> IntoResponse for Maybe<T> {
    fn into_response(self) -> axum::response::Response {
        match self.0 {
            Some(t) => t.into_response(),
            None => axum::http::StatusCode::NOT_FOUND.into_response(),
        }
    }
}

pub enum Permitted<T> {
    Ok(T),
    Unauthorized,
    Forbidden,
}

impl<T: IntoResponse> IntoResponse for Permitted<T> {
    fn into_response(self) -> axum::response::Response {
        match self {
            Permitted::Ok(t) => t.into_response(),
            Permitted::Unauthorized => axum::http::StatusCode::UNAUTHORIZED.into_response(),
            Permitted::Forbidden => axum::http::StatusCode::FORBIDDEN.into_response(),
        }
    }
}

pub struct Deleted;

impl IntoResponse for Deleted {
    fn into_response(self) -> axum::response::Response {
        NoContent.into_response()
    }
}

pub enum Validated<T> {
    Valid(T),
    BadRequest,
}

impl<T: IntoResponse> IntoResponse for Validated<T> {
    fn into_response(self) -> axum::response::Response {
        match self {
            Validated::Valid(t) => t.into_response(),
            Validated::BadRequest => axum::http::StatusCode::BAD_REQUEST.into_response(),
        }
    }
}
