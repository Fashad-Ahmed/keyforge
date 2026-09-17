pub mod event;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub use event::SoundEvent;

pub(crate) type SoundEventSink = Box<dyn Fn(SoundEvent) + Send + 'static>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum InputError {
    PermissionDenied,
    Unavailable,
    UnsupportedPlatform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum InputStatus {
    Ready,
    Unsupported,
    PermissionDenied,
    Unavailable,
}

impl InputStatus {
    pub(crate) fn from_error(error: InputError) -> Self {
        match error {
            InputError::PermissionDenied => Self::PermissionDenied,
            InputError::Unavailable => Self::Unavailable,
            InputError::UnsupportedPlatform => Self::Unsupported,
        }
    }
}

#[cfg(target_os = "linux")]
pub(crate) use linux::InputListener;
#[cfg(target_os = "macos")]
pub(crate) use macos::InputListener;
#[cfg(target_os = "windows")]
pub(crate) use windows::InputListener;

pub(crate) fn start_listener_for_platform(
    sink: SoundEventSink,
) -> Result<InputListener, InputError> {
    platform_start_listener(sink)
}

#[cfg(target_os = "linux")]
fn platform_start_listener(sink: SoundEventSink) -> Result<InputListener, InputError> {
    linux::start_listener(sink)
}

#[cfg(target_os = "macos")]
fn platform_start_listener(sink: SoundEventSink) -> Result<InputListener, InputError> {
    macos::start_listener(sink)
}

#[cfg(target_os = "windows")]
fn platform_start_listener(sink: SoundEventSink) -> Result<InputListener, InputError> {
    windows::start_listener(sink)
}

#[cfg(test)]
mod tests {
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn unsupported_platform_reports_unsupported_without_starting_listener() {
        use super::*;

        let result = start_listener_for_platform(Box::new(|_| {}));

        assert!(matches!(result, Err(InputError::UnsupportedPlatform)));
        assert_eq!(
            InputStatus::from_error(result.unwrap_err()),
            InputStatus::Unsupported
        );
    }
}
