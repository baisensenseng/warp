use warp_i18n::{set_current_ui_language, Language, Translator};
use warpui::{AppContext, Entity, ModelContext, SingletonEntity, Tracked};

use crate::settings::{LanguageSettings, LanguageSettingsChangedEvent};

pub enum LocalizationEvent {
    LanguageChanged,
}

pub struct Localization {
    language: Tracked<Language>,
    translator: Translator,
}

impl Localization {
    /// Creates the runtime localization singleton and subscribes to language setting changes.
    ///
    /// # Parameters
    /// - `ctx`: Model context used to read settings and register subscriptions.
    ///
    /// # Returns
    /// A localization model initialized with the persisted app language.
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let language = *LanguageSettings::as_ref(ctx).app_language;
        set_current_ui_language(language);
        let settings_handle = LanguageSettings::handle(ctx);
        ctx.subscribe_to_model(&settings_handle, |me, _, event, ctx| match event {
            LanguageSettingsChangedEvent::AppLanguage { .. } => {
                let language = *LanguageSettings::as_ref(ctx).app_language;
                me.set_language(language, ctx);
            }
        });

        Self {
            language: Tracked::new(language),
            translator: Translator::new(),
        }
    }

    /// Resolves a localized string for the current language.
    ///
    /// # Parameters
    /// - `key`: Fluent message identifier.
    ///
    /// # Returns
    /// Localized text, falling back to English or the key itself.
    pub fn text(&self, key: &'static str) -> String {
        self.translator.text(*self.language, key)
    }

    /// Resolves a localized string with Fluent arguments for the current language.
    ///
    /// # Parameters
    /// - `key`: Fluent message identifier.
    /// - `args`: Fluent argument name/value pairs.
    ///
    /// # Returns
    /// Localized formatted text, falling back to English or the key itself.
    pub fn text_with_args(&self, key: &'static str, args: &[(&str, String)]) -> String {
        self.translator.text_with_args(*self.language, key, args)
    }

    /// Updates the active language and schedules a full UI refresh when the value changes.
    ///
    /// # Parameters
    /// - `language`: Newly selected display language.
    /// - `ctx`: Model context used to notify subscribers and schedule view invalidation.
    ///
    /// # Returns
    /// Nothing.
    fn set_language(&mut self, language: Language, ctx: &mut ModelContext<Self>) {
        if *self.language == language {
            return;
        }

        *self.language = language;
        set_current_ui_language(language);
        ctx.notify();
        ctx.emit(LocalizationEvent::LanguageChanged);
        let _ = ctx.spawn(async {}, |_, _, ctx| {
            ctx.invalidate_all_views();
        });
    }
}

impl Entity for Localization {
    type Event = LocalizationEvent;
}

impl SingletonEntity for Localization {}

/// Resolves a localized string from the global localization singleton.
///
/// # Parameters
/// - `app`: Application context used to access the localization singleton.
/// - `key`: Fluent message identifier.
///
/// # Returns
/// Localized text for the current app language.
pub fn t(app: &AppContext, key: &'static str) -> String {
    Localization::as_ref(app).text(key)
}

/// Resolves a localized string with Fluent arguments from the global localization singleton.
///
/// # Parameters
/// - `app`: Application context used to access the localization singleton.
/// - `key`: Fluent message identifier.
/// - `args`: Fluent argument name/value pairs.
///
/// # Returns
/// Localized formatted text for the current app language.
pub fn t_args(app: &AppContext, key: &'static str, args: &[(&str, String)]) -> String {
    Localization::as_ref(app).text_with_args(key, args)
}
