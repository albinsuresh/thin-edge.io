#![forbid(unsafe_code)]
#![deny(clippy::mem_forget)]

use anyhow::Context;
use cap::Cap;
use clap::error::ErrorFormatter;
use clap::error::RichFormatter;
use clap::CommandFactory;
use clap::FromArgMatches;
use clap::Parser;
use std::alloc;
use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::time::Duration;
use tedge::command::BuildCommand;
use tedge::log::MaybeFancy;
use tedge::Component;
use tedge::ComponentOpt;
use tedge::TEdgeCli;
use tedge::TEdgeOpt;
use tedge::TEdgeOptMulticall;
use tedge_apt_plugin::AptCli;
use tedge_config::cli::CommonArgs;
use tedge_config::log_init;
use tedge_config::unconfigured_logger;
use tracing::log;

#[global_allocator]
static ALLOCATOR: Cap<alloc::System> = Cap::new(alloc::System, usize::MAX);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opt = tracing::subscriber::with_default(unconfigured_logger(), || {
        clap_complete::CompleteEnv::with_factory(TEdgeCli::command).complete();

        parse_multicall(&executable_name(), std::env::args_os())
    });
    match opt {
        TEdgeOptMulticall::Component(Component::TedgeMapper(opt)) => {
            let tedge_config = tedge_config::TEdgeConfig::load(&opt.common.config_dir).await?;
            log_memory_usage(tedge_config.run.log_memory_interval.duration());
            tedge_mapper::run(opt, tedge_config).await
        }
        TEdgeOptMulticall::Component(Component::TedgeAgent(opt)) => {
            let tedge_config = tedge_config::TEdgeConfig::load(&opt.common.config_dir).await?;
            log_memory_usage(tedge_config.run.log_memory_interval.duration());
            tedge_agent::run(opt, tedge_config).await
        }
        TEdgeOptMulticall::Component(Component::C8yFirmwarePlugin(fp_opt)) => {
            c8y_firmware_plugin::run(fp_opt).await
        }
        TEdgeOptMulticall::Component(Component::C8yRemoteAccessPlugin(opt)) => {
            let _ = c8y_remote_access_plugin::run(opt).await;
            Ok(())
        }
        TEdgeOptMulticall::Component(Component::TedgeWatchdog(opt)) => {
            tedge_watchdog::run(opt).await
        }
        TEdgeOptMulticall::Component(Component::TedgeWrite(opt)) => {
            tokio::task::spawn_blocking(move || tedge_write::bin::run(opt))
                .await
                .context("failed to run tedge write process")?
        }
        TEdgeOptMulticall::Component(Component::TedgeAptPlugin(opt)) => {
            let config = tedge_apt_plugin::get_config(opt.common.config_dir.as_std_path()).await;
            tokio::task::spawn_blocking(move || tedge_apt_plugin::run_and_exit(opt, config))
                .await
                .context("failed to run tedge apt plugin")?
        }
        TEdgeOptMulticall::Tedge(TEdgeCli { cmd, common }) => {
            log_init(
                "tedge",
                &common.log_args.with_default_level(tracing::Level::WARN),
                &common.config_dir,
            )?;

            let tedge_config = tedge_config::TEdgeConfig::load(&common.config_dir).await?;

            let cmd = cmd
                .build_command(&tedge_config)
                .with_context(|| "missing configuration parameter")?;

            if !std::io::stdout().is_terminal() {
                yansi::disable();
            }

            match cmd.execute(tedge_config).await {
                Ok(()) => Ok(()),
                // If the command already prints its own nicely formatted errors
                // don't also print the error by returning it
                Err(MaybeFancy::Fancy(_)) => std::process::exit(1),
                Err(MaybeFancy::Unfancy(err)) => {
                    Err(err.context(format!("failed to {}", cmd.description())))
                }
            }
        }
    }
}

fn log_memory_usage(log_memory_interval: Duration) {
    if log_memory_interval.is_zero() {
        return;
    }
    tokio::spawn(async move {
        loop {
            log::info!(
                "Allocated memory: {} Bytes {log_memory_interval:?}",
                ALLOCATOR.allocated()
            );
            tokio::time::sleep(log_memory_interval).await;
        }
    });
}

