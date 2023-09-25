use std::path::Path;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::MessageReceiver;
use tedge_actors::NoMessage;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_downloader_ext::DownloadResponse;
use tedge_file_system_ext::FsWatchEvent;
use tedge_http_ext::test_helpers::assert_request_eq;
use tedge_http_ext::test_helpers::HttpResponseBuilder;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResult;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tedge_test_utils::fs::TempTedgeDir;

use crate::actor::ConfigDownloadRequest;
use crate::actor::ConfigDownloadResult;
use crate::ConfigManagerBuilder;
use crate::ConfigManagerConfig;

const TEST_TIMEOUT_MS: Duration = Duration::from_secs(5);

type MqttMessageBox = TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>;
type DownloaderMessageBox =
    TimedMessageBox<SimpleMessageBox<ConfigDownloadRequest, ConfigDownloadResult>>;

fn prepare() -> Result<TempTedgeDir, anyhow::Error> {
    let tempdir = TempTedgeDir::new();
    let tempdir_path = tempdir
        .path()
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("temp dir not created"))?;

    std::fs::File::create(format!("{tempdir_path}/file_a"))?;
    tempdir.file("file_b").with_raw_content("Some content");
    std::fs::File::create(format!("{tempdir_path}/file_c"))?;
    std::fs::File::create(format!("{tempdir_path}/file_d"))?;

    tempdir
        .file("tedge-configuration-plugin.toml")
        .with_raw_content(&format!(
            r#"files = [
            {{ path = "{tempdir_path}/file_a", type = "type_one" }},
            {{ path = "{tempdir_path}/file_b", type = "type_two" }},
            {{ path = "{tempdir_path}/file_c", type = "type_three" }},
            {{ path = "{tempdir_path}/file_d", type = "type_four" }},
        ]"#
        ));

    Ok(tempdir)
}

#[allow(clippy::type_complexity)]
fn new_config_manager_builder(
    temp_dir: &Path,
) -> (
    ConfigManagerBuilder,
    MqttMessageBox,
    SimpleMessageBox<HttpRequest, HttpResult>,
    SimpleMessageBox<NoMessage, FsWatchEvent>,
    DownloaderMessageBox,
) {
    let config = ConfigManagerConfig {
        config_dir: temp_dir.to_path_buf(),
        plugin_config_dir: temp_dir.to_path_buf(),
        plugin_config_path: temp_dir.join("tedge-configuration-plugin.toml"),
        config_reload_topics: vec![
            "te/device/main///cmd/config_snapshot",
            "te/device/main///cmd/config_update",
        ]
        .try_into()
        .expect("Infallible"),
        config_snapshot_topic: TopicFilter::new_unchecked("te/device/main///cmd/config_snapshot/+"),
        config_update_topic: TopicFilter::new_unchecked("te/device/main///cmd/config_update/+"),
    };

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mut http_builder: SimpleMessageBoxBuilder<HttpRequest, HttpResult> =
        SimpleMessageBoxBuilder::new("HTTP", 1);
    let mut fs_watcher_builder: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent> =
        SimpleMessageBoxBuilder::new("FS", 5);
    let mut downloader_builder: SimpleMessageBoxBuilder<
        ConfigDownloadRequest,
        ConfigDownloadResult,
    > = SimpleMessageBoxBuilder::new("Downloader", 5);

    let config_builder = ConfigManagerBuilder::try_new(
        config,
        &mut mqtt_builder,
        &mut http_builder,
        &mut fs_watcher_builder,
        &mut downloader_builder,
    )
    .unwrap();

    (
        config_builder,
        mqtt_builder.build().with_timeout(TEST_TIMEOUT_MS),
        http_builder.build(),
        fs_watcher_builder.build(),
        downloader_builder.build().with_timeout(TEST_TIMEOUT_MS),
    )
}

fn spawn_config_manager_actor(
    temp_dir: &Path,
) -> (
    MqttMessageBox,
    SimpleMessageBox<HttpRequest, HttpResult>,
    SimpleMessageBox<NoMessage, FsWatchEvent>,
    DownloaderMessageBox,
) {
    let (actor_builder, mqtt, http, fs, downloader) = new_config_manager_builder(temp_dir);
    let mut actor = actor_builder.build();
    tokio::spawn(async move { actor.run().await });
    (mqtt, http, fs, downloader)
}

#[tokio::test]
async fn config_manager_reloads_config_types() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _http, _fs, _downloader) = spawn_config_manager_actor(tempdir.path());

    let config_snapshot_reload_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot");
    let config_update_reload_topic = Topic::new_unchecked("te/device/main///cmd/config_update");

    assert_eq!(
        mqtt.recv().await,
        Some(
            MqttMessage::new(
                &config_snapshot_reload_topic,
                r#"{"types":["tedge-configuration-plugin","type_four","type_one","type_three","type_two"]}"#
            )
            .with_retain()
        )
    );

    assert_eq!(
        mqtt.recv().await,
        Some(
            MqttMessage::new(
                &config_update_reload_topic,
                r#"{"types":["tedge-configuration-plugin","type_four","type_one","type_three","type_two"]}"#
            )
            .with_retain()
        )
    );

    Ok(())
}

