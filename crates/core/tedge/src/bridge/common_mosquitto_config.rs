use camino::Utf8PathBuf;
use tedge_config::TEdgeConfig;
use tedge_utils::paths::DraftFile;
use tokio::io::AsyncWriteExt;

use super::TEDGE_BRIDGE_CONF_DIR_PATH;

const COMMON_MOSQUITTO_CONFIG_FILENAME: &str = "tedge-mosquitto.conf";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListenerConfig {
    pub port: Option<u16>,
    pub bind_address: Option<String>,
    pub bind_interface: Option<String>,
    pub allow_anonymous: bool,
    pub capath: Option<Utf8PathBuf>,
    pub certfile: Option<Utf8PathBuf>,
    pub keyfile: Option<Utf8PathBuf>,
    pub require_certificate: bool,
}

impl Default for ListenerConfig {
    fn default() -> Self {
        Self {
            port: None,
            bind_address: None,
            bind_interface: None,
            allow_anonymous: false,
            capath: None,
            certfile: None,
            keyfile: None,
            require_certificate: true,
        }
    }
}

impl ListenerConfig {
    async fn maybe_writeln<W: AsyncWriteExt + Unpin + ?Sized, D: std::fmt::Display>(
        &self,
        writer: &mut W,
        key: &str,
        value: Option<D>,
    ) -> std::io::Result<()> {
        if let Some(value) = value {
            self.writeln(writer, key, value).await?;
        }
        Ok(())
    }
    async fn writeln<W: AsyncWriteExt + Unpin + ?Sized, D: std::fmt::Display>(
        &self,
        writer: &mut W,
        key: &str,
        value: D,
    ) -> std::io::Result<()> {
        writer
            .write_all(format!("{key} {value}\n").as_bytes())
            .await
    }

