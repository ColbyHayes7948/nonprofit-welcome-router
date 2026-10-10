use nonprofit_welcome_router::{InfraiClient, NonprofitSignup, OnboardingError, WelcomeOutcome};
use std::env;

fn required(name: &str) -> Result<String, OnboardingError> {
    env::var(name).map_err(|_| OnboardingError::Input(format!("{name} is required")))
}

#[tokio::main]
async fn main() -> Result<(), OnboardingError> {
    let signup = NonprofitSignup {
        email: required("NONPROFIT_EMAIL")?,
        name: required("NONPROFIT_CONTACT_NAME")?,
        password: required("NONPROFIT_TEMP_PASSWORD")?,
        organization: required("NONPROFIT_NAME")?,
        idempotency_key: required("ONBOARDING_ID")?,
    };
    let client = InfraiClient::new(required("INFRAI_API_KEY")?)?;

    match client.onboard(&signup).await? {
        WelcomeOutcome::Sent {
            channel,
            message_id,
        } => println!("account created; welcome sent by {channel:?}; message_id={message_id}"),
        WelcomeOutcome::Suppressed => {
            println!("account created; welcome skipped because the email is suppressed")
        }
    }
    Ok(())
}
