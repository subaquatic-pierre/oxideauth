use log::info;
use std::{collections::HashSet, env};

use dotenv::dotenv;

use crate::{
    config::Config,
    core::error::{CoreError, CoreResult},
};

use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::Value;
use std::error::Error;

use crate::core::models::account::Account;

use super::crypt::hash_password;

#[derive(Deserialize, Debug)]
pub struct OAuthResponse {
    pub access_token: String,
    pub id_token: String,
}

#[derive(Deserialize, Debug)]
pub struct OAuthErrorResponse {
    pub error: String,
    pub error_description: String,
}

#[derive(Deserialize, Debug)]
pub struct GoogleUserResult {
    pub id: String,
    pub email: String,
    pub verified_email: bool,
    pub name: String,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub picture: Option<String>,
    pub locale: Option<String>,
}

pub async fn request_google_token(
    authorization_code: &str,
    config: &Config,
) -> CoreResult<OAuthResponse> {
    let redirect_url = config.google_oauth_redirect_url.to_owned();
    let client_secret = config.google_oauth_client_secret.to_owned();
    let client_id = config.google_oauth_client_id.to_owned();

    let root_url = "https://oauth2.googleapis.com/token";
    let client = Client::new();

    let params = [
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_url.as_str()),
        ("client_id", client_id.as_str()),
        ("code", authorization_code),
        ("client_secret", client_secret.as_str()),
    ];

    let res = client.post(root_url).form(&params).send().await?;

    info!("response: {res:?}");

    match res.status().is_success() {
        true => match res.json::<OAuthResponse>().await {
            Ok(r) => Ok(r),
            Err(e) => Err(CoreError::ApiError(e.to_string())),
        },
        false => {
            if let Ok(json) = res.json::<OAuthErrorResponse>().await {
                Err(CoreError::ApiError(
                    "An error occurred while trying to retrieve access token.".to_string(),
                ))
            } else {
                Err(CoreError::ApiError(
                    "An error occurred while trying to retrieve access token.".to_string(),
                ))
            }
        }
    }
}

pub async fn get_google_user(access_token: &str, id_token: &str) -> CoreResult<GoogleUserResult> {
    let client = Client::new();
    let mut url = Url::parse("https://www.googleapis.com/oauth2/v1/userinfo").unwrap();
    url.query_pairs_mut().append_pair("alt", "json");
    url.query_pairs_mut()
        .append_pair("access_token", access_token);

    let response = client.get(url).bearer_auth(id_token).send().await?;

    if response.status().is_success() {
        let user_info = response.json::<GoogleUserResult>().await?;
        Ok(user_info)
    } else {
        let message = "An error occurred while trying to retrieve user information.";
        Err(CoreError::ApiError(message.to_string()))
    }
}

pub struct RegisterRedirectParams {
    pub project_name: String,
    pub confirm_url: String,
}

impl RegisterRedirectParams {
    pub fn from_req(body: Value, config: &Config, token: &str) -> Self {
        // let redirect_url = match (&body.redirect_host, &body.confirm_email_redirect_endpoint) {
        //     (Some(host), Some(endpoint)) => format!("{}{}", host.clone(), endpoint.clone()),
        //     _ => format!("{}/auth/sign-in", config.client_origin),
        // };

        // let dashboard_url = match (&body.redirect_host, &body.dashboard_endpoint) {
        //     (Some(host), Some(endpoint)) => format!("{}{}", host.clone(), endpoint.clone()),
        //     _ => format!("{}/dashboard", config.client_origin),
        // };

        // let project_name = match &body.project_name {
        //     Some(name) => name.to_string(),
        //     None => "OxideAuth".to_string(),
        // };

        // let server_host = format!("{}:{}", config.host.clone(), config.port.clone());
        // let confirm_url = format!(
        //     "{}/auth/confirm-account?token={}&redirectUrl={}&dashboardUrl={}&projectName={}",
        //     server_host, token, redirect_url, dashboard_url, project_name
        // );

        // Self {
        //     project_name,
        //     confirm_url,
        // }
        todo!()
    }
}
