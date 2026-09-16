use crate::runtime::{KeyForgeRuntime, RuntimeControlError, RuntimeSnapshot};

pub(crate) fn get_runtime_status_from(runtime: &KeyForgeRuntime) -> RuntimeSnapshot {
    runtime.snapshot()
}

pub(crate) fn set_sound_enabled_on(runtime: &KeyForgeRuntime, enabled: bool) -> RuntimeSnapshot {
    runtime.set_enabled(enabled)
}

pub(crate) fn set_master_volume_on(
    runtime: &KeyForgeRuntime,
    volume: f32,
) -> Result<RuntimeSnapshot, RuntimeControlError> {
    runtime.set_volume(volume)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_return_sanitized_status_and_apply_controls() {
        let runtime = KeyForgeRuntime::new_for_test();
        assert!(get_runtime_status_from(&runtime).sound_enabled);
        assert!(!set_sound_enabled_on(&runtime, false).sound_enabled);
        assert_eq!(set_master_volume_on(&runtime, 0.25).unwrap().volume, 0.25);
    }
}
