use std::sync::Arc;

use playferrous_presentation::Presentation;
use playferrous_presentation_ssh::PresentationSsh;
use tracing_subscriber::{fmt::format::FmtSpan, EnvFilter};

use crate::{database::Database, user_management::UserManagementImpl};

#[macro_use]
mod database;
mod user_management;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenv::dotenv();
    tracing_subscriber::fmt()
        .with_span_events(FmtSpan::ACTIVE)
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let database = Database::connect().await?;
    let user_management = Arc::new(UserManagementImpl::new(database));
    let _ssh_presentation = PresentationSsh::new(Default::default(), user_management).await?;
    println!("Hello, world!");
    Ok(())
}
