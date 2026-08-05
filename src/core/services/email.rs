use lettre::{SmtpTransport, Transport};
use log::{error, info};

use aws_sdk_ses::config::Credentials;
use aws_sdk_ses::config::{Builder, Region};
use aws_sdk_ses::types::{Body, Content, Destination, Message};
use aws_sdk_ses::Client as SesClient;
use tera::{Context, Tera};

use crate::config::Config;

use crate::core::error::{CoreError, CoreResult};
use crate::core::services::storage::StorageService;

#[derive(Debug)]
pub struct EmailResult {
    pub message: String,
}

pub struct EmailService<T: StorageService> {
    ses_client: SesClient,
    from_email: String,
    storage: T,
    dry_mode: bool,
}

impl<T: StorageService> EmailService<T> {
    pub fn new(config: &Config, storage: T) -> Self {
        let region = Region::new(config.aws_region.clone());

        let credentials = Credentials::new(
            config.aws_ses_access_key.clone(),
            config.aws_ses_secret_key.clone(),
            None,
            None,
            "custom",
        );

        let ses_config = Builder::new()
            .region(region)
            .credentials_provider(credentials)
            .build();

        let client = SesClient::from_conf(ses_config);
        let from_email = format!("🛡️ OxideAuth <{}>", config.aws_ses_from);
        Self {
            ses_client: client,
            from_email,
            storage: storage,
            dry_mode: config.email_dry_mode,
        }
    }

    pub async fn send_email(
        &self,
        to_email: &str,
        subject: &str,
        template_name: &str,
        context: Context,
    ) -> CoreResult<EmailResult> {
        if self.dry_mode {
            info!("Email Dry Run Mode: to: {to_email}, subject: {subject}, template_name: {template_name}, context: {context:?}");
            Ok(EmailResult {
                message: "Email sent successfully!".to_string(),
            })
        } else {
            let content = match self.storage.get_file(template_name).await {
                Ok(s) => s.to_string(),
                Err(e) => {
                    error!("Error reading file, {}, {e}", template_name);
                    "".to_string()
                }
            };

            let mut tera = Tera::default();
            let _ = tera.add_raw_template(template_name, &content);

            let body = match tera.render(template_name, &context) {
                Ok(body) => body,
                Err(e) => return Err(CoreError::ApiError(e.to_string())),
            };

            let destination = Destination::builder().to_addresses(to_email).build();

            let subject = Content::builder().data(subject).build().unwrap();
            let body = Body::builder()
                .html(Content::builder().data(body).build().unwrap())
                .build();

            let message = Message::builder().subject(subject).body(body).build();

            let send_email_request = self
                .ses_client
                .send_email()
                .source(&self.from_email)
                .destination(destination)
                .message(message)
                .send()
                .await;

            // Send the email
            match send_email_request {
                Ok(res) => {
                    info!("Send email Result: {res:?}");
                    Ok(EmailResult {
                        message: "Email sent successfully!".to_string(),
                    })
                }
                Err(e) => {
                    error!("Could not send email: {:?}", e);
                    Err(CoreError::ApiError(e.to_string()))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core::services::storage::MockStorageService;

    use super::*;
    use aws_sdk_ses::types::Body;
    use aws_sdk_ses::types::Content;
    use aws_sdk_ses::types::Destination;
    use aws_sdk_ses::types::Message;
    use mockall::{mock, predicate::*};
    use tera::Context;

    #[tokio::test]
    async fn test_send_email_success() {
        let mut mock_storage = MockStorageService {};

        let config = Config::mock_config();
        // Mocking SES client can be more complex, might need to use `mockito` or similar library if required

        let service = EmailService::new(&config, mock_storage);

        let context = Context::new();
        let result = service
            .send_email("test@example.com", "Test Subject", "test_template", context)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().message, "Email sent successfully!");
    }
}
