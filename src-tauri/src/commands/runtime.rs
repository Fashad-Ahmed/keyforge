use std::path::Path;

use tauri_plugin_dialog::{DialogExt, FilePath};

use crate::runtime::{
    ImportOutcome, KeyForgeRuntime, PackActionError, RuntimeControlError, RuntimeSnapshot,
};

pub(crate) fn get_runtime_status_from(runtime: &KeyForgeRuntime) -> RuntimeSnapshot {
    runtime.snapshot()
}

pub(crate) fn set_sound_enabled_on(
    runtime: &KeyForgeRuntime,
    enabled: bool,
) -> Result<RuntimeSnapshot, RuntimeControlError> {
    runtime.set_enabled(enabled)
}

pub(crate) fn set_master_volume_on(
    runtime: &KeyForgeRuntime,
    volume: f32,
) -> Result<RuntimeSnapshot, RuntimeControlError> {
    runtime.set_volume(volume)
}

pub(crate) fn import_sound_pack_from(
    runtime: &KeyForgeRuntime,
    source: Option<&Path>,
) -> Result<ImportOutcome, PackActionError> {
    let Some(source) = source else {
        return Ok(ImportOutcome::Cancelled);
    };
    runtime.install_pack(source).map(ImportOutcome::Installed)
}

pub(crate) fn select_sound_pack_on(
    runtime: &KeyForgeRuntime,
    pack_id: String,
) -> Result<RuntimeSnapshot, PackActionError> {
    runtime.select_pack(&pack_id)
}

#[tauri::command]
pub fn get_runtime_status(runtime: tauri::State<'_, KeyForgeRuntime>) -> RuntimeSnapshot {
    get_runtime_status_from(runtime.inner())
}

#[tauri::command]
pub fn set_sound_enabled(
    runtime: tauri::State<'_, KeyForgeRuntime>,
    enabled: bool,
) -> Result<RuntimeSnapshot, RuntimeControlError> {
    set_sound_enabled_on(runtime.inner(), enabled)
}

#[tauri::command]
pub fn set_master_volume(
    runtime: tauri::State<'_, KeyForgeRuntime>,
    volume: f32,
) -> Result<RuntimeSnapshot, RuntimeControlError> {
    set_master_volume_on(runtime.inner(), volume)
}

#[tauri::command]
pub async fn import_sound_pack(
    app: tauri::AppHandle,
    runtime: tauri::State<'_, KeyForgeRuntime>,
) -> Result<ImportOutcome, PackActionError> {
    let selected = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("KeyForge sound pack", &["zip"])
            .blocking_pick_file()
    })
    .await
    .map_err(|_| PackActionError::ActivationFailed)?;
    let path = selected.as_ref().and_then(|selected| match selected {
        FilePath::Path(path) => Some(path.as_path()),
        FilePath::Url(_) => None,
    });
    import_sound_pack_from(runtime.inner(), path)
}

#[tauri::command]
pub fn select_sound_pack(
    runtime: tauri::State<'_, KeyForgeRuntime>,
    pack_id: String,
) -> Result<RuntimeSnapshot, PackActionError> {
    select_sound_pack_on(runtime.inner(), pack_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_return_sanitized_status_and_apply_controls() {
        let runtime = KeyForgeRuntime::new_for_test();
        assert!(get_runtime_status_from(&runtime).sound_enabled);
        assert!(!set_sound_enabled_on(&runtime, false).unwrap().sound_enabled);
        assert_eq!(set_master_volume_on(&runtime, 0.25).unwrap().volume, 0.25);
    }

    #[test]
    fn cancelled_import_is_a_successful_sanitized_outcome() {
        let runtime = KeyForgeRuntime::new_for_test();

        assert_eq!(
            import_sound_pack_from(&runtime, None),
            Ok(ImportOutcome::Cancelled)
        );
    }

    #[test]
    fn invalid_selection_returns_a_sanitized_error() {
        let runtime = KeyForgeRuntime::new_for_test();

        assert_eq!(
            select_sound_pack_on(&runtime, "../private".to_owned()),
            Err(PackActionError::NotFound)
        );
        let _path_type_check: Option<&Path> = None;
    }
}