#[tokio::test]
async fn config_manager_uploads_snapshot() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, mut http, _fs, _downloader) = spawn_config_manager_actor(tempdir.path());

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    // Let's ignore the reload messages sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_two-1234",
            "type": "type_two"
        }"#;

    mqtt.send(MqttMessage::new(&config_topic, snapshot_request).with_retain())
        .await?;

    // The config manager notifies that the request has been received and is processed
    let executing_message = mqtt.recv().await;
    assert_eq!(
            executing_message,
            Some(MqttMessage::new(
                &config_topic,
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_two-1234","type":"type_two"}"#
            ).with_retain())
        );

    // This message being published over MQTT is also received by the config-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Then uploads the requested content over HTTP
    let actual_request = http.recv().await;
    let expected_request = Some(
        HttpRequestBuilder::put(
            "http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_two-1234",
        )
        .header("Content-Type", "text/plain")
        .body("filename: file_b\nSome content\n".to_string())
        .build()
        .unwrap(),
    );
    assert_request_eq(actual_request, expected_request);

    // File transfer responds with 201
    let response = HttpResponseBuilder::new().status(201).build().unwrap();
    http.send(Ok(response)).await?;

    // Finally, the config manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &config_topic,
                format!(r#"{{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_two-1234","type":"type_two","path":{:?}}}"#, tempdir.path().join("file_b"))
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn config_manager_download_update() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _http, _fs, mut downloader) = spawn_config_manager_actor(tempdir.path());

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_update/1234");

    // Let's ignore the reload messages sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/config_update/type_two-1234",
            "remoteUrl": "http://www.remote.url",
            "type": "type_two"
        }"#;

    mqtt.send(MqttMessage::new(&config_topic, snapshot_request).with_retain())
        .await?;

    // The config manager notifies that the request has been received and is processed
    let executing_message = mqtt.recv().await;
    assert_eq!(
        executing_message,
            Some(MqttMessage::new(
                &config_topic,
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config_update/type_two-1234","remoteUrl":"http://www.remote.url","type":"type_two"}"#
            ).with_retain())
        );

    // This message being published over MQTT is also received by the config-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Assert config download request.
    let (topic, download_request) = downloader.recv().await.unwrap();

    assert_eq!(Topic::new_unchecked(&topic), config_topic);

    assert_eq!(
        download_request.url,
        "http://127.0.0.1:3000/tedge/file-transfer/main/config_update/type_two-1234"
    );
    assert_eq!(download_request.file_path, tempdir.path().join("file_b"));

    assert_eq!(download_request.auth, None);

    // Simulate downloading a file is completed.
    let download_response =
        DownloadResponse::new(&download_request.url, &download_request.file_path);
    downloader.send((topic, Ok(download_response))).await?;

    // Finally, the config manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &config_topic,
                format!(r#"{{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config_update/type_two-1234","remoteUrl":"http://www.remote.url","type":"type_two","path":{:?}}}"#, tempdir.path().join("file_b"))
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn request_config_snapshot_that_does_not_exist() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _http, _fs, _downloader) = spawn_config_manager_actor(tempdir.path());

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_five-1234",
            "type": "type_five"
        }"#;

    mqtt.send(MqttMessage::new(&config_topic, snapshot_request).with_retain())
        .await?;

    let executing_message = mqtt.recv().await;
    // The config manager notifies that the request has been received and is processed
    assert_eq!(
        executing_message,
        Some(MqttMessage::new(
            &config_topic,
            r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_five-1234","type":"type_five"}"#
        ).with_retain())
    );

    // This message being published over MQTT is also received by the config-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Finally, the config manager notifies that given config type does not exists
    assert_eq!(
        mqtt.recv().await,
        Some(MqttMessage::new(
            &config_topic,
            r#"{"status":"failed","reason":"Handling of operation failed with The requested config_type type_five is not defined in the plugin configuration file.","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_five-1234","type":"type_five"}"#
        ).with_retain())
    );

    Ok(())
}

#[tokio::test]
async fn put_config_snapshot_without_permissions() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, mut http, _fs, _downloader) = spawn_config_manager_actor(tempdir.path());

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_two-1234",
            "type": "type_two"
        }"#;

    mqtt.send(MqttMessage::new(&config_topic, snapshot_request).with_retain())
        .await?;

    let executing_message = mqtt.recv().await;
    // The config manager notifies that the request has been received and is processed
    assert_eq!(
            executing_message,
            Some(MqttMessage::new(
                &config_topic,
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_two-1234","type":"type_two"}"#
            ).with_retain())
        );

    // This message being published over MQTT is also received by the config-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Then uploads the requested content over HTTP
    assert!(http.recv().await.is_some());

    // File transfer responds with error code
    let response = HttpResponseBuilder::new().status(403).build().unwrap();
    http.send(Ok(response)).await?;

    // Finally, the config manager notifies that could not upload config snapshot via HTTP
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &config_topic,
                r#"{"status":"failed","reason":"Handling of operation failed with Failed with HTTP error status 403 Forbidden","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_two-1234","type":"type_two"}"#
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn ignore_topic_for_another_device() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _http, _fs, _downloader) = spawn_config_manager_actor(tempdir.path());

    // Check for child device topic
    let another_device_topic = Topic::new_unchecked("te/device/child01///cmd/config-snapshot/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/child01/config-snapshot/type_two-1234",
            "type": "type_two"
        }"#;

    mqtt.send(MqttMessage::new(&another_device_topic, snapshot_request).with_retain())
        .await?;

    // The config manager does proceed to "executing" state
    assert!(mqtt.recv().await.is_none());

    Ok(())
}

#[tokio::test]
async fn send_incorrect_payload() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _http, _fs, _downloader) = spawn_config_manager_actor(tempdir.path());

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received with url instead of tedgeUrl
    let snapshot_request = r#"
        {
            "status": "init",
            "url": "http://127.0.0.1:3000/tedge/file-transfer/child01/config-snapshot/type_two-1234",
            "type": "type_two"
        }"#;

    mqtt.send(MqttMessage::new(&config_topic, snapshot_request).with_retain())
        .await?;

    // The config manager does proceed to "executing" state
    assert!(mqtt.recv().await.is_none());

    Ok(())
}