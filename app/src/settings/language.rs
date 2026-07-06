use settings::macros::define_settings_group;
use settings::{RespectUserSyncSetting, SupportedPlatforms, SyncToCloud};
use warp_i18n::Language;

define_settings_group!(LanguageSettings, settings: [
    app_language: AppLanguage {
        type: Language,
        default: Language::English,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        surface: settings::SettingSurfaces::GUI,
        private: false,
        toml_path: "general.language",
        description: "The display language used by Warp.",
    },
]);