fn executable_name() -> Option<String> {
    Some(
        PathBuf::from(std::env::args_os().next()?)
            .file_stem()?
            .to_str()?
            .to_owned(),
    )
}

fn parse_multicall<Arg, Args>(executable_name: &Option<String>, args: Args) -> TEdgeOptMulticall
where
    Args: IntoIterator<Item = Arg>,
    Arg: Into<OsString> + Clone,
{
    if matches!(executable_name.as_deref(), Some("apt" | "tedge-apt-plugin")) {
        // the apt plugin must be treated apart
        // as we want to exit 1 and not 2 when the command line cannot be parsed
        match AptCli::try_parse() {
            Ok(apt) => return TEdgeOptMulticall::Component(Component::TedgeAptPlugin(apt)),
            Err(e) => {
                eprintln!("{}", RichFormatter::format_error(&e));
                std::process::exit(1);
            }
        }
    }

    let cmd = TEdgeOptMulticall::command();

    let is_known_subcommand = executable_name
        .as_deref()
        .is_some_and(|name| cmd.find_subcommand(name).is_some());
    let cmd = cmd.multicall(is_known_subcommand);

    let cmd2 = cmd.clone();
    match TEdgeOptMulticall::from_arg_matches(&cmd.get_matches_from(args)) {
        Ok(TEdgeOptMulticall::Tedge(TEdgeCli { cmd, common })) => {
            redirect_if_multicall(cmd, common)
        }
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}", RichFormatter::format_error(&e.with_cmd(&cmd2)));
            std::process::exit(1);
        }
    }
}

// Transform `tedge mapper|agent|write` commands into multicall commands
//
// This method has to be kept in sync with TEdgeOpt::build_command
fn redirect_if_multicall(cmd: TEdgeOpt, common: CommonArgs) -> TEdgeOptMulticall {
    match cmd {
        TEdgeOpt::Run(ComponentOpt { component }) => TEdgeOptMulticall::Component(component),
        cmd => TEdgeOptMulticall::Tedge(TEdgeCli { cmd, common }),
    }
}

#[cfg(test)]
mod tests {
    use crate::parse_multicall;
    use crate::Component;
    use crate::TEdgeOptMulticall;
    use test_case::test_case;

    #[test]
    fn launching_a_mapper() {
        let exec = Some("tedge-mapper".to_string());
        let cmd = parse_multicall(&exec, ["tedge-mapper", "c8y"]);
        assert!(matches!(
            cmd,
            TEdgeOptMulticall::Component(Component::TedgeMapper(_))
        ))
    }

    #[test]
    fn using_tedge_to_launch_a_mapper() {
        let exec = Some("tedge".to_string());
        let cmd = parse_multicall(&exec, ["tedge", "run", "tedge-mapper", "c8y"]);
        assert!(matches!(
            cmd,
            TEdgeOptMulticall::Component(Component::TedgeMapper(_))
        ))
    }

    #[test_case("tedge-mapper c8y --config-dir /some/dir")]
    #[test_case("tedge-mapper --config-dir /some/dir c8y")]
    #[test_case("tedge run tedge-mapper c8y --config-dir /some/dir")]
    #[test_case("tedge run tedge-mapper --config-dir /some/dir c8y")]
    #[test_case("tedge --config-dir /some/dir run tedge-mapper c8y")]
    // clap fails to raise an error here and takes the inner value for all global args
    #[test_case("tedge --config-dir /oops run tedge-mapper c8y --config-dir /some/dir")]
    fn setting_config_dir(cmd_line: &'static str) {
        let args: Vec<&str> = cmd_line.split(' ').collect();
        let exec = Some(args.get(0).unwrap().to_string());
        let cmd = parse_multicall(&exec, args);
        match cmd {
            TEdgeOptMulticall::Component(Component::TedgeMapper(mapper)) => {
                assert_eq!(mapper.common.config_dir, "/some/dir")
            }
            _ => panic!(),
        }
    }
}
