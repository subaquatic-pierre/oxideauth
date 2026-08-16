use axum::{Json, Router, extract::Extension, routing::post};

use crate::{
    app::App,
    core::{
        ctx::CoreCtx,
        models::profile::{ProfileDeleteParams, ProfileDescribeParams, ProfileListParams, ProfileUpdateParams},
        traits::{
            params::IntoParams,
            service::{
                CoreModelDeleteService, CoreModelDescribeService, CoreModelListService,
                CoreModelUpdateService,
            },
        },
    },
    web::{
        dtos::profile::{
            ProfileDeleteReq, ProfileDeleteRes, ProfileDescribeReq, ProfileDescribeRes,
            ProfileListReq, ProfileListRes, ProfileUpdateReq,
        },
        error::{JsonReqResult, JsonResResult},
        response::WebResponse,
    },
};

// --- Describe Profile ---
#[axum::debug_handler]
pub async fn describe_profile(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ProfileDescribeReq>,
) -> JsonResResult<WebResponse<ProfileDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.profile.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: ProfileDescribeParams = body.into_params(ws_id)?;

    let profile = svc.describe(&mut ctx, params).await?;

    let profile_res: ProfileDescribeRes = profile.into();

    WebResponse::json(profile_res)
}

// --- List Profiles ---
#[axum::debug_handler]
pub async fn list_profiles(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ProfileListReq>,
) -> JsonResResult<WebResponse<ProfileListRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.profile.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: ProfileListParams = body.into_params(ws_id)?;
    let res = svc.list(&mut ctx, params).await?;

    let profiles: Vec<ProfileDescribeRes> =
        res.data.into_iter().map(ProfileDescribeRes::from).collect();

    let res = ProfileListRes {
        profiles,
        metadata: res.metadata,
    };

    WebResponse::json(res)
}

// --- Update Profile ---
#[axum::debug_handler]
pub async fn update_profile(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ProfileUpdateReq>,
) -> JsonResResult<WebResponse<ProfileDescribeRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.profile.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: ProfileUpdateParams = body.into_params(ws_id)?;

    let profile = svc.update(&mut ctx, params).await?;

    let profile_res: ProfileDescribeRes = profile.into();

    WebResponse::json(profile_res)
}

// --- Delete Profile ---
#[axum::debug_handler]
pub async fn delete_profile(
    mut ctx: Extension<CoreCtx>,
    app: Extension<App>,
    body: JsonReqResult<ProfileDeleteReq>,
) -> JsonResResult<WebResponse<ProfileDeleteRes>> {
    let Json(body) = body?;
    let svc = app.svc_reg.profile.clone();

    let ws_id = ctx.scoped_ws_id();
    let params: ProfileDeleteParams = body.into_params(ws_id)?;

    let profile = svc.delete(&mut ctx, params).await?;

    let res = ProfileDeleteRes { id: profile.id };

    WebResponse::json(res)
}

// --- Profile Router ---
pub struct ProfileRouter;

impl ProfileRouter {
    pub fn routes() -> Router {
        Router::new()
            .route("/describe", post(describe_profile))
            .route("/list", post(list_profiles))
            .route("/update", post(update_profile))
            .route("/delete", post(delete_profile))
    }
}
