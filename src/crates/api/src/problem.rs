use axum::{
    Json,
    body::Body,
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    #[serde(rename = "type")]
    pub problem_type: &'static str,
    pub title: &'static str,
    pub status: u16,
    pub detail: &'static str,
    pub request_id: Option<String>,
}

impl Problem {
    fn response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, Json(self)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

pub async fn negotiate(request: Request, next: Next) -> Response {
    let accepts_json = request
        .headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_none_or(|value| {
            value.contains("*/*")
                || value.contains("application/json")
                || value.contains("application/problem+json")
        });
    if !accepts_json {
        return problem(
            &request,
            StatusCode::NOT_ACCEPTABLE,
            "not_acceptable",
            "The requested representation is not available.",
        );
    }
    if matches!(
        *request.method(),
        axum::http::Method::POST | axum::http::Method::PUT | axum::http::Method::PATCH
    ) && request
        .headers()
        .get(header::CONTENT_LENGTH)
        .is_some_and(|value| value != "0")
        && !request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"))
    {
        return problem(
            &request,
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "A JSON request body is required.",
        );
    }
    next.run(request).await
}

pub async fn not_found(request: Request<Body>) -> Response {
    problem(
        &request,
        StatusCode::NOT_FOUND,
        "not_found",
        "The requested API resource was not found.",
    )
}

fn problem(
    request: &Request,
    status: StatusCode,
    title: &'static str,
    detail: &'static str,
) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    Problem {
        problem_type: "https://pvlog.example/problems/http",
        title,
        status: status.as_u16(),
        detail,
        request_id,
    }
    .response()
}
