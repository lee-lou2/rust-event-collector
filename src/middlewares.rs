use axum::response::IntoResponse;
use axum::{
    body::Body,
    http::{header::AUTHORIZATION, Request, StatusCode},
    middleware::Next,
};

// 인증 미들웨어
pub async fn auth_middleware(req: Request<Body>, next: Next) -> axum::response::Response {
    // ping 은 인증하지 않음
    if req.uri().path() == "/ping" {
        return next.run(req).await;
    }

    // API 키 확인
    let auth_header = req.headers().get(AUTHORIZATION);
    if let Some(value) = auth_header {
        if let Ok(auth_str) = value.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                let expected = std::env::var("API_KEY").unwrap_or_default();
                if token == expected {
                    return next.run(req).await;
                }
            }
        }
    }
    StatusCode::UNAUTHORIZED.into_response()
}
