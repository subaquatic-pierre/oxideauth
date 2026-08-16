use axum::{
    Router,
    error_handling::HandleErrorLayer,
    extract::Extension,
    middleware::{from_fn, map_response},
};
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use crate::{
    app::App,
    web::{
        handlers::{
            account::AccountRouter, auth::AuthRouter, client::ClientRouter,
            credential::CredentialRouter, membership::MembershipRouter,
            permission::PermissionRouter, policy::PolicyRouter, profile::ProfileRouter,
            project::ProjectRouter, role::RoleRouter, root::RootRouter, workspace::WorkspaceRouter,
        },
        middlewares::{
            cors::build_cors,
            ctx::{CtxLayer, CtxMw},
            empty_body::empty_body_fallback,
            fallback::FallbackMw,
            request::RequestMw,
            response::ResponseMw,
        },
    },
};

pub struct AppRouter;

impl AppRouter {
    pub fn routes_with_state(state: App) -> Router {
        let cors = build_cors();
        let ctx = CtxLayer::new(&state);

        let global_error_layer = ServiceBuilder::new()
            .layer(HandleErrorLayer::new(FallbackMw::global_error_handler))
            .timeout(Duration::from_secs(30));

        // ── Public routes (no auth required) ──
        let public = Router::new()
            .nest("/", RootRouter::routes())
            .nest("/auth", AuthRouter::public_routes());

        // ── Protected routes (auth required via CtxLayer) ──
        let protected = Router::new()
            .nest("/accounts", AccountRouter::routes())
            .nest("/workspace", WorkspaceRouter::routes())
            .nest("/projects", ProjectRouter::routes())
            .nest("/profiles", ProfileRouter::routes())
            .nest("/clients", ClientRouter::routes())
            .nest("/roles", RoleRouter::routes())
            .nest("/permissions", PermissionRouter::routes())
            .nest("/policies", PolicyRouter::routes())
            .nest("/memberships", MembershipRouter::routes())
            .nest("/credentials", CredentialRouter::routes())
            .nest("/auth", AuthRouter::protected_routes())
            .layer(from_fn(empty_body_fallback))
            .layer(ctx);

        Router::new()
            .merge(public)
            .merge(protected)
            // Global middleware (applied to ALL routes)
            .layer(global_error_layer)
            .layer(map_response(ResponseMw::response_map_handler))
            .layer(cors)
            .layer(Extension(state))
            .layer(from_fn(RequestMw::request_map_handler))
            .layer(TraceLayer::new_for_http())
            .fallback(FallbackMw::fallback_handler)
    }
}
