use crate::LinkError;
use crate::Message;
use crate::MessageSink;
use crate::MessageSource;
use crate::MessageSourceSink;

pub fn connect_one_way<T, C>(
    source: &mut impl MessageSource<T, C>,
    sink: &impl MessageSink<T>,
    config: C,
) -> Result<(), LinkError>
where
    T: Message,
{
    source.register_peer(config, sink.get_sender());
    Ok(())
}

pub fn connect_two_way<I, O, C1, C2>(
    peer1: &mut impl MessageSourceSink<I, O, C1>,
    config1: C1,
    peer2: &mut impl MessageSourceSink<O, I, C2>,
    config2: C2,
) -> Result<(), LinkError>
where
    I: Message,
    O: Message,
{
    peer1.register_peer(config1, peer2.get_sender());
    peer2.register_peer(config2, peer1.get_sender());
    Ok(())
}
