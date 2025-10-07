use crate::plugin::ExternalPluginCommand;
use crate::plugin::Plugin;
use crate::plugin::LIST;
use camino::Utf8PathBuf;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self};
use std::path::Path;
use std::path::PathBuf;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::SoftwareListCommand;
use tedge_api::commands::SoftwareUpdateCommand;
use tedge_api::CommandLog;
use tedge_api::SoftwareError;
use tedge_api::SoftwareType;
use tedge_api::DEFAULT;
use tedge_config::SudoCommandBuilder;
use tedge_config::SudoError;
use tracing::error;
use tracing::info;
use tracing::warn;

/// The main responsibility of a `Plugins` implementation is to retrieve the appropriate plugin for a given software module.
pub trait Plugins {
    type Plugin;

    /// Return the plugin to be used by default when installing a software module, if any.
    fn default(&self) -> Option<&Self::Plugin>;

    /// Return the plugin declared with the given name, if any.
    fn by_software_type(&self, software_type: &str) -> Option<&Self::Plugin>;

    /// Return the plugin associated with the file extension of the module name, if any.
    fn by_file_extension(&self, module_name: &str) -> Option<&Self::Plugin>;

    fn update_default(&mut self, new_default: &Option<SoftwareType>) -> Result<(), SoftwareError>;

    fn get_all_software_types(&self) -> Vec<SoftwareType>;
}

#[derive(Debug)]
pub struct ExternalPlugins {
    plugin_dir: PathBuf,
    plugin_map: BTreeMap<SoftwareType, ExternalPluginCommand>,
    default_plugin_type: Option<SoftwareType>,
    sudo: SudoCommandBuilder,
    config_dir: Utf8PathBuf,
}

impl Plugins for ExternalPlugins {
    type Plugin = ExternalPluginCommand;

    fn default(&self) -> Option<&Self::Plugin> {
        if let Some(default_plugin_type) = &self.default_plugin_type {
            self.by_software_type(default_plugin_type.as_str())
        } else if self.plugin_map.len() == 1 {
            Some(self.plugin_map.iter().next().unwrap().1) //Unwrap is safe here as one entry is guaranteed
        } else {
            None
        }
    }

    fn update_default(&mut self, new_default: &Option<SoftwareType>) -> Result<(), SoftwareError> {
        new_default.clone_into(&mut self.default_plugin_type);
        Ok(())
    }

    fn by_software_type(&self, software_type: &str) -> Option<&Self::Plugin> {
        if software_type.eq(DEFAULT) {
            self.default()
        } else {
            self.plugin_map.get(software_type)
        }
    }

    fn by_file_extension(&self, module_name: &str) -> Option<&Self::Plugin> {
        if let Some(dot) = module_name.rfind('.') {
            let (_, extension) = module_name.split_at(dot + 1);
            self.by_software_type(extension)
        } else {
            self.default()
        }
    }

    fn get_all_software_types(&self) -> Vec<SoftwareType> {
        let mut software_types: Vec<SoftwareType> = self.plugin_map.keys().cloned().collect();
        software_types.sort();
        software_types
    }
}

impl ExternalPlugins {
    pub async fn open(
        plugin_dir: impl Into<PathBuf>,
        default_plugin_type: Option<String>,
        sudo: SudoCommandBuilder,
        config_dir: Utf8PathBuf,
    ) -> Result<ExternalPlugins, SoftwareError> {
        let mut plugins = ExternalPlugins {
            plugin_dir: plugin_dir.into(),
            plugin_map: BTreeMap::new(),
            default_plugin_type: default_plugin_type.clone(),
            sudo,
            config_dir,
        };
        if let Err(e) = plugins.load().await {
            warn!(target: "SM plugins",
                "Reading the plugins directory ({:?}): failed with: {e:?}",
                &plugins.plugin_dir
            );
            return Ok(plugins);
        }

        match default_plugin_type {
            Some(default_plugin_type) => {
                if plugins
                    .by_software_type(default_plugin_type.as_str())
                    .is_none()
                {
                    warn!(target: "SM plugins",
                        "The configured default plugin: {} not found",
                        default_plugin_type
                    );
                }
                info!(target: "SM plugins", "Default plugin type: {}", default_plugin_type)
            }
            None => {
                info!(target: "SM plugins", "Default plugin type: Not configured")
            }
        }

        Ok(plugins)
    }

