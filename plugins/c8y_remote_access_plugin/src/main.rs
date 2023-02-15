use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;

use futures::future::try_select;
use futures::future::Either;
use miette::Context;
use miette::IntoDiagnostic;
use tedge_config::C8yUrlSetting;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;
use tedge_config::TEdgeConfigRepository;
use tedge_utils::file::create_file_with_user_group;
use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use url::Url;

use crate::auth::Jwt;
use crate::input::Command;
use crate::input::RemoteAccessConnect;
use crate::proxy::WebsocketSocketProxy;

mod auth;
mod csv;
mod input;
mod proxy;

#[tokio::main]
async fn main() -> miette::Result<()> {
    let config_dir = TEdgeConfigLocation::default();
    let tedge_config = TEdgeConfigRepository::new(config_dir.clone())
        .load()
        .into_diagnostic()
        .context("Reading tedge config")?;

    let command = input::parse_arguments()?;

    match command {
        Command::Init => declare_supported_operation(config_dir.tedge_config_root_path()),
        Command::Cleanup => remove_supported_operation(config_dir.tedge_config_root_path()),
        Command::Connect(command) => proxy(command, tedge_config).await,
        Command::SpawnChild(command) => spawn_child(command).await,
    }
}

fn declare_supported_operation(config_dir: &Path) -> miette::Result<()> {
    create_file_with_user_group(
        supported_operation_path(config_dir),
        "tedge",
        "tedge",
        0o644,
        Some(
            r#"[exec]
command = "/usr/bin/c8y-remote-access-plugin"
topic = "c8y/s/ds"
on_message = "530"
"#,
        ),
    )
    .into_diagnostic()
    .context("Declaring supported operations")
}

fn remove_supported_operation(config_dir: &Path) -> miette::Result<()> {
    let path = supported_operation_path(config_dir);
    std::fs::remove_file(&path)
        .into_diagnostic()
        .with_context(|| format!("Removing supported operation at {}", path.display()))
}

static SUCCESS_MESSAGE: &str = "CONNECTED";

#[derive(miette::Diagnostic, Debug, thiserror::Error)]
#[error("Failed while {1}")]
#[diagnostic(help(
    "This should never happen. It's very likely a bug in the c8y remote access plugin."
))]
struct Unreachable<E: std::error::Error + 'static>(#[source] E, &'static str);

async fn spawn_child(command: String) -> miette::Result<()> {
    let mut command = tokio::process::Command::new("/usr/bin/c8y-remote-access-plugin")
        .arg("--child")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .into_diagnostic()
        .context("Failed to spawn child process")?;

    let mut stdout = BufReader::new(command.stdout.take().unwrap());
    let mut stderr = BufReader::new(command.stderr.take().unwrap());

    let copy_error_messages = tokio::task::spawn(async move {
        let mut line = String::new();
        while let Ok(amount) = stderr.read_line(&mut line).await {
            if amount == 0 {
                break;
            }
            eprint!("{line}");
            line.clear();
        }
    });

    let wait_for_connection = tokio::task::spawn(async move {
        let mut line = String::new();
        while stdout.read_line(&mut line).await.is_ok() {
            // Copy the output to the parent process stdout to ensure anything we might
            // print doesn't get lost before we connect
            print!("{line}");
            if line.trim() == SUCCESS_MESSAGE {
                break;
            }
            line.clear();
        }
    });

    let wait_for_failure = tokio::task::spawn(async move { command.wait().await });

    match try_select(wait_for_connection, wait_for_failure).await {
        Ok(Either::Left(_)) => Ok(()),
        Ok(Either::Right((Ok(code), _))) => {
            copy_error_messages
                .await
                .map_err(|e| Unreachable(e, "copying stderr from child process"))?;
            let code = code.code().unwrap_or(1);
            std::process::exit(code)
        }
        Ok(Either::Right((Err(e), _))) => Err(e)
            .into_diagnostic()
            .context("Failed to retrieve exit code from child process"),
        Err(Either::Left((e, _))) => {
            Err(Unreachable(e, "waiting for the connection to be established").into())
        }
        Err(Either::Right((e, _))) => Err(Unreachable(e, "waiting for the process to exit").into()),
    }
}

async fn proxy(command: RemoteAccessConnect, config: TEdgeConfig) -> miette::Result<()> {
    let host = config.query(C8yUrlSetting).into_diagnostic()?;
    let url = build_proxy_url(host.as_str(), command.key())?;
    let jwt = Jwt::retrieve(&config)
        .await
        .context("Failed when requesting JWT from Cumulocity")?;

    let proxy = WebsocketSocketProxy::connect(&url, command.target_address(), jwt).await?;

    proxy.run().await;
    Ok(())
}

fn supported_operation_path(config_dir: &Path) -> PathBuf {
    let mut path = config_dir.to_owned();
    path.push("operations/c8y/c8y_RemoteAccessConnect");
    path
}

fn build_proxy_url(cumulocity_host: &str, key: &str) -> miette::Result<Url> {
    format!("wss://{cumulocity_host}/service/remoteaccess/device/{key}")
        .parse()
        .into_diagnostic()
        .context("Creating websocket URL")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_supported_operation_path() {
        assert_eq!(
            supported_operation_path("/etc/tedge".as_ref()),
            PathBuf::from("/etc/tedge/operations/c8y/c8y_RemoteAccessConnect")
        );
    }
}