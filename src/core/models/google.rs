use std::collections::HashMap;

use crate::{
    config::Config,
    core::error::{CoreError, CoreResult},
};

#[derive(Debug, Default)]
pub struct GoogleOAuthState {
    pub csrf_token: String,
    pub redirect_url: String,
    pub dash_url: String,
    pub project_name: String,
}

impl TryFrom<String> for GoogleOAuthState {
    type Error = CoreError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let v: Self = value.as_str().try_into()?;
        Ok(v)
    }
}

impl TryFrom<&str> for GoogleOAuthState {
    type Error = CoreError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let params: HashMap<String, String> = url::form_urlencoded::parse(value.as_bytes())
            .into_owned()
            .collect();

        let csrf_token = params
            .get("csrf_token")
            .ok_or(CoreError::ParseError(
                "unable to get csrf_token for GoogleOAuthState".to_string(),
            ))?
            .into();
        let redirect_url = params
            .get("redirect_url")
            .ok_or(CoreError::ParseError(
                "unable to get redirect_url for GoogleOAuthState".to_string(),
            ))?
            .into();

        let dash_url = params
            .get("dash_url")
            .ok_or(CoreError::ParseError(
                "unable to get dash_url for GoogleOAuthState".to_string(),
            ))?
            .into();
        let project_name = params.get("project_name").map_or("OxideAuth", |v| v).into();

        Ok(Self {
            csrf_token,
            redirect_url,
            dash_url,
            project_name,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_state_with_valid_data() {
        let state = "redirect_url=http://example.com&csrf_token=abc123&dash_url=http://dashboard.com&project_name=TestProject".to_string();

        let google_oauth_state = GoogleOAuthState::try_from(state).expect("unable to parse state");

        assert_eq!(google_oauth_state.redirect_url, "http://example.com");
        assert_eq!(google_oauth_state.csrf_token, "abc123");
        assert_eq!(google_oauth_state.dash_url, "http://dashboard.com");
        assert_eq!(google_oauth_state.project_name, "TestProject");
    }

    #[test]
    fn test_from_state_with_empty_state() {
        let google_oauth_state = GoogleOAuthState::default();

        assert_eq!(google_oauth_state.redirect_url, "");
        assert_eq!(google_oauth_state.csrf_token, "");
        assert_eq!(google_oauth_state.dash_url, "");
        assert_eq!(google_oauth_state.project_name, "");
    }
}