    pub async fn load(&mut self) -> anyhow::Result<()> {
        self.plugin_map.clear();

        let config = tedge_config::TEdgeConfig::load(&self.config_dir)
            .await
            .map_err(|err| io::Error::other(format!("Failed to load tedge config: {}", err)))?;

        for maybe_entry in fs::read_dir(&self.plugin_dir)? {
            let entry = maybe_entry?;
            let path = entry.path();
            if path.is_file() {
                match self.sudo.ensure_command_succeeds(&path, &vec![LIST]) {
                    Ok(()) => {
                        info!(target: "SM plugins", "Plugin activated: {}", path.display());
                    }
                    Err(SudoError::CannotSudo) => {
                        error!(target: "SM plugins",
                            "Skipping {}: not properly configured to run with sudo",
                            path.display()
                        );
                        continue;
                    }
                    Err(SudoError::ExecutionFailed(_)) => {
                        error!(target: "SM plugins",
                            "Skipping {}: does not support list operation and may not be a valid plugin",
                            path.display()
                        );
                        continue;
                    }
                    Err(err) => {
                        error!(target: "SM plugins",
                            "Skipping {}: can not be launched as a plugin: {err}",
                            path.display()
                        );
                        continue;
                    }
                }

                if let Some(file_name) = path.file_name() {
                    if let Some(plugin_name) = file_name.to_str() {
                        let identity = config.http.client.auth.identity()?;
                        let plugin = ExternalPluginCommand::new(
                            plugin_name,
                            &path,
                            self.sudo.clone(),
                            config.software.plugin.max_packages,
                            config.software.plugin.exclude.or_none().cloned(),
                            config.software.plugin.include.or_none().cloned(),
                            identity,
                            config.cloud_root_certs()?,
                            config.tmp.path.as_path().into(),
                        );
                        self.plugin_map.insert(plugin_name.into(), plugin);
                    }
                }
            }
        }

        Ok(())
    }

    pub fn empty(&self) -> bool {
        self.plugin_map.is_empty()
    }

    pub async fn list(
        &self,
        mut response: SoftwareListCommand,
        mut command_log: Option<CommandLog>,
    ) -> SoftwareListCommand {
        let mut errors = Vec::new();

        if self.plugin_map.is_empty() {
            response.add_modules("".into(), vec![]);
        } else {
            for (software_type, plugin) in self.plugin_map.iter() {
                match plugin.list(command_log.as_mut()).await {
                    Ok(software_list) => response.add_modules(software_type.clone(), software_list),
                    Err(err) => errors.push(err.to_string()),
                }
            }
        }

        if let Some(reason) = ExternalPlugins::error_message(errors, command_log) {
            response.with_error(reason)
        } else {
            response.with_status(CommandStatus::Successful)
        }
    }

    pub async fn process(
        &self,
        request: SoftwareUpdateCommand,
        mut command_log: Option<CommandLog>,
        download_path: &Path,
    ) -> SoftwareUpdateCommand {
        let mut response = request.clone().with_status(CommandStatus::Executing);
        let mut error_messages = Vec::new();

        for software_type in request.modules_types() {
            let updates = request.updates_for(&software_type);
            let errors = if let Some(plugin) = self.by_software_type(&software_type) {
                plugin
                    .apply_all(updates, command_log.as_mut(), download_path)
                    .await
            } else {
                let error = SoftwareError::UnknownSoftwareType {
                    software_type: software_type.clone(),
                    updates,
                };
                if let Some(command_log) = &mut command_log {
                    command_log.log_error(&error.to_string()).await;
                }
                vec![error]
            };

            if !errors.is_empty() {
                let message = errors
                    .iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                error_messages.push(message);
                response.add_errors(&software_type, errors);
            }
        }

        if let Some(reason) = ExternalPlugins::error_message(error_messages, command_log) {
            response.with_error(reason)
        } else {
            response.with_status(CommandStatus::Successful)
        }
    }

    fn error_message(errors: Vec<String>, command_log: Option<CommandLog>) -> Option<String> {
        if !errors.is_empty() {
            let reason = match &errors[..] {
                [single_error_message] => Cow::Borrowed(single_error_message),
                _ => Cow::Owned(format!("{} errors", errors.len())),
            };
            let reason = command_log
                .map(|log| format!("{}, see device log file {}", reason, log.path))
                .unwrap_or_else(|| reason.into_owned());
            Some(reason)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn test_no_sm_plugin_dir() {
        let config_dir = TempTedgeDir::new();
        let mut plugin_dir_path = config_dir.utf8_path_buf();
        plugin_dir_path.push("sm-plugins");

        let actual = ExternalPlugins::open(
            plugin_dir_path,
            None,
            SudoCommandBuilder::enabled(false),
            config_dir.utf8_path_buf(),
        )
        .await;
        assert!(actual.is_ok());
    }
}
