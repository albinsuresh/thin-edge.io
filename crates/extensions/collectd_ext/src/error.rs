use tedge_actors::RuntimeError;
use tokio::sync::mpsc::error::SendError;

#[derive(thiserror::Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum DeviceMonitorError {
    #[error(transparent)]
    FromMqttClient(#[from] tedge_mqtt_ext::MqttError),

    #[error(transparent)]
    FromInvalidCollectdMeasurement(#[from] crate::collectd::CollectdError),

    #[error(transparent)]
    FromInvalidThinEdgeJson(#[from] tedge_api::measurement::MeasurementGrouperError),

    #[error(transparent)]
    FromThinEdgeJsonSerializationError(
        #[from] tedge_api::measurement::ThinEdgeJsonSerializationError,
    ),

    #[error(transparent)]
    FromBatchingError(#[from] SendError<tedge_api::measurement::MeasurementGrouper>),
}

impl From<DeviceMonitorError> for RuntimeError {
    fn from(error: DeviceMonitorError) -> Self {
        Box::new(error).into()
    }
}
