use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tower::{Layer, Service};
use tracing::error;
use uuid::Uuid;

use crate::{
    app::App,
    cache::redis::RedisChx,
    core::services::ctx::CtxService,
    web::error::ErrorBody,
};
use crate::{core::services::token::TokenService, store::dbx::PgDbx};

/// Header used by global/root tokens to specify which workspace they are
/// operating on. Scoped tokens do not need to send this header — their
/// workspace is resolved from the JWT.
const WORKSPACE_ID_HEADER: &str = "X-Workspace-Id";

#[derive(Clone)]
pub struct CtxLayer {
    ctx_svc: Arc<CtxService<PgDbx, RedisChx>>,
}

impl CtxLayer {
    pub fn new(app_state: &App) -> Self {
        let ctx_svc = Arc::new(CtxService::new(
            app_state.sm.clone(),
            app_state.cm.clone(),
            app_state.svc_factory.clone(),
            app_state.config.clone(),
        ));
        Self { ctx_svc }
    }
}

impl<S> Layer<S> for CtxLayer {
    type Service = CtxMw<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CtxMw {
            inner,
            ctx_svc: self.ctx_svc.clone(),
        }
    }
}

#[derive(Clone)]
pub struct CtxMw<S> {
    inner: S,
    ctx_svc: Arc<CtxService<PgDbx, RedisChx>>,
}

/// Parse the `X-Workspace-Id` header as a UUID. Returns `None` if the header
/// is missing or unparseable.
fn parse_workspace_header(headers: &HeaderMap) -> Option<Uuid> {
    headers
        .get(WORKSPACE_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
}

impl<S> Service<Request<Body>> for CtxMw<S>
where
    S: Service<Request, Response = Response> + Send + 'static + Clone,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        let ctx_svc = self.ctx_svc.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Extract the parsed workspace header early (before ctx is moved).
            let header_ws = parse_workspace_header(req.headers());

            match ctx_svc.resolve_ctx(req.headers()).await {
                Ok(mut ctx) => {
                    // Resolve the operational workspace for global tokens.
                    if ctx.is_global_workspace().unwrap_or(false) {
                        match header_ws {
                            Some(ws) => ctx.set_scoped_ws_id(ws),
                            None => {
                                error!("X-Workspace-Id header required for global-scope tokens");
                                let body = ErrorBody {
                                    success: false,
                                    status: StatusCode::BAD_REQUEST.as_u16(),
                                    message: "X-Workspace-Id header required for global-scope tokens"
                                        .to_string(),
                                };
                                return Ok(
                                    (StatusCode::BAD_REQUEST, axum::Json(body)).into_response()
                                );
                            }
                        }
                    }
                    // Scoped tokens: scoped_ws_id is already the token's workspace.

                    req.extensions_mut().insert(ctx);
                    inner.call(req).await
                }
                Err(e) => {
                    let body = ErrorBody {
                        success: false,
                        status: StatusCode::UNAUTHORIZED.as_u16(),
                        message: "unauthorized".to_string(),
                    };

                    error!("{e}");

                    Ok((StatusCode::UNAUTHORIZED, axum::Json(body)).into_response())
                }
            }
        })
    }
}
