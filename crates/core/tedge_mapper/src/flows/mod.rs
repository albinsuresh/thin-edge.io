use crate::core::mapper::start_basic_actors;
use crate::TEdgeComponent;
use tedge_config::TEdgeConfig;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_flows::FlowsMapperBuilder;
use tedge_watch_ext::WatchActorBuilder;

pub struct GenMapper;

#[async_trait::async_trait]
impl TEdgeComponent for GenMapper {
    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        config_dir: &tedge_config::Path,
    ) -> Result<(), anyhow::Error> {
        let (mut runtime, mut mqtt_actor) =
            start_basic_actors("tedge-flows", &tedge_config).await?;

        let mut fs_actor = FsWatchActorBuilder::new();
        let mut flows_mapper = FlowsMapperBuilder::try_new(config_dir.join("flows")).await?;
        flows_mapper.connect(&mut mqtt_actor);
        flows_mapper.connect_fs(&mut fs_actor);

        let mut cmd_watcher_actor = WatchActorBuilder::new();
        cmd_watcher_actor.connect(&mut flows_mapper);

        runtime.spawn(flows_mapper).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.spawn(fs_actor).await?;
        runtime.spawn(cmd_watcher_actor).await?;
        runtime.run_to_completion().await?;
        Ok(())
    }
}