    pub async fn write(&self, writer: &mut (impl AsyncWriteExt + Unpin)) -> std::io::Result<()> {
        let bind_address = self.bind_address.clone().unwrap_or_default();
        let maybe_listener = self
            .port
            .as_ref()
            .map(|port| format!("{} {}", port, bind_address));
        match maybe_listener {
            None => Ok(()),
            Some(listener) => {
                self.writeln(writer, "listener", listener).await?;
                self.writeln(writer, "allow_anonymous", self.allow_anonymous)
                    .await?;
                self.writeln(writer, "require_certificate", self.require_certificate)
                    .await?;
                self.maybe_writeln(writer, "bind_interface", self.bind_interface.as_ref())
                    .await?;
                self.maybe_writeln(writer, "capath", self.capath.as_ref())
                    .await?;
                self.maybe_writeln(writer, "certfile", self.certfile.as_ref())
                    .await?;
                self.maybe_writeln(writer, "keyfile", self.keyfile.as_ref())
                    .await
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommonMosquittoConfig {
    pub config_file: String,
    pub internal_listener: ListenerConfig,
    pub external_listener: ListenerConfig,
    pub log_types: Vec<String>,
    pub message_size_limit: u32,
}

impl Default for CommonMosquittoConfig {
    fn default() -> Self {
        CommonMosquittoConfig {
            config_file: COMMON_MOSQUITTO_CONFIG_FILENAME.into(),
            internal_listener: ListenerConfig {
                port: Some(1883),
                bind_address: Some("127.0.0.1".into()),
                allow_anonymous: true,
                require_certificate: false,
                ..Default::default()
            },
            external_listener: Default::default(),
            log_types: vec![
                "error".into(),
                "warning".into(),
                "notice".into(),
                "information".into(),
                "subscribe".into(),
                "unsubscribe".into(),
            ],
            message_size_limit: 256 * 1024 * 1024 - 1,
        }
    }
}

impl CommonMosquittoConfig {
    pub async fn serialize<W: AsyncWriteExt + Unpin>(&self, writer: &mut W) -> std::io::Result<()> {
        writer.write_all(b"per_listener_settings true\n").await?;

        writer.write_all(b"connection_messages true\n").await?;

        for log_type in &self.log_types {
            writer
                .write_all(format!("log_type {log_type}\n").as_bytes())
                .await?;
        }

        writer
            .write_all(format!("message_size_limit {}\n", self.message_size_limit).as_bytes())
            .await?;

        self.internal_listener.write(writer).await?;
        self.external_listener.write(writer).await?;
        writer.flush().await?;

        Ok(())
    }

    pub fn with_internal_opts(self, port: u16, bind_address: String) -> Self {
        let internal_listener = ListenerConfig {
            port: Some(port),
            bind_address: Some(bind_address),
            ..self.internal_listener
        };
        Self {
            internal_listener,
            ..self
        }
    }

    pub fn with_external_opts(
        self,
        port: Option<u16>,
        bind_address: Option<String>,
        bind_interface: Option<String>,
        capath: Option<Utf8PathBuf>,
        certfile: Option<Utf8PathBuf>,
        keyfile: Option<Utf8PathBuf>,
    ) -> Self {
        let mut external_listener = ListenerConfig {
            port,
            bind_address,
            bind_interface,
            capath,
            certfile,
            keyfile,
            allow_anonymous: true,
            require_certificate: false,
        };

        if external_listener.capath.is_some() {
            external_listener.allow_anonymous = false;
            external_listener.require_certificate = true;
        }

        Self {
            external_listener,
            ..self
        }
    }

    pub fn from_tedge_config(config: &TEdgeConfig) -> Self {
        CommonMosquittoConfig::default()
            .with_internal_opts(
                config.mqtt.bind.port.into(),
                config.mqtt.bind.address.to_string(),
            )
            .with_external_opts(
                config.mqtt.external.bind.port.or_none().cloned(),
                config
                    .mqtt
                    .external
                    .bind
                    .address
                    .or_none()
                    .cloned()
                    .map(|a| a.to_string()),
                config.mqtt.external.bind.interface.or_none().cloned(),
                config
                    .mqtt
                    .external
                    .ca_path
                    .or_none()
                    .cloned()
                    .map(Utf8PathBuf::from),
                config
                    .mqtt
                    .external
                    .cert_file
                    .or_none()
                    .cloned()
                    .map(Utf8PathBuf::from),
                config
                    .mqtt
                    .external
                    .key_file
                    .or_none()
                    .cloned()
                    .map(Utf8PathBuf::from),
            )
    }

    /// Write the configuration file in a mosquitto configuration directory relative to the main
    /// tedge config location.
    pub async fn save(
        &self,
        tedge_config: &TEdgeConfig,
    ) -> Result<(), tedge_utils::paths::PathsError> {
        let dir_path = tedge_config.root_dir().join(TEDGE_BRIDGE_CONF_DIR_PATH);

        tedge_utils::paths::create_directories(dir_path)?;

        let config_path = self.file_path(tedge_config);
        let mut config_draft = DraftFile::new(config_path).await?.with_mode(0o644);
        self.serialize(&mut config_draft).await?;
        config_draft.persist().await?;

        Ok(())
    }

    fn file_path(&self, tedge_config: &TEdgeConfig) -> Utf8PathBuf {
        tedge_config
            .root_dir()
            .join(TEDGE_BRIDGE_CONF_DIR_PATH)
            .join(&self.config_file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_serialize() -> anyhow::Result<()> {
        let common_mosquitto_config = CommonMosquittoConfig::default();

        let mut buffer = Vec::new();
        common_mosquitto_config.serialize(&mut buffer).await?;

        let contents = String::from_utf8(buffer).unwrap();
        let config_set: std::collections::HashSet<&str> = contents
            .lines()
            .filter(|str| !str.is_empty() && !str.starts_with('#'))
            .collect();
        let mut expected = std::collections::HashSet::new();

        expected.insert("listener 1883 127.0.0.1");
        expected.insert("allow_anonymous true");
        expected.insert("connection_messages true");

        expected.insert("log_type error");
        expected.insert("log_type warning");
        expected.insert("log_type notice");
        expected.insert("log_type information");
        expected.insert("log_type subscribe");
        expected.insert("log_type unsubscribe");
        expected.insert("message_size_limit 268435455");
        expected.insert("per_listener_settings true");
        expected.insert("require_certificate false");

        assert_eq!(config_set, expected);

        Ok(())
    }

    #[tokio::test]
    async fn test_serialize_with_opts() -> anyhow::Result<()> {
        let common_mosquitto_config = CommonMosquittoConfig::default();
        let mosquitto_config_with_opts = common_mosquitto_config
            .with_internal_opts(1234, "1.2.3.4".into())
            .with_external_opts(
                Some(2345),
                Some("0.0.0.0".to_string()),
                Some("wlan0".into()),
                Some("/etc/ssl/certs".into()),
                Some("cert.pem".into()),
                Some("key.pem".into()),
            );

        assert!(mosquitto_config_with_opts
            .internal_listener
            .port
            .eq(&Some(1234)));

        let mut buffer = Vec::new();
        mosquitto_config_with_opts.serialize(&mut buffer).await?;

        let contents = String::from_utf8(buffer).unwrap();
        let expected = concat!(
            "per_listener_settings true\n",
            "connection_messages true\n",
            "log_type error\n",
            "log_type warning\n",
            "log_type notice\n",
            "log_type information\n",
            "log_type subscribe\n",
            "log_type unsubscribe\n",
            "message_size_limit 268435455\n",
            "listener 1234 1.2.3.4\n",
            "allow_anonymous true\n",
            "require_certificate false\n",
            "listener 2345 0.0.0.0\n",
            "allow_anonymous false\n",
            "require_certificate true\n",
            "bind_interface wlan0\n",
            "capath /etc/ssl/certs\n",
            "certfile cert.pem\n",
            "keyfile key.pem\n"
        );
        assert_eq!(contents, expected);

        Ok(())
    }
}
