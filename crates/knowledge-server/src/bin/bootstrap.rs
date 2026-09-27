use std::env;

use knowledge_crypto::MasterKey;
use knowledge_server::{
    bootstrap::{BootstrapInput, BootstrapService},
    config::{DatabaseConfig, load_required_secret},
};
use knowledge_store::{DatabaseSettings, Store};

#[derive(Debug)]
struct Arguments {
    email: String,
    workspace_name: String,
    display_name: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = parse_arguments(env::args().skip(1))?;
    let database = DatabaseConfig::from_env()?;
    let encoded_master_key = load_required_secret("KNOWLEDGE_MASTER_KEY")?;
    let master_key = MasterKey::from_base64(&encoded_master_key)?;
    let password = if env::var_os("KNOWLEDGE_BOOTSTRAP_PASSWORD_FILE").is_some() {
        load_required_secret("KNOWLEDGE_BOOTSTRAP_PASSWORD")?.to_string()
    } else {
        rpassword::prompt_password("Initial admin password: ")?
    };

    let store = Store::connect(&DatabaseSettings::new(database.as_url(), 5)).await?;
    let result = BootstrapService::run(
        &store,
        &master_key,
        BootstrapInput {
            workspace_name: arguments.workspace_name,
            email: arguments.email,
            display_name: arguments.display_name,
            password,
        },
    )
    .await?;

    println!(
        "Bootstrap completed: workspace={}, admin={}",
        result.workspace_id, result.user_id
    );
    Ok(())
}

fn parse_arguments(arguments: impl IntoIterator<Item = String>) -> Result<Arguments, String> {
    let mut email = None;
    let mut workspace_name = "Knowledge Platform".to_owned();
    let mut display_name = None;
    let mut arguments = arguments.into_iter();

    while let Some(argument) = arguments.next() {
        let option_name = argument
            .split_once('=')
            .map_or(argument.as_str(), |(name, _)| name);
        if option_name == "--password" {
            return Err("password arguments are not accepted; use the secure prompt".to_owned());
        }
        if !matches!(
            argument.as_str(),
            "--email" | "--workspace-name" | "--display-name"
        ) {
            return Err(format!("unknown argument: {option_name}"));
        }
        let value = arguments
            .next()
            .ok_or_else(|| format!("{argument} requires a value"))?;
        match argument.as_str() {
            "--email" => email = Some(value),
            "--workspace-name" => workspace_name = value,
            "--display-name" => display_name = Some(value),
            _ => unreachable!("known arguments were checked above"),
        }
    }

    let email = email.ok_or_else(|| "--email is required".to_owned())?;
    let display_name = display_name.unwrap_or_else(|| email.clone());
    Ok(Arguments {
        email,
        workspace_name,
        display_name,
    })
}
