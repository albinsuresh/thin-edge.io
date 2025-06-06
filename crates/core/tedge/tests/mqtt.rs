#[cfg(test)]
mod tests {

    // These test cases need mosquitto on localhost on GH hosted machine.

    use std::time::Duration;

    use assert_cmd::assert::OutputAssertExt;
    use assert_cmd::Command;
    use predicates::prelude::predicate;
    use test_case::test_case;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    fn make_config(port: u16) -> Result<tempfile::TempDir, anyhow::Error> {
        let dir = tempfile::TempDir::new().unwrap();
        let toml_conf = &format!("[mqtt]\nclient_port = {port}");

        std::fs::write(dir.path().join("tedge.toml"), toml_conf.as_bytes())?;
        Ok(dir)
    }

    #[test_case(Some("0"))]
    #[test_case(Some("1"))]
    #[test_case(Some("2"))]
    #[test_case(None)]
    #[tokio::test]
    async fn test_cli_pub_basic(qos: Option<&str>) -> Result<(), anyhow::Error> {
        let broker = mqtt_tests::test_mqtt_broker();
        let tmpfile = make_config(broker.port)?;

        let mut messages = broker.messages_published_on("topic").await;

        let mut cmd = Command::cargo_bin("tedge")?;
        cmd.args(["--config-dir", tmpfile.path().to_str().unwrap()])
            .args(["mqtt", "pub", "topic", "message"]);

        if let Some(qos) = qos {
            cmd.args(["--qos", qos]);
        }
        let assert = cmd.unwrap().assert();

        mqtt_tests::assert_received_all_expected(&mut messages, TEST_TIMEOUT_MS, &["message"])
            .await;

        assert.success().code(predicate::eq(0));
        Ok(())
    }
}
