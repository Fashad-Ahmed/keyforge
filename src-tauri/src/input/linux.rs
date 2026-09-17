use super::{InputError, SoundEventSink};

#[derive(Debug)]
pub(crate) struct InputListener;

pub(crate) fn start_listener(_sink: SoundEventSink) -> Result<InputListener, InputError> {
    Err(InputError::UnsupportedPlatform)
}
