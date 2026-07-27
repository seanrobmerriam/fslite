use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

const REQUEST_ID_HEADER: &str = "x-request-id";

/// Ensures every request carries an `x-request-id`, generating one when
/// absent, and echoes it back on the response for client-side correlation.
pub async fn request_id(mut request: Request, next: Next) -> Response {
    let id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::now_v7().to_string());

    request
        .extensions_mut()
        .insert(RequestId(id.clone()));

    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}

/// The per-request correlation id, available to handlers via `Extension<RequestId>`.
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

/// A `tower-http` layer logging method, path, status, and latency per request.
pub fn trace_layer() -> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
}
