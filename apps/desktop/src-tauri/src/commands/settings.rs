//! Settings (task 8.8) and the log folder (task 8.18).

use dbc_store::Settings;

use crate::error::AppError;
use crate::ipc::SettingsSetArgs;
use crate::state::AppState;

pub fn settings_get(state: &AppState) -> Settings {
    state.settings()
}

pub async fn settings_set(state: &AppState, args: SettingsSetArgs) -> Result<Settings, AppError> {
    let settings = args.settings;
    let saved = settings.clone();
    state
        .with_store(move |store| store.save_settings(&saved).map_err(AppError::from))
        .await?;
    state.replace_settings(settings.clone());
    Ok(settings)
}
