use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, StatusCode, header},
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
    core::{error::CoreError, services::ctx::CtxService},
    web::error::ErrorBody,
};
use crate::{core::services::token::TokenService, store::dbx::PgDbx};

#[derive(Clone)]
pub struct CtxLayer {
    ctx_svc: Arc<CtxService<PgDbx, RedisChx>>,
}

impl CtxLayer {
    pub fn new(app_state: &App) -> Self {
        let ctx_svc = Arc::new(CtxService::new(
            app_state.sm.clone(),
            app_state.cm.clone(),
            app_state.svc_reg.clone(),
            app_state.ctx_factory.clone(),
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
            match ctx_svc.resolve_ctx(req.headers()).await {
                Ok(mut ctx) => {
                    req.extensions_mut().insert(ctx);
                    inner.call(req).await
                }
                Err(e) => {
                    error!("{e}");

                    let message = match e {
                        CoreError::Auth(msg) => msg,
                        _ => "unauthorized".into(),
                    };

                    let body = ErrorBody {
                        success: false,
                        status: StatusCode::UNAUTHORIZED.as_u16(),
                        message,
                    };

                    Ok((StatusCode::UNAUTHORIZED, axum::Json(body)).into_response())
                }
            }
        })
    }
}
