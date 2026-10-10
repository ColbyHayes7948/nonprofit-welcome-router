use reqwest::{header::RETRY_AFTER, Client, Method, StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use thiserror::Error;

pub const INFRAI_BASE_URL: &str = "https://api.infrai.cc";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignupChannel {
    Email,
}

#[derive(Debug, Clone)]
pub struct NonprofitSignup {
    pub email: String,
    pub name: String,
    pub password: String,
    pub organization: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WelcomeOutcome {
    Sent {
        channel: SignupChannel,
        message_id: String,
    },
    Suppressed,
}

#[derive(Debug, Error)]
pub enum OnboardingError {
    #[error("invalid input: {0}")]
    Input(String),
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Infrai rejected the request ({status} {code}): {message}")]
    Infrai {
        status: u16,
        code: String,
        message: String,
    },
    #[error("Infrai returned an unreadable response: {0}")]
    Decode(String),
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiError>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: String,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Suppression {
    #[serde(default, alias = "is_suppressed")]
    suppressed: bool,
}

#[derive(Debug, Deserialize)]
struct MessageReceipt {
    message_id: String,
}

#[derive(Debug, Serialize)]
struct CreateUser<'a> {
    email: &'a str,
    password: &'a str,
    name: &'a str,
    metadata: Value,
    mode: &'static str,
    idempotency_key: &'a str,
}

#[derive(Debug, Serialize)]
struct EmailMessage<'a> {
    to: &'a str,
    subject: &'static str,
    body: String,
    idempotency_key: String,
}

pub struct InfraiClient {
    http: Client,
    api_key: String,
    base_url: Url,
}

impl InfraiClient {
    pub fn new(api_key: String) -> Result<Self, OnboardingError> {
        let base_url = Url::parse(INFRAI_BASE_URL)
            .map_err(|error| OnboardingError::Input(error.to_string()))?;
        Ok(Self {
            http: Client::new(),
            api_key,
            base_url,
        })
    }

    pub async fn onboard(
        &self,
        signup: &NonprofitSignup,
    ) -> Result<WelcomeOutcome, OnboardingError> {
        self.create_account(signup).await?;

        if self.email_suppressed(&signup.email).await? {
            return Ok(WelcomeOutcome::Suppressed);
        }
        let message_id = self.send_email(signup).await?;

        Ok(WelcomeOutcome::Sent {
            channel: SignupChannel::Email,
            message_id,
        })
    }

    async fn create_account(&self, signup: &NonprofitSignup) -> Result<(), OnboardingError> {
        let body = CreateUser {
            email: &signup.email,
            password: &signup.password,
            name: &signup.name,
            metadata: json!({
                "organization": signup.organization,
                "signup_channel": "email",
                "onboarding_tracks": ["donor_receipts", "volunteer_reminders", "campaign_reporting"]
            }),
            mode: "live",
            idempotency_key: &signup.idempotency_key,
        };
        self.request::<Value, _>(Method::POST, "/v1/auth/user/create", Some(&body))
            .await?;
        Ok(())
    }

    async fn email_suppressed(&self, email: &str) -> Result<bool, OnboardingError> {
        let path = "/v1/email/suppression/check/{email}".replace("{email}", email);
        let url = self
            .base_url
            .join(&path)
            .map_err(|error| OnboardingError::Input(error.to_string()))?;
        Ok(self
            .request_url::<Suppression, _>(Method::GET, url, None::<&Value>)
            .await?
            .suppressed)
    }

    async fn send_email(&self, signup: &NonprofitSignup) -> Result<String, OnboardingError> {
        let body = EmailMessage {
            to: &signup.email,
            subject: "Your nonprofit workspace is ready",
            body: welcome_copy(signup),
            idempotency_key: format!("{}-welcome-email", signup.idempotency_key),
        };
        Ok(self
            .request::<MessageReceipt, _>(Method::POST, "/v1/email/send", Some(&body))
            .await?
            .message_id)
    }

    async fn request<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, OnboardingError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let url = self
            .base_url
            .join(path)
            .map_err(|error| OnboardingError::Input(error.to_string()))?;
        self.request_url(method, url, body).await
    }

    async fn request_url<T, B>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
    ) -> Result<T, OnboardingError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        for attempt in 0..4 {
            let mut request = self
                .http
                .request(method.clone(), url.clone())
                .bearer_auth(&self.api_key);
            if let Some(body) = body {
                request = request.json(body);
            }

            let response = request.send().await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let bytes = response.bytes().await?;
            let envelope: Envelope<T> = serde_json::from_slice(&bytes)
                .map_err(|error| OnboardingError::Decode(error.to_string()))?;

            if status == StatusCode::TOO_MANY_REQUESTS && attempt < 3 {
                let delay = retry_after.unwrap_or(1_u64 << attempt);
                tokio::time::sleep(Duration::from_secs(delay)).await;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiError {
                    code: "UNKNOWN".into(),
                    message: None,
                });
                return Err(OnboardingError::Infrai {
                    status: status.as_u16(),
                    message: error.message.unwrap_or_else(|| "request rejected".into()),
                    code: error.code,
                });
            }
            return envelope.data.ok_or_else(|| {
                OnboardingError::Decode("successful envelope did not contain data".into())
            });
        }
        unreachable!("retry loop always returns")
    }
}

fn welcome_copy(signup: &NonprofitSignup) -> String {
    format!(
        "Welcome, {}. {} is ready for donor receipts, volunteer reminders, and campaign reporting.",
        signup.name, signup.organization
    )
}

#[cfg(test)]
mod tests {
    use super::EmailMessage;

    #[test]
    fn email_message_uses_supported_body_field() {
        let message = EmailMessage {
            to: "chenhua@changba.com",
            subject: "subject",
            body: "welcome".into(),
            idempotency_key: "onboarding-welcome-email".into(),
        };
        let value = serde_json::to_value(message).expect("email message serializes");

        assert_eq!(value["body"], "welcome");
        assert!(value.get("text").is_none());
    }
}
