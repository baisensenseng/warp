use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use serde::{Deserialize, Serialize};
use unic_langid::LanguageIdentifier;

const EN_US_RESOURCES: &[&str] = &[
    include_str!("../locales/en-US/common.ftl"),
    include_str!("../locales/en-US/settings.ftl"),
    include_str!("../locales/en-US/slash_commands.ftl"),
    include_str!("../locales/en-US/ui_literals.ftl"),
];

const ZH_CN_RESOURCES: &[&str] = &[
    include_str!("../locales/zh-CN/common.ftl"),
    include_str!("../locales/zh-CN/settings.ftl"),
    include_str!("../locales/zh-CN/slash_commands.ftl"),
    include_str!("../locales/zh-CN/ui_literals.ftl"),
];

static CURRENT_UI_LANGUAGE: AtomicU8 = AtomicU8::new(Language::English.as_u8());

thread_local! {
    static UI_LITERAL_TRANSLATOR: RefCell<Translator> = RefCell::new(Translator::new());
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Deserialize,
    Eq,
    Hash,
    PartialEq,
    schemars::JsonSchema,
    Serialize,
    settings_value::SettingsValue,
)]
#[serde(rename_all = "snake_case")]
#[schemars(rename_all = "snake_case")]
pub enum Language {
    #[default]
    English,
    SimplifiedChinese,
}

impl Language {
    /// All display languages currently supported by Warp.
    pub const ALL: [Self; 2] = [Self::English, Self::SimplifiedChinese];

    /// Returns a compact numeric value for storing the language in atomics.
    ///
    /// # Returns
    /// Stable numeric representation of the language.
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::English => 0,
            Self::SimplifiedChinese => 1,
        }
    }

    /// Converts a compact numeric value back into a language.
    ///
    /// # Parameters
    /// - `value`: Numeric language representation.
    ///
    /// # Returns
    /// Matching language, falling back to English for unknown values.
    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::SimplifiedChinese,
            0 => Self::English,
            _ => Self::English,
        }
    }

    /// Returns the BCP 47 locale identifier for this display language.
    ///
    /// # Returns
    /// Locale identifier used to build the corresponding Fluent bundle.
    pub fn locale(self) -> &'static str {
        match self {
            Self::English => "en-US",
            Self::SimplifiedChinese => "zh-CN",
        }
    }

    /// Returns the Fluent key for the language's native display name.
    ///
    /// # Returns
    /// Fluent message identifier for labels such as `English` or `简体中文`.
    pub fn native_name_key(self) -> &'static str {
        match self {
            Self::English => "common-language-english-native",
            Self::SimplifiedChinese => "common-language-simplified-chinese-native",
        }
    }

    /// Detects the best supported display language from the operating system locale.
    ///
    /// # Returns
    /// Supported language inferred from the system locale, falling back to English.
    pub fn detect_system() -> Self {
        sys_locale::get_locale()
            .or_else(|| std::env::var("LC_ALL").ok())
            .or_else(|| std::env::var("LC_MESSAGES").ok())
            .or_else(|| std::env::var("LANG").ok())
            .and_then(|locale| Self::from_locale_hint(&locale))
            .unwrap_or(Self::English)
    }

    /// Converts a locale hint into a supported display language.
    ///
    /// # Parameters
    /// - `locale`: BCP 47 or POSIX locale string such as `zh-CN`, `zh_Hans_CN`, or `en_US.UTF-8`.
    ///
    /// # Returns
    /// Matching supported language, or `None` when the locale is unsupported.
    pub fn from_locale_hint(locale: &str) -> Option<Self> {
        let normalized = locale
            .split('.')
            .next()
            .unwrap_or(locale)
            .replace('_', "-")
            .to_ascii_lowercase();

        if normalized == "zh" || normalized.starts_with("zh-") || normalized.starts_with("cmn-") {
            Some(Self::SimplifiedChinese)
        } else if normalized == "en" || normalized.starts_with("en-") {
            Some(Self::English)
        } else {
            None
        }
    }
}

pub struct Translator {
    bundles: HashMap<Language, FluentBundle<FluentResource>>,
}

impl Default for Translator {
    fn default() -> Self {
        Self::new()
    }
}

impl Translator {
    /// Builds a translator with all bundled Fluent resources loaded in memory.
    ///
    /// # Returns
    /// Translator containing one Fluent bundle per supported language.
    pub fn new() -> Self {
        let bundles = HashMap::from([
            (
                Language::English,
                build_bundle(Language::English, EN_US_RESOURCES),
            ),
            (
                Language::SimplifiedChinese,
                build_bundle(Language::SimplifiedChinese, ZH_CN_RESOURCES),
            ),
        ]);

        Self { bundles }
    }

    /// Resolves a localized message without arguments.
    ///
    /// # Parameters
    /// - `language`: Target display language.
    /// - `key`: Fluent message identifier.
    ///
    /// # Returns
    /// Localized text, falling back to English or the key itself.
    pub fn text(&self, language: Language, key: &'static str) -> String {
        self.text_with_args(language, key, &[])
    }

    /// Resolves a localized message with Fluent arguments.
    ///
    /// # Parameters
    /// - `language`: Target display language.
    /// - `key`: Fluent message identifier.
    /// - `args`: Fluent argument name/value pairs.
    ///
    /// # Returns
    /// Localized formatted text, falling back to English or the key itself.
    pub fn text_with_args(
        &self,
        language: Language,
        key: &'static str,
        args: &[(&str, String)],
    ) -> String {
        self.format(language, key, args)
            .or_else(|| self.format(Language::English, key, args))
            .unwrap_or_else(|| {
                log::warn!("Missing localization key: {key}");
                key.to_owned()
            })
    }

    /// Formats a Fluent message for one language without applying fallback logic.
    ///
    /// # Parameters
    /// - `language`: Target display language.
    /// - `key`: Fluent message identifier.
    /// - `args`: Fluent argument name/value pairs.
    ///
    /// # Returns
    /// Localized formatted text when the target bundle and message exist.
    fn format(
        &self,
        language: Language,
        key: &'static str,
        args: &[(&str, String)],
    ) -> Option<String> {
        let bundle = self.bundles.get(&language)?;
        let message = bundle.get_message(key)?;
        let pattern = message.value()?;
        let fluent_args = build_args(args);
        let mut errors = Vec::new();
        let formatted = bundle.format_pattern(
            pattern,
            if args.is_empty() {
                None
            } else {
                Some(&fluent_args)
            },
            &mut errors,
        );

        if !errors.is_empty() {
            log::warn!(
                "Localization formatting errors for {key} in {}: {errors:?}",
                language.locale()
            );
        }

        Some(formatted.into_owned())
    }
}

/// Updates the process-wide language used by UI literal translation fallbacks.
///
/// # Parameters
/// - `language`: Newly selected display language.
///
/// # Returns
/// Nothing.
pub fn set_current_ui_language(language: Language) {
    CURRENT_UI_LANGUAGE.store(language.as_u8(), Ordering::Relaxed);
}

/// Returns the process-wide language used by UI literal translation fallbacks.
///
/// # Returns
/// Currently selected display language for UI literals.
pub fn current_ui_language() -> Language {
    Language::from_u8(CURRENT_UI_LANGUAGE.load(Ordering::Relaxed))
}

/// Translates a built-in UI string literal when a non-English language is active.
///
/// # Parameters
/// - `text`: Built-in UI text to translate if it is present in the literal dictionary.
///
/// # Returns
/// Translated UI text for the current language, or the original text when no mapping exists.
pub fn translate_ui_literal(text: impl Into<Cow<'static, str>>) -> Cow<'static, str> {
    let text = text.into();
    let language = current_ui_language();
    if language == Language::English {
        return text;
    }

    let Some(key) = ui_literal_key(text.as_ref()) else {
        return translate_dynamic_ui_literal(text.as_ref(), language)
            .map(Cow::Owned)
            .unwrap_or(text);
    };

    Cow::Owned(UI_LITERAL_TRANSLATOR.with(|translator| translator.borrow().text(language, key)))
}

/// Translates generated UI strings whose runtime value includes interpolated data.
///
/// # Parameters
/// - `text`: Runtime English UI text after interpolation.
/// - `language`: Target display language.
///
/// # Returns
/// Translated UI text when a known runtime pattern matches.
fn translate_dynamic_ui_literal(text: &str, language: Language) -> Option<String> {
    match language {
        Language::SimplifiedChinese => translate_dynamic_ui_literal_zh_cn(text),
        Language::English => None,
    }
}

/// Translates known generated UI string patterns to Simplified Chinese.
///
/// # Parameters
/// - `text`: Runtime English UI text after interpolation.
///
/// # Returns
/// Simplified Chinese text when a known generated UI pattern matches.
fn translate_dynamic_ui_literal_zh_cn(text: &str) -> Option<String> {
    if let Some(row) = translate_agent_management_metadata_row_zh_cn(text) {
        return Some(row);
    }

    if let Some(prefixed_value) = translate_agent_management_prefixed_value_zh_cn(text) {
        return Some(prefixed_value);
    }

    if let Some(title) = translate_agent_management_title_zh_cn(text) {
        return Some(title);
    }

    if let Some((name, tab)) = text.split_once(" · Tab ") {
        return Some(format!("{name} · 标签页 {tab}"));
    }

    if let Some(rest) = text.strip_prefix("Created by ") {
        return Some(format!("由 {rest} 创建"));
    }

    if let Some(rest) = text.strip_prefix("Live session started at ") {
        if let Some((time, device)) = rest.rsplit_once(" on ") {
            return Some(format!("实时会话开始于 {time}，设备为 {device}"));
        }
    }

    if let Some(location) = text
        .strip_prefix("Send a prompt below to start a new conversation in `")
        .and_then(|text| text.strip_suffix("`"))
    {
        return Some(format!("在下方发送提示，开始在 {location} 中的新对话"));
    }

    if let Some(time) = translate_relative_time_zh_cn(text) {
        return Some(time);
    }

    if let Some(agent_name) = text
        .strip_prefix("Queue a follow up for the ")
        .and_then(|text| text.strip_suffix(" agent"))
    {
        return Some(format!("为 {agent_name} Agent 排队一条追问"));
    }

    if let Some(agent_name) = text
        .strip_prefix("Steer the ")
        .and_then(|text| text.strip_suffix(" agent"))
    {
        return Some(format!("引导 {agent_name} Agent"));
    }

    if let Some(agent_name) = text
        .strip_prefix("Ask the ")
        .and_then(|text| text.strip_suffix(" agent a follow up"))
    {
        return Some(format!("向 {agent_name} Agent 追问"));
    }

    if let Some(error) = text
        .strip_prefix("(no response body: ")
        .and_then(|text| text.strip_suffix(")"))
    {
        return Some(format!("（无响应正文：{error}）"));
    }

    if let Some(command) = text
        .strip_prefix("Running `")
        .and_then(|text| text.strip_suffix("`..."))
    {
        return Some(format!("正在运行 `{command}`..."));
    }

    if let Some(content) = text
        .strip_prefix("Comment addressed: \"")
        .and_then(|text| text.strip_suffix("\""))
    {
        return Some(format!("评论已处理：\"{content}\""));
    }

    if let Some(rest) = text.strip_prefix("Remote-server tarball download failed with status ") {
        if let Some((status, body)) = rest.split_once(": ") {
            return Some(format!(
                "远程服务器 tarball 下载失败，状态 {status}：{body}"
            ));
        }
    }

    if let Some(rest) = text.strip_prefix("Failed to authenticate with AWS Bedrock when using ") {
        if let Some((model, command)) =
            rest.split_once(". \n                     Run `")
                .and_then(|(model, rest)| {
                    rest.strip_suffix("` to refresh credentials.")
                        .map(|command| (model, command))
                })
        {
            return Some(format!(
                "使用 {model} 时无法通过 AWS Bedrock 认证。\n                     运行 `{command}` 以刷新凭据。"
            ));
        }
    }

    if let Some(rest) = text.strip_prefix("Failed to authenticate with ") {
        if let Some((provider, rest)) = rest.split_once(" when using ") {
            if let Some(model_name) = rest
                .strip_suffix(". \n                     Double-check that your API key is correct.")
            {
                return Some(format!(
                    "使用 {model_name} 时无法通过 {provider} 认证。\n                     请再次确认你的 API 密钥是否正确。"
                ));
            }
        }
    }

    translate_between(
        text,
        "Always open ",
        " on the web?",
        |value| format!("始终在网页端打开 {value}？"),
    )
    .or_else(|| {
        translate_between(
            text,
            "Are you sure you want to transfer team ownership to ",
            "? You will no longer be the owner and will not be able to take any administrative actions for this team.",
            |value| format!("确定要将团队所有权转让给 {value} 吗？你将不再是所有者，也无法再对此团队执行任何管理操作。"),
        )
    })
    .or_else(|| {
        translate_between(text, "Open image at ", "", |value| {
            format!("打开图片：{value}")
        })
    })
    .or_else(|| translate_between(text, "", "% off", |value| format!("优惠 {value}%")))
    .or_else(|| {
        translate_between(text, "Default: ", "", |value| {
            format!("默认值：{value}")
        })
    })
    .or_else(|| translate_between(text, "Credits used: ", ".", |value| format!("已用点数：{value}。")))
    .or_else(|| translate_between(text, "", " until refresh.", |value| format!("距离刷新还有 {value}。")))
    .or_else(|| {
        translate_between(text, "Name: ", "", |value| {
            format!("名称：{value}")
        })
    })
    .or_else(|| {
        translate_between(text, "", " outdated", |value| {
            format!("{value} 个已过期")
        })
    })
    .or_else(|| {
        translate_between(text, "Repo is initialized with a ", " file.", |value| {
            format!("仓库已使用 {value} 文件初始化。")
        })
    })
    .or_else(|| {
        translate_between(text, "error inserting text: ", "", |value| {
            format!("插入文本出错：{value}")
        })
    })
    .or_else(|| translate_between(text, "", " · Tab ", |value| format!("{value} · 标签页 ")))
    .or_else(|| {
        translate_between(text, "Create a file named ", "…", |value| {
            format!("创建名为 {value}… 的文件")
        })
    })
    .or_else(|| {
        translate_between(text, "Slash command: ", "", |value| {
            format!("斜杠命令：{value}")
        })
    })
    .or_else(|| {
        translate_between(text, "Install ", "", |value| {
            format!("安装 {value}")
        })
    })
    .or_else(|| {
        translate_between(text, "Update ", "", |value| {
            format!("更新 {value}")
        })
    })
    .or_else(|| {
        translate_between(text, "Agents (", ")", |value| {
            format!("Agent（{value}）")
        })
    })
    .or_else(|| translate_between(text, "(", " credits)", |value| format!("（{value} 点数）")))
    .or_else(|| {
        translate_between(text, "Limit: ", "", |value| {
            format!("限制：{value}")
        })
    })
    .or_else(|| {
        translate_between(text, "Your agent is currently running on a ", " machine. ", |value| {
            format!("你的 Agent 当前正在 {value} 机器上运行。 ")
        })
    })
    .or_else(|| translate_between(text, "", "% off!", |value| format!("优惠 {value}%！")))
    .or_else(|| {
        translate_between(text, "", " is not available for free users. ", |value| {
            format!("免费用户无法使用 {value}。 ")
        })
    })
    .or_else(|| {
        translate_between(text, "+ ", " more", |value| {
            format!("+ 另外 {value} 个")
        })
    })
    .or_else(|| {
        translate_between(text, "Open ", " in your browser", |value| {
            format!("在浏览器中打开 {value}")
        })
    })
    .or_else(|| {
        translate_between(text, "and enter code: ", "", |value| {
            format!("并输入代码：{value}")
        })
    })
    .or_else(|| {
        translate_between(text, "Login failed: ", "", |value| {
            format!("登录失败：{value}")
        })
    })
    .or_else(|| {
        translate_between(text, "Error: ", "", |value| {
            format!("错误：{value}")
        })
    })
    .or_else(|| {
        translate_between(text, "Debug information: ", "", |value| {
            format!("调试信息：{value}")
        })
    })
}

/// Translates Agent Management metadata rows split by bullet separators.
///
/// # Parameters
/// - `text`: Runtime metadata row such as `Source: Warp App • Harness: Warp`.
///
/// # Returns
/// Simplified Chinese metadata row when at least one segment matches.
fn translate_agent_management_metadata_row_zh_cn(text: &str) -> Option<String> {
    if !text.contains(" • ") {
        return None;
    }

    let mut changed = false;
    let translated = text
        .split(" • ")
        .map(|part| {
            if let Some(translated) = translate_agent_management_prefixed_value_zh_cn(part) {
                changed = true;
                translated
            } else {
                part.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" • ");

    changed.then_some(translated)
}

/// Translates Agent Management labels that are composed as `label: value`.
///
/// # Parameters
/// - `text`: Runtime label/value text such as `Status: All` or `Credits used: 10 credits`.
///
/// # Returns
/// Simplified Chinese label/value text when the label is known.
fn translate_agent_management_prefixed_value_zh_cn(text: &str) -> Option<String> {
    [
        ("Status: ", "状态"),
        ("Source: ", "来源"),
        ("Created on: ", "创建时间"),
        ("Has artifact: ", "有产物"),
        ("Harness: ", "运行框架"),
        ("Environment: ", "环境"),
        ("Created by: ", "创建者"),
        ("Credits used: ", "已用点数"),
        ("Run time: ", "运行时长"),
        ("Agent: ", "Agent"),
        ("Executor: ", "执行者"),
    ]
    .into_iter()
    .find_map(|(prefix, label)| {
        text.strip_prefix(prefix)
            .map(|value| format!("{label}：{}", translate_agent_management_value_zh_cn(value)))
    })
}

/// Translates known Agent Management selected values while preserving product names.
///
/// # Parameters
/// - `value`: Runtime selected value or metadata value.
///
/// # Returns
/// Simplified Chinese value, falling back to the original value for names and IDs.
fn translate_agent_management_value_zh_cn(value: &str) -> Cow<'_, str> {
    match value {
        "All" => Cow::Borrowed("全部"),
        "None" => Cow::Borrowed("无"),
        "Working" => Cow::Borrowed("进行中"),
        "Done" => Cow::Borrowed("已完成"),
        "Failed" => Cow::Borrowed("失败"),
        "Last 24 hours" => Cow::Borrowed("过去 24 小时"),
        "Past 3 days" => Cow::Borrowed("过去 3 天"),
        "Last week" => Cow::Borrowed("过去一周"),
        "Pull Request" => Cow::Borrowed("拉取请求"),
        "Plan" => Cow::Borrowed("计划"),
        "Screenshot" => Cow::Borrowed("截图"),
        "File" => Cow::Borrowed("文件"),
        "Warp App" => Cow::Borrowed("Warp 应用"),
        "Cloud Mode" => Cow::Borrowed("云模式"),
        "Agent Webhook" => Cow::Borrowed("Agent Webhook"),
        "CLI" => Cow::Borrowed("CLI"),
        "Linear" => Cow::Borrowed("Linear"),
        "Slack" => Cow::Borrowed("Slack"),
        "Scheduled Agent" => Cow::Borrowed("定时 Agent"),
        "Interactive" => Cow::Borrowed("交互式"),
        _ => translate_agent_management_credit_value_zh_cn(value)
            .map(Cow::Owned)
            .unwrap_or_else(|| Cow::Borrowed(value)),
    }
}

/// Translates Agent Management credit values while preserving the numeric part.
///
/// # Parameters
/// - `value`: Runtime value such as `10 credits` or `1 credit`.
///
/// # Returns
/// Simplified Chinese credit value when the suffix is recognized.
fn translate_agent_management_credit_value_zh_cn(value: &str) -> Option<String> {
    value
        .strip_suffix(" credits")
        .or_else(|| value.strip_suffix(" credit"))
        .map(|amount| format!("{amount} 点数"))
}

/// Translates common generated Agent Management task titles.
///
/// # Parameters
/// - `text`: Runtime task title displayed in the Agent Management list.
///
/// # Returns
/// Simplified Chinese title for known generated title patterns.
fn translate_agent_management_title_zh_cn(text: &str) -> Option<String> {
    match text {
        "Identify Project Directory Path" => return Some("识别项目目录路径".to_owned()),
        _ => {}
    }

    if let Some(tool) = text.strip_prefix("Run Development Server with ") {
        return Some(format!("使用 {tool} 运行开发服务器"));
    }

    if let Some(project) = text
        .strip_prefix("Change Directory To ")
        .and_then(|text| text.strip_suffix(" Project"))
    {
        return Some(format!("切换目录到 {project} 项目"));
    }

    None
}

/// Translates English relative time labels to Simplified Chinese.
///
/// # Parameters
/// - `text`: Runtime English relative time label.
///
/// # Returns
/// Simplified Chinese relative time label when the pattern is supported.
fn translate_relative_time_zh_cn(text: &str) -> Option<String> {
    match text {
        "Just now" | "just now" => return Some("刚刚".to_owned()),
        "1 min ago" | "1 minute ago" => return Some("1 分钟前".to_owned()),
        "1 hour ago" => return Some("1 小时前".to_owned()),
        "1 day ago" => return Some("1 天前".to_owned()),
        "1 week ago" => return Some("1 周前".to_owned()),
        "1 month ago" => return Some("1 个月前".to_owned()),
        "1 year ago" => return Some("1 年前".to_owned()),
        _ => {}
    }

    [
        (" min ago", " 分钟前"),
        (" minutes ago", " 分钟前"),
        (" hours ago", " 小时前"),
        (" days ago", " 天前"),
        (" weeks ago", " 周前"),
        (" months ago", " 个月前"),
        (" years ago", " 年前"),
    ]
    .into_iter()
    .find_map(|(suffix, translated_suffix)| {
        text.strip_suffix(suffix)
            .and_then(|value| value.parse::<u32>().ok())
            .map(|value| format!("{value}{translated_suffix}"))
    })
}

/// Translates text by preserving the runtime segment between a known prefix and suffix.
///
/// # Parameters
/// - `text`: Runtime English UI text.
/// - `prefix`: Required English prefix.
/// - `suffix`: Required English suffix.
/// - `render`: Formatter receiving the preserved runtime segment.
///
/// # Returns
/// Translated text when `text` starts with `prefix` and ends with `suffix`.
fn translate_between(
    text: &str,
    prefix: &str,
    suffix: &str,
    render: impl FnOnce(&str) -> String,
) -> Option<String> {
    text.strip_prefix(prefix)
        .and_then(|text| text.strip_suffix(suffix))
        .map(render)
}

/// Returns the Fluent key for a built-in UI string literal.
///
/// # Parameters
/// - `text`: Built-in English UI text.
///
/// # Returns
/// Fluent message identifier when the literal has a localized counterpart.
fn ui_literal_key(text: &str) -> Option<&'static str> {
    match text {
        "Warp is the intelligent terminal with AI and your dev team's knowledge built-in." => Some("ui-literal-0001"),
        "*Secrets are not sent to Warp's server." => Some("ui-literal-0002"),
        "Using Warp Offline" => Some("ui-literal-0003"),
        "Privacy Settings" => Some("ui-literal-0004"),
        "By continuing, you agree to Warp's " => Some("ui-literal-0005"),
        "Already have an account? " => Some("ui-literal-0006"),
        "Don't want to sign in right now? " => Some("ui-literal-0007"),
        "are only available to logged-in users. " => Some("ui-literal-0008"),
        "If your browser hasn't launched, " => Some("ui-literal-0009"),
        "and open the page manually." => Some("ui-literal-0010"),
        " and open" => Some("ui-literal-0011"),
        "the page manually." => Some("ui-literal-0012"),
        "Cancel" => Some("ui-literal-0013"),
        "Delete" => Some("ui-literal-0014"),
        " + Add new repo" => Some("ui-literal-0015"),
        "Editing" => Some("ui-literal-0016"),
        "Viewing" => Some("ui-literal-0017"),
        "Auto reload" => Some("ui-literal-0018"),
        "Manage" => Some("ui-literal-0019"),
        "Show prompt" => Some("ui-literal-0020"),
        "Redact secrets (API keys, passwords, IP addresses, PII etc.)" => Some("ui-literal-0021"),
        "Loading session..." => Some("ui-literal-0022"),
        "Add repo" => Some("ui-literal-0023"),
        "Create environment" => Some("ui-literal-0024"),
        "Remove endpoint" => Some("ui-literal-0025"),
        "Copyright 2026 Warp" => Some("ui-literal-0026"),
        "Deleting..." => Some("ui-literal-0027"),
        "You don't have any shared blocks yet." => Some("ui-literal-0028"),
        "Getting blocks..." => Some("ui-literal-0029"),
        "Failed to load blocks. Please try again." => Some("ui-literal-0030"),
        "Unshare block" => Some("ui-literal-0031"),
        "Shared blocks" => Some("ui-literal-0032"),
        "Create" => Some("ui-literal-0033"),
        "Delete environment" => Some("ui-literal-0034"),
        "Share with team" => Some("ui-literal-0035"),
        "Remove" => Some("ui-literal-0036"),
        "Not now" => Some("ui-literal-0037"),
        "Change default model" => Some("ui-literal-0038"),
        "Edit" => Some("ui-literal-0039"),
        "MODELS" => Some("ui-literal-0040"),
        "PERMISSIONS" => Some("ui-literal-0041"),
        "Open file" => Some("ui-literal-0042"),
        "Session type" => Some("ui-literal-0043"),
        "Get Warping" => Some("ui-literal-0044"),
        "Open Tab" => Some("ui-literal-0045"),
        "Same line prompt" => Some("ui-literal-0046"),
        "Separator" => Some("ui-literal-0047"),
        "Save" => Some("ui-literal-0048"),
        "+ Add rule" => Some("ui-literal-0049"),
        "Continue locally" => Some("ui-literal-0050"),
        "View in Oz" => Some("ui-literal-0051"),
        "Update Agent" => Some("ui-literal-0052"),
        "Restore" => Some("ui-literal-0053"),
        "Install nvm" => Some("ui-literal-0054"),
        "Complete!" => Some("ui-literal-0055"),
        "No matches found." => Some("ui-literal-0056"),
        "Or" => Some("ui-literal-0057"),
        "Trash" => Some("ui-literal-0058"),
        "Theme name" => Some("ui-literal-0059"),
        "Background color" => Some("ui-literal-0060"),
        "No matching themes!" => Some("ui-literal-0061"),
        "Commit and create PR" => Some("ui-literal-0062"),
        "Import" => Some("ui-literal-0063"),
        "OpenAI base URL (optional, press Enter to skip):" => Some("ui-literal-0064"),
        "Visit Oz" => Some("ui-literal-0065"),
        "Suggested" => Some("ui-literal-0066"),
        "Edit rule" => Some("ui-literal-0067"),
        "Delete profile" => Some("ui-literal-0068"),
        "New environment" => Some("ui-literal-0069"),
        "Don't show me suggested code banners again" => Some("ui-literal-0070"),
        "(no arguments)" => Some("ui-literal-0071"),
        "Always allow" => Some("ui-literal-0072"),
        " · Check now" => Some("ui-literal-0073"),
        "Mark all as read" => Some("ui-literal-0074"),
        "No results found." => Some("ui-literal-0075"),
        "Current" => Some("ui-literal-0076"),
        "Not visible to other users" => Some("ui-literal-0077"),
        "Add" => Some("ui-literal-0078"),
        "Update" => Some("ui-literal-0079"),
        "Usage" => Some("ui-literal-0080"),
        "Don't ask me this again" => Some("ui-literal-0081"),
        "Free credits" => Some("ui-literal-0082"),
        "Configure" => Some("ui-literal-0083"),
        "Manage defaults" => Some("ui-literal-0084"),
        "Default" => Some("ui-literal-0085"),
        "New" => Some("ui-literal-0086"),
        "No tabs open" => Some("ui-literal-0087"),
        "New conversation" => Some("ui-literal-0088"),
        "Search" => Some("ui-literal-0089"),
        "Set by Team Workspace" => Some("ui-literal-0090"),
        "Opening your browser…" => Some("ui-literal-0091"),
        "Press Ctrl-C to exit." => Some("ui-literal-0092"),
        "executed a tool call" => Some("ui-literal-0093"),
        "No images" => Some("ui-literal-0094"),
        "+ Add router" => Some("ui-literal-0095"),
        "Add Profile" => Some("ui-literal-0096"),
        "+ Add custom model" => Some("ui-literal-0097"),
        "Connect" => Some("ui-literal-0098"),
        "Connecting" => Some("ui-literal-0099"),
        "Disconnect" => Some("ui-literal-0100"),
        "Refresh" => Some("ui-literal-0101"),
        "Your organization disallows AI when the active pane contains content from a remote session" => Some("ui-literal-0102"),
        "Use your" => Some("ui-literal-0103"),
        "Toolbar layout" => Some("ui-literal-0104"),
        "Show model picker in prompt" => Some("ui-literal-0105"),
        "Commands that enable the toolbar" => Some("ui-literal-0106"),
        "Load more" => Some("ui-literal-0107"),
        "Plan" => Some("ui-literal-0108"),
        "Balance" => Some("ui-literal-0109"),
        "Buy credits" => Some("ui-literal-0110"),
        "Purchased this month" => Some("ui-literal-0111"),
        "Auto-reload" => Some("ui-literal-0112"),
        "Last 30 days" => Some("ui-literal-0113"),
        "No usage history" => Some("ui-literal-0114"),
        "Commit" => Some("ui-literal-0115"),
        "Undo" => Some("ui-literal-0116"),
        "Discard changes" => Some("ui-literal-0117"),
        "Initialize codebase" => Some("ui-literal-0118"),
        "Open repository" => Some("ui-literal-0119"),
        "No open changes" => Some("ui-literal-0120"),
        "Stash changes" => Some("ui-literal-0121"),
        "File explorer" => Some("ui-literal-0122"),
        "Rich Input" => Some("ui-literal-0123"),
        "Enable notifications" => Some("ui-literal-0124"),
        "Notifications setup instructions" => Some("ui-literal-0125"),
        "Update Warp plugin" => Some("ui-literal-0126"),
        "Plugin update instructions" => Some("ui-literal-0127"),
        "/remote-control" => Some("ui-literal-0128"),
        "Stop sharing" => Some("ui-literal-0129"),
        "Index new folder" => Some("ui-literal-0130"),
        "Initialized / indexed folders" => Some("ui-literal-0131"),
        "INDEXING" => Some("ui-literal-0132"),
        "LSP SERVERS" => Some("ui-literal-0133"),
        "Open in code review" => Some("ui-literal-0134"),
        "Manage rules" => Some("ui-literal-0135"),
        "Review changes" => Some("ui-literal-0136"),
        "Open all in code review" => Some("ui-literal-0137"),
        "Dismiss" => Some("ui-literal-0138"),
        "Don't show again" => Some("ui-literal-0139"),
        "Rewind" => Some("ui-literal-0140"),
        "One-time purchase" => Some("ui-literal-0141"),
        "All" => Some("ui-literal-0142"),
        "Personal" => Some("ui-literal-0143"),
        "View Agents" => Some("ui-literal-0144"),
        "Clear filters" => Some("ui-literal-0145"),
        "Clear all" => Some("ui-literal-0146"),
        "Delete rule" => Some("ui-literal-0147"),
        "Name" => Some("ui-literal-0148"),
        "Rule" => Some("ui-literal-0149"),
        "Loading..." => Some("ui-literal-0150"),
        "Looks like you're out of credits. " => Some("ui-literal-0151"),
        " for more credits." => Some("ui-literal-0152"),
        "Type" => Some("ui-literal-0153"),
        "Agent" => Some("ui-literal-0154"),
        "Expiration" => Some("ui-literal-0155"),
        "Edit Variables" => Some("ui-literal-0156"),
        "Delete MCP" => Some("ui-literal-0157"),
        "Remove from team" => Some("ui-literal-0158"),
        "See what's new" => Some("ui-literal-0159"),
        "Next" => Some("ui-literal-0160"),
        "Finish" => Some("ui-literal-0161"),
        "Continue" => Some("ui-literal-0162"),
        "Open in Warp" => Some("ui-literal-0163"),
        "Warpify subshell" => Some("ui-literal-0164"),
        "Use agent" => Some("ui-literal-0165"),
        "Comment" => Some("ui-literal-0166"),
        "Previous" => Some("ui-literal-0167"),
        "Hunk:" => Some("ui-literal-0168"),
        "Enable" => Some("ui-literal-0169"),
        "Enable auto-handoff?" => Some("ui-literal-0170"),
        "Visit the repo" => Some("ui-literal-0171"),
        "Warp is now open-source" => Some("ui-literal-0172"),
        "Learn more" => Some("ui-literal-0173"),
        "Close" => Some("ui-literal-0174"),
        " to paste your token from the browser." => Some("ui-literal-0175"),
        "Add-on credits" => Some("ui-literal-0176"),
        "Monthly spend limit" => Some("ui-literal-0177"),
        "Add custom model" => Some("ui-literal-0178"),
        "Add router" => Some("ui-literal-0179"),
        "Open in new pane" => Some("ui-literal-0180"),
        "Open in new tab" => Some("ui-literal-0181"),
        "New file" => Some("ui-literal-0182"),
        "Rename" => Some("ui-literal-0183"),
        "Who has access" => Some("ui-literal-0184"),
        "Share session QR code" => Some("ui-literal-0185"),
        "Refresh AWS Credentials" => Some("ui-literal-0186"),
        "Remove queued prompt" => Some("ui-literal-0187"),
        "Send now" => Some("ui-literal-0188"),
        "New tab" => Some("ui-literal-0189"),
        "Split pane" => Some("ui-literal-0190"),
        "Environment variables" => Some("ui-literal-0191"),
        "Select all" => Some("ui-literal-0192"),
        "Replace all" => Some("ui-literal-0193"),
        "View all cloud runs" => Some("ui-literal-0194"),
        "Use latest codex model" => Some("ui-literal-0195"),
        "Code review" => Some("ui-literal-0196"),
        "Shortcut" => Some("ui-literal-0197"),
        "Notifications" => Some("ui-literal-0198"),
        "No notifications" => Some("ui-literal-0199"),
        "Open conversation" => Some("ui-literal-0200"),
        "Add a title" => Some("ui-literal-0201"),
        "Add a description" => Some("ui-literal-0202"),
        "Untitled" => Some("ui-literal-0203"),
        "Copy link" => Some("ui-literal-0204"),
        "Open on Desktop" => Some("ui-literal-0205"),
        "Cut" => Some("ui-literal-0206"),
        "Copy" => Some("ui-literal-0207"),
        "Paste" => Some("ui-literal-0208"),
        "Split pane right" => Some("ui-literal-0209"),
        "Split pane left" => Some("ui-literal-0210"),
        "1 Comment" => Some("ui-literal-0211"),
        "Send to Agent" => Some("ui-literal-0212"),
        "CLI agent" => Some("ui-literal-0213"),
        "Untitled workflow" => Some("ui-literal-0214"),
        "Copy workflow text" => Some("ui-literal-0215"),
        "TRASH" => Some("ui-literal-0216"),
        "Open in GitHub" => Some("ui-literal-0217"),
        "Maximize" => Some("ui-literal-0218"),
        "Navigate to a repo and initialize it for coding" => Some("ui-literal-0219"),
        "Cloud agent failed to start" => Some("ui-literal-0220"),
        "Continue this cloud conversation" => Some("ui-literal-0221"),
        "Install Warp Plugin for Codex" => Some("ui-literal-0222"),
        "Run the following commands, then restart Codex." => Some("ui-literal-0223"),
        "Add the Warp plugin marketplace repository" => Some("ui-literal-0224"),
        "Install the Warp plugin" => Some("ui-literal-0225"),
        "Install Warp Plugin for Claude Code" => Some("ui-literal-0226"),
        "Ensure that jq is installed on your machine. Then, run these commands." => Some("ui-literal-0227"),
        "Install Warp Plugin for OpenCode" => Some("ui-literal-0228"),
        "Update Warp Plugin for OpenCode" => Some("ui-literal-0229"),
        "Install Warp Plugin for Gemini CLI" => Some("ui-literal-0230"),
        "Run the following command, then restart Gemini CLI." => Some("ui-literal-0231"),
        "Install the Warp extension" => Some("ui-literal-0232"),
        "Update Warp Plugin for Gemini CLI" => Some("ui-literal-0233"),
        "Voice input" => Some("ui-literal-0234"),
        "Attach file" => Some("ui-literal-0235"),
        "Hand off to cloud (or type & )" => Some("ui-literal-0236"),
        "Hand off to cloud (or type &)" => Some("ui-literal-0237"),
        "(myenv)" => Some("ui-literal-0238"),
        " ~/myproject" => Some("ui-literal-0239"),
        " git:(" => Some("ui-literal-0240"),
        "main" => Some("ui-literal-0241"),
        "for terminal" => Some("ui-literal-0242"),
        "Initialize Project" => Some("ui-literal-0243"),
        "Edit Profile" => Some("ui-literal-0244"),
        "Fill out the arguments in this workflow and copy it to run in your terminal session" => Some("ui-literal-0245"),
        " and open the page manually. " => Some("ui-literal-0246"),
        "If you'd like to opt out of analytics, you can adjust your " => Some("ui-literal-0247"),
        "My Label" => Some("ui-literal-0248"),
        "Your app is out of date and some features may not work as expected. Please update immediately." => Some("ui-literal-0249"),
        "Some Warp features may not work as expected without updating immediately, but Warp is unable to perform the update." => Some("ui-literal-0250"),
        "Access your tab configs here." => Some("ui-literal-0251"),
        "Untitled pane" => Some("ui-literal-0252"),
        "Open settings file" => Some("ui-literal-0253"),
        "Rename pane" => Some("ui-literal-0254"),
        "Create Environment" => Some("ui-literal-0255"),
        "Screen edge to pin the hotkey window to." => Some("ui-literal-0256"),
        "Start a new conversation" => Some("ui-literal-0257"),
        "Start a new cloud agent conversation" => Some("ui-literal-0258"),
        "Add a new MCP server via the MCP settings page" => Some("ui-literal-0259"),
        "Pull GitHub PR review comments" => Some("ui-literal-0260"),
        "Create an Oz environment (Docker image + repos) via guided setup" => Some("ui-literal-0261"),
        "Create a new docker sandbox terminal session" => Some("ui-literal-0262"),
        "Have Oz walk you through creating a new coding project" => Some("ui-literal-0263"),
        "Open a skill's markdown file in Warp's built-in editor" => Some("ui-literal-0264"),
        "Invoke a skill" => Some("ui-literal-0265"),
        "Add new Agent prompt" => Some("ui-literal-0266"),
        "Add a new global rule for the agent" => Some("ui-literal-0267"),
        "Open a file in Warp's code editor" => Some("ui-literal-0268"),
        "`/` to open the slash-command menu and access quick agent actions." => Some("ui-literal-0270"),
        "<keybinding> to toggle natural language detection and switch between agent and terminal input." => Some("ui-literal-0271"),
        "`/plan` <prompt> to create a plan for the agent before executing." => Some("ui-literal-0272"),
        "<keybinding> to open the Command Palette and access Warp actions and shortcuts." => Some("ui-literal-0273"),
        "Store reusable workflows, notebooks, and prompts in your" => Some("ui-literal-0274"),
        "Enter a new prompt to redirect the agent while it's running." => Some("ui-literal-0275"),
        "`@` to add context from files, blocks, or Warp Drive objects to your prompt." => Some("ui-literal-0276"),
        "<keybinding> to attach the prior command output as agent context." => Some("ui-literal-0277"),
        "`/init` to index the repo so the agent can understand your codebase." => Some("ui-literal-0278"),
        "Add agent profiles to customize permissions and models per session." => Some("ui-literal-0279"),
        "Right-click a block to fork the conversation from that point." => Some("ui-literal-0280"),
        "Team name" => Some("ui-literal-0281"),
        "When you create a team, you can collaborate on agent-driven development by sharing cloud agent runs, environments, automations, and artifacts. You can also create a shared knowledge store for teammates and agents alike." => Some("ui-literal-0282"),
        "Leave team" => Some("ui-literal-0283"),
        "Domains, comma separated" => Some("ui-literal-0284"),
        "Emails, comma separated" => Some("ui-literal-0285"),
        "Set" => Some("ui-literal-0286"),
        "Invite" => Some("ui-literal-0287"),
        "As an admin, you can choose whether to enable or disable the ability for team members to invite others by invitation link." => Some("ui-literal-0288"),
        "Email invitations are valid for 7 days." => Some("ui-literal-0289"),
        "You are offline." => Some("ui-literal-0290"),
        "Error leaving team" => Some("ui-literal-0291"),
        "Cancel invite" => Some("ui-literal-0292"),
        "Let AI suggest the next command to run based on your command history, outputs, and common workflows." => Some("ui-literal-0293"),
        "Let AI suggest natural language prompts, as inline banners in the input, based on recent commands and their outputs." => Some("ui-literal-0294"),
        "Let AI suggest code diffs and queries as inline banners in the blocklist, based on recent commands and their outputs." => Some("ui-literal-0295"),
        "Let AI generate a title for your shared block based on the command and output." => Some("ui-literal-0296"),
        "Let AI generate commit messages and pull request titles and descriptions." => Some("ui-literal-0297"),
        "Show agent tips" => Some("ui-literal-0298"),
        "Hide agent tips" => Some("ui-literal-0299"),
        "Select MCP servers" => Some("ui-literal-0300"),
        "Add custom endpoint" => Some("ui-literal-0301"),
        "Edit custom endpoint" => Some("ui-literal-0302"),
        "View changes" => Some("ui-literal-0303"),
        "Diffs only work for local workspaces." => Some("ui-literal-0304"),
        "Diffs only work for git repositories." => Some("ui-literal-0305"),
        "Diffs don't currently work in WSL." => Some("ui-literal-0306"),
        "Add diff set as context" => Some("ui-literal-0307"),
        "Add comment" => Some("ui-literal-0308"),
        "Enables codebase indexing and WARP.md" => Some("ui-literal-0309"),
        "Add file diff as context" => Some("ui-literal-0310"),
        "Revert file" => Some("ui-literal-0311"),
        "Open all review comments" => Some("ui-literal-0312"),
        "Agent waiting for instructions..." => Some("ui-literal-0313"),
        "I'm sorry, I couldn't complete that request." => Some("ui-literal-0314"),
        "Internal Warp error." => Some("ui-literal-0315"),
        "Warping..." => Some("ui-literal-0316"),
        "Adjusting tasks..." => Some("ui-literal-0317"),
        "Generating fix..." => Some("ui-literal-0318"),
        "Creating diff..." => Some("ui-literal-0319"),
        "Preparing question..." => Some("ui-literal-0320"),
        "Generating plan..." => Some("ui-literal-0321"),
        "Updating plan..." => Some("ui-literal-0322"),
        "Summarizing conversation..." => Some("ui-literal-0323"),
        "Searching codebase..." => Some("ui-literal-0324"),
        "Configuring agents..." => Some("ui-literal-0325"),
        "Conversation summarized" => Some("ui-literal-0326"),
        "this conversation" => Some("ui-literal-0327"),
        "won't count towards usage" => Some("ui-literal-0328"),
        "View screenshot" => Some("ui-literal-0330"),
        "View details on overage usage" => Some("ui-literal-0331"),
        "Enable premium model usage overages" => Some("ui-literal-0332"),
        "Premium model usage overages are enabled" => Some("ui-literal-0333"),
        "Premium model usage overages are not enabled" => Some("ui-literal-0334"),
        "Continue using premium models beyond your plan's limits. Usage is charged in $20 increments up to your spending limit, with any remaining balance carried over." => Some("ui-literal-0335"),
        "A to Z" => Some("ui-literal-0336"),
        "Z to A" => Some("ui-literal-0337"),
        "Usage ascending" => Some("ui-literal-0338"),
        "Usage descending" => Some("ui-literal-0339"),
        "Overview" => Some("ui-literal-0340"),
        "Usage History" => Some("ui-literal-0341"),
        "Usage reporting is currently limited" => Some("ui-literal-0342"),
        "Compare plans" => Some("ui-literal-0343"),
        "Manage billing" => Some("ui-literal-0344"),
        "Enable premium overages" => Some("ui-literal-0345"),
        "Increase monthly spend limit" => Some("ui-literal-0346"),
        "Upgrade" => Some("ui-literal-0347"),
        "Contact support" => Some("ui-literal-0348"),
        ", contact a team admin" => Some("ui-literal-0349"),
        "Share commands & knowledge with your teammates." => Some("ui-literal-0350"),
        "Folder" => Some("ui-literal-0351"),
        "Notebook" => Some("ui-literal-0352"),
        "Workflow" => Some("ui-literal-0353"),
        "Prompt" => Some("ui-literal-0354"),
        "New folder" => Some("ui-literal-0355"),
        "New notebook" => Some("ui-literal-0356"),
        "New workflow" => Some("ui-literal-0357"),
        "New prompt" => Some("ui-literal-0358"),
        "New environment variables" => Some("ui-literal-0359"),
        "You are offline. Some files will be read only." => Some("ui-literal-0360"),
        "Description" => Some("ui-literal-0361"),
        "Default value (optional)" => Some("ui-literal-0362"),
        "Save workflow" => Some("ui-literal-0363"),
        "Autofill" => Some("ui-literal-0364"),
        "Loading" => Some("ui-literal-0365"),
        "You have unsaved changes." => Some("ui-literal-0366"),
        "Keep editing" => Some("ui-literal-0367"),
        "add argument" => Some("ui-literal-0368"),
        "Generate a title, descriptions, or parameters with Warp AI" => Some("ui-literal-0369"),
        "Tell the agent what to build..." => Some("ui-literal-0370"),
        "Kick off a cloud agent" => Some("ui-literal-0371"),
        "Command Input." => Some("ui-literal-0372"),
        "Input your shell command, press enter to execute. Press cmd-up to navigate to output of previously executed commands. Press cmd-l to re-focus input." => Some("ui-literal-0373"),
        "Type '#' for AI command suggestions" => Some("ui-literal-0374"),
        "Run commands" => Some("ui-literal-0375"),
        "Steer the running agent" => Some("ui-literal-0376"),
        "Queue a follow up for the running agent" => Some("ui-literal-0377"),
        "Ask a follow up" => Some("ui-literal-0378"),
        "Run the following command to generate variants:" => Some("ui-literal-0379"),
        "Run command" => Some("ui-literal-0380"),
        "Code" => Some("ui-literal-0381"),
        "Initialization Settings" => Some("ui-literal-0382"),
        "Codebase indexing" => Some("ui-literal-0383"),
        "Warp can automatically index code repositories as you navigate them, helping agents quickly understand context and provide solutions. Code indexing runs locally and respects your .gitignore and .warpindexingignore files." => Some("ui-literal-0384"),
        "To exclude specific files or directories from indexing, add them to the .warpindexingignore file in your repository directory. These files will not be indexed or used as codebase context." => Some("ui-literal-0385"),
        "Index new folders by default" => Some("ui-literal-0386"),
        "When set to true, Warp will automatically index code repositories as you navigate them - helping agents quickly understand context and provide solutions." => Some("ui-literal-0387"),
        "Team admins have disabled codebase indexing." => Some("ui-literal-0388"),
        "Team admins have enabled codebase indexing." => Some("ui-literal-0389"),
        "You have reached the maximum number of codebase indices for your plan. Delete existing indices to auto-index new codebases." => Some("ui-literal-0390"),
        "Open project rules" => Some("ui-literal-0391"),
        "Invite a friend to Warp" => Some("ui-literal-0392"),
        "Sign up to participate in Warp's referral program" => Some("ui-literal-0393"),
        "Failed to load referral code." => Some("ui-literal-0394"),
        "Send" => Some("ui-literal-0395"),
        "Sending..." => Some("ui-literal-0396"),
        "Link copied." => Some("ui-literal-0397"),
        "Successfully sent emails." => Some("ui-literal-0398"),
        "Failed to send emails. Please try again." => Some("ui-literal-0399"),
        "Get exclusive Warp goodies when you refer someone*" => Some("ui-literal-0400"),
        "Current referral" => Some("ui-literal-0401"),
        "Current referrals" => Some("ui-literal-0402"),
        "Certain restrictions apply." => Some("ui-literal-0403"),
        "Secret redaction" => Some("ui-literal-0404"),
        "Custom secret redaction" => Some("ui-literal-0405"),
        "Help improve Warp" => Some("ui-literal-0406"),
        "Manage your data" => Some("ui-literal-0407"),
        "Visit the data management page" => Some("ui-literal-0408"),
        "Privacy policy" => Some("ui-literal-0409"),
        "Read Warp's privacy policy" => Some("ui-literal-0410"),
        "Recommended" => Some("ui-literal-0411"),
        "Add all" => Some("ui-literal-0412"),
        "Reject" => Some("ui-literal-0413"),
        "Run" => Some("ui-literal-0414"),
        "Done" => Some("ui-literal-0415"),
        "Generating command..." => Some("ui-literal-0416"),
        "OK if I run this command and read the output?" => Some("ui-literal-0417"),
        "OK if I call this MCP tool?" => Some("ui-literal-0418"),
        "Agent is monitoring command..." => Some("ui-literal-0419"),
        "Agent needs your input to continue" => Some("ui-literal-0420"),
        "User is in control." => Some("ui-literal-0421"),
        "Paused agent. User is in control." => Some("ui-literal-0422"),
        "User in control" => Some("ui-literal-0423"),
        "Agent ran into an issue. Take over control." => Some("ui-literal-0424"),
        "Refine" => Some("ui-literal-0425"),
        "Accept" => Some("ui-literal-0426"),
        "Auto-approve" => Some("ui-literal-0427"),
        "Accept and continue with agent" => Some("ui-literal-0428"),
        "Iterate with agent" => Some("ui-literal-0429"),
        "Expand" => Some("ui-literal-0430"),
        "Collapse" => Some("ui-literal-0431"),
        "Vertical tabs" => Some("ui-literal-0432"),
        "Rich tab titles and metadata like git branch, worktree, and PR. Fully customizable." => Some("ui-literal-0433"),
        "Tab configs" => Some("ui-literal-0434"),
        "Tab-level schema to set your directory, startup commands, theme, and worktree with one click" => Some("ui-literal-0435"),
        "Agent inbox" => Some("ui-literal-0436"),
        "Notifications when any agent needs your attention, also accessible in a central inbox" => Some("ui-literal-0437"),
        "Native code review" => Some("ui-literal-0438"),
        "Send inline comments from Warp's code review directly to Claude Code, Codex, or OpenCode" => Some("ui-literal-0439"),
        "Skip" => Some("ui-literal-0440"),
        "Back to terminal" => Some("ui-literal-0441"),
        "Meet the Warp input" => Some("ui-literal-0442"),
        "Talk to the agent" => Some("ui-literal-0443"),
        "Welcome to terminal mode" => Some("ui-literal-0444"),
        "You’re in terminal mode" => Some("ui-literal-0445"),
        "Enable Natural Language Detection" => Some("ui-literal-0446"),
        "You're in agent mode" => Some("ui-literal-0447"),
        "Tab styling" => Some("ui-literal-0448"),
        "Vertical" => Some("ui-literal-0449"),
        "Horizontal" => Some("ui-literal-0450"),
        "Conversation history" => Some("ui-literal-0451"),
        "Global file search" => Some("ui-literal-0452"),
        "Tools panel" => Some("ui-literal-0453"),
        "Enabled" => Some("ui-literal-0454"),
        "Disabled" => Some("ui-literal-0455"),
        "Back" => Some("ui-literal-0456"),
        "It looks like you logged into a Warp account through a web browser. If you continue, any personal Warp drive objects and preferences from this device will be replaced with data from that account." => Some("ui-literal-0457"),
        "This cannot be undone." => Some("ui-literal-0458"),
        "New login detected" => Some("ui-literal-0459"),
        "Export your data" => Some("ui-literal-0460"),
        " to import later." => Some("ui-literal-0461"),
        "Are you sure you want to leave this team?" => Some("ui-literal-0462"),
        "Are you sure you want to remove this member?" => Some("ui-literal-0463"),
        "You will need to be reinvited in order to rejoin." => Some("ui-literal-0464"),
        "Yes, leave" => Some("ui-literal-0465"),
        "Leave Team" => Some("ui-literal-0466"),
        "Remove Member" => Some("ui-literal-0467"),
        "Choose an AI execution profile" => Some("ui-literal-0468"),
        "Choose an agent model" => Some("ui-literal-0469"),
        "Follow-ups use the original run's model" => Some("ui-literal-0470"),
        "Request edit access to change model" => Some("ui-literal-0471"),
        "Manage API keys" => Some("ui-literal-0472"),
        "Cost" => Some("ui-literal-0473"),
        "Resource not found or access denied" => Some("ui-literal-0474"),
        "Use your custom prompt" => Some("ui-literal-0475"),
        "Integrate Warp with your IDE" => Some("ui-literal-0476"),
        "How Warp uses Warp" => Some("ui-literal-0477"),
        "First Tab" => Some("ui-literal-0478"),
        "Second Tab" => Some("ui-literal-0479"),
        "Yes, log out" => Some("ui-literal-0480"),
        "Show running processes" => Some("ui-literal-0481"),
        "Sharing a session" => Some("ui-literal-0482"),
        "Handed session off to the cloud" => Some("ui-literal-0483"),
        "Show in file explorer" => Some("ui-literal-0484"),
        "Looks like you're out of AI credits." => Some("ui-literal-0485"),
        "Upgrade for more credits." => Some("ui-literal-0486"),
        "Failed to load conversation." => Some("ui-literal-0487"),
        "Conversation forking failed." => Some("ui-literal-0488"),
        "Troubleshoot notifications" => Some("ui-literal-0489"),
        "View changelog" => Some("ui-literal-0490"),
        "Cannot open a new terminal session" => Some("ui-literal-0491"),
        "View" => Some("ui-literal-0492"),
        "Linear Issue" => Some("ui-literal-0493"),
        "Disabled all synchronized inputs." => Some("ui-literal-0494"),
        "Conversation deleted" => Some("ui-literal-0495"),
        "Failed to load file." => Some("ui-literal-0496"),
        "Failed to save file." => Some("ui-literal-0497"),
        "Cannot save — remote session disconnected." => Some("ui-literal-0498"),
        "File saved." => Some("ui-literal-0499"),
        "Open" => Some("ui-literal-0500"),
        "Enable auto reload?" => Some("ui-literal-0501"),
        "Slash commands" => Some("ui-literal-0502"),
        "Notification" => Some("ui-literal-0503"),
        "Project setup" => Some("ui-literal-0504"),
        "PowerShell subshells not supported" => Some("ui-literal-0505"),
        "New API key" => Some("ui-literal-0506"),
        "Save your key" => Some("ui-literal-0507"),
        "Selected repos" => Some("ui-literal-0508"),
        "Available indexed repos" => Some("ui-literal-0509"),
        "e.g., dev-env" => Some("ui-literal-0510"),
        "e.g., node:20-alpine" => Some("ui-literal-0511"),
        "e.g., node start" => Some("ui-literal-0512"),
        "Environment name" => Some("ui-literal-0513"),
        "e.g. python:3.11, node:20-alpine" => Some("ui-literal-0514"),
        "e.g. cd my-repo && pip install -r requirements.txt" => Some("ui-literal-0515"),
        "Warp will suggest a Docker image based on your selected repositories." => Some("ui-literal-0516"),
        "Copy URL" => Some("ui-literal-0517"),
        "SuperGrok subscription connected" => Some("ui-literal-0518"),
        "Quick setup" => Some("ui-literal-0519"),
        "Use the agent" => Some("ui-literal-0520"),
        "Subshells supported: bash, zsh, and fish." => Some("ui-literal-0521"),
        "Warpify your interactive SSH sessions." => Some("ui-literal-0522"),
        "Add regex pattern" => Some("ui-literal-0523"),
        "Don't Save" => Some("ui-literal-0524"),
        "Cloud agent run" => Some("ui-literal-0525"),
        "Fork this conversation locally" => Some("ui-literal-0526"),
        "View this run in the Oz web app" => Some("ui-literal-0527"),
        "Show version history" => Some("ui-literal-0528"),
        "Copied to clipboard as Markdown" => Some("ui-literal-0529"),
        "Link copied to clipboard" => Some("ui-literal-0530"),
        "Plan ID copied to clipboard" => Some("ui-literal-0531"),
        "Finished exporting objects" => Some("ui-literal-0532"),
        "Custom URI is invalid." => Some("ui-literal-0533"),
        "New tab created" => Some("ui-literal-0534"),
        "Open a new terminal session in this directory" => Some("ui-literal-0535"),
        "Copy file path" => Some("ui-literal-0536"),
        "No git actions available" => Some("ui-literal-0537"),
        "Push commits to remote" => Some("ui-literal-0538"),
        "Create a pull request" => Some("ui-literal-0539"),
        "Publish branch to remote" => Some("ui-literal-0540"),
        "Open PR" => Some("ui-literal-0541"),
        "Unable to download QR code." => Some("ui-literal-0542"),
        "QR code downloaded." => Some("ui-literal-0543"),
        "Edit inherited permissions on the parent folder" => Some("ui-literal-0544"),
        "Cannot edit inherited permissions" => Some("ui-literal-0545"),
        "Untitled Plan" => Some("ui-literal-0546"),
        "Agent task" => Some("ui-literal-0547"),
        "Deleted conversation" => Some("ui-literal-0548"),
        "New Oz cloud agent conversation" => Some("ui-literal-0549"),
        "New Oz agent conversation" => Some("ui-literal-0550"),
        "Requested Edit" => Some("ui-literal-0551"),
        "Copied to clipboard" => Some("ui-literal-0552"),
        "Open file explorer" => Some("ui-literal-0553"),
        "Open Rich Input" => Some("ui-literal-0554"),
        "Open coding agent settings" => Some("ui-literal-0555"),
        "View instructions to install the Warp plugin" => Some("ui-literal-0556"),
        "A new version of the Warp plugin is available" => Some("ui-literal-0557"),
        "View instructions to update the Warp plugin" => Some("ui-literal-0558"),
        "Hide Rich Input" => Some("ui-literal-0559"),
        "Context window usage" => Some("ui-literal-0560"),
        "See logs for details" => Some("ui-literal-0561"),
        "Choose an environment" => Some("ui-literal-0562"),
        "No results found" => Some("ui-literal-0563"),
        "MCP server updated" => Some("ui-literal-0564"),
        "Log out" => Some("ui-literal-0565"),
        "This MCP server contains secrets. Visit Settings > Privacy to modify your secret redaction settings." => Some("ui-literal-0566"),
        "No MCP Server specified." => Some("ui-literal-0567"),
        "Overall usage" => Some("ui-literal-0568"),
        "Local agent usage" => Some("ui-literal-0569"),
        "Cloud agent usage" => Some("ui-literal-0570"),
        "Transfer ownership" => Some("ui-literal-0571"),
        "Demote from admin" => Some("ui-literal-0572"),
        "Promote to admin" => Some("ui-literal-0573"),
        "Your team is full" => Some("ui-literal-0574"),
        "You've exceeded your member limit" => Some("ui-literal-0575"),
        "Payment past due" => Some("ui-literal-0576"),
        "Subscription unpaid" => Some("ui-literal-0577"),
        "You've reached your plan's member limit." => Some("ui-literal-0578"),
        "Upgrade to grow your team." => Some("ui-literal-0579"),
        "Contact support to restore access." => Some("ui-literal-0580"),
        "Other" => Some("ui-literal-0581"),
        "Sign up" => Some("ui-literal-0582"),
        "Suggested Code Banners" => Some("ui-literal-0583"),
        "Shared Block Title Generation" => Some("ui-literal-0584"),
        "Orchestration message display" => Some("ui-literal-0585"),
        "Loading open changes..." => Some("ui-literal-0586"),
        "Error loading diffs" => Some("ui-literal-0587"),
        "Could not submit comments to the agent" => Some("ui-literal-0588"),
        "New empty file" => Some("ui-literal-0589"),
        "Push" => Some("ui-literal-0590"),
        "Selected folder is not a Git repository" => Some("ui-literal-0591"),
        "Docker image" => Some("ui-literal-0592"),
        "Press Enter or click the submit button to add each command." => Some("ui-literal-0593"),
        "Docker image reference" => Some("ui-literal-0594"),
        "Browse GitHub repos..." => Some("ui-literal-0595"),
        "Command pending..." => Some("ui-literal-0596"),
        "Command failed" => Some("ui-literal-0597"),
        "Command returned no results" => Some("ui-literal-0598"),
        "Get Figma MCP" => Some("ui-literal-0599"),
        "Enable Figma MCP" => Some("ui-literal-0600"),
        "Edit Prompt" => Some("ui-literal-0601"),
        "Index folder" => Some("ui-literal-0602"),
        "Restart server" => Some("ui-literal-0603"),
        "View logs" => Some("ui-literal-0604"),
        "Show code review button" => Some("ui-literal-0605"),
        "Show a button in the top right of the window to toggle the code review panel." => Some("ui-literal-0606"),
        "Reading files..." => Some("ui-literal-0607"),
        "Grepping..." => Some("ui-literal-0608"),
        "Finding files..." => Some("ui-literal-0609"),
        "Executing command..." => Some("ui-literal-0610"),
        "Writing command input..." => Some("ui-literal-0611"),
        "Searching the web..." => Some("ui-literal-0612"),
        "Fetching PR comments..." => Some("ui-literal-0613"),
        "Grant access to the following files?" => Some("ui-literal-0614"),
        "Warp lost connection" => Some("ui-literal-0615"),
        "Edit API Keys" => Some("ui-literal-0616"),
        "Enable Warp Notifications for Codex" => Some("ui-literal-0617"),
        "Update Codex to the latest version, then enable in-focus notifications so Warp can display them while you work." => Some("ui-literal-0618"),
        "Update Codex to the latest version." => Some("ui-literal-0619"),
        "Update Warp Plugin for Codex" => Some("ui-literal-0620"),
        "Upgrade the marketplace" => Some("ui-literal-0621"),
        "Run the following commands." => Some("ui-literal-0622"),
        "Remove the existing marketplace (if present)" => Some("ui-literal-0623"),
        "Re-add the marketplace" => Some("ui-literal-0624"),
        "Install the latest plugin version" => Some("ui-literal-0625"),
        "N tabs" => Some("ui-literal-0626"),
        "Untitled tab" => Some("ui-literal-0627"),
        "Pane title as" => Some("ui-literal-0628"),
        "Branch" => Some("ui-literal-0629"),
        "Requires the GitHub CLI to be installed and authenticated" => Some("ui-literal-0630"),
        "Router Editor" => Some("ui-literal-0631"),
        "At least one rule with a description and model is required." => Some("ui-literal-0632"),
        "Models" => Some("ui-literal-0633"),
        "Default model" => Some("ui-literal-0634"),
        "Rules" => Some("ui-literal-0635"),
        "Router name" => Some("ui-literal-0636"),
        "Complexity-based" => Some("ui-literal-0637"),
        "Rule-based" => Some("ui-literal-0638"),
        "Router type" => Some("ui-literal-0639"),
        "Add rule" => Some("ui-literal-0640"),
        "Sort by" => Some("ui-literal-0641"),
        "Retry sync" => Some("ui-literal-0642"),
        "Please contact a team admin to restore access." => Some("ui-literal-0643"),
        "Empty trash" => Some("ui-literal-0644"),
        "Create team" => Some("ui-literal-0645"),
        "don't ask again" => Some("ui-literal-0646"),
        "The first cloud-mode prompt cannot be changed." => Some("ui-literal-0647"),
        "Send to full terminal use agent" => Some("ui-literal-0648"),
        "Read-only viewers cannot send prompts." => Some("ui-literal-0649"),
        "(queued until the command finishes)" => Some("ui-literal-0650"),
        "wait for the cloud agent" => Some("ui-literal-0651"),
        "send now" => Some("ui-literal-0652"),
        "to send" => Some("ui-literal-0653"),
        "Title (optional)" => Some("ui-literal-0654"),
        "Command" => Some("ui-literal-0655"),
        "Output" => Some("ui-literal-0656"),
        "Something went wrong. Please try again." => Some("ui-literal-0657"),
        "Embed code copied." => Some("ui-literal-0658"),
        "Error generating embed snippet" => Some("ui-literal-0659"),
        "Manage shared blocks" => Some("ui-literal-0660"),
        "+ Create API Key" => Some("ui-literal-0661"),
        "Key" => Some("ui-literal-0662"),
        "Scope" => Some("ui-literal-0663"),
        "Created" => Some("ui-literal-0664"),
        "Last used" => Some("ui-literal-0665"),
        "Expires at" => Some("ui-literal-0666"),
        "Set up Warp to honor your PS1 setting" => Some("ui-literal-0667"),
        "View documentation" => Some("ui-literal-0668"),
        "Configure Warp to launch from your most used development tools" => Some("ui-literal-0669"),
        "Learn how Warp's engineering team uses their favorite features" => Some("ui-literal-0670"),
        "Read article" => Some("ui-literal-0671"),
        "Failed to prepare file download." => Some("ui-literal-0672"),
        "View your agent tasks plus all shared team tasks" => Some("ui-literal-0673"),
        "View agent tasks you created" => Some("ui-literal-0674"),
        "Rewind to before this block" => Some("ui-literal-0675"),
        "Revoke all edit permissions" => Some("ui-literal-0676"),
        "Clear upload" => Some("ui-literal-0677"),
        "Open this conversation in the Warp desktop app" => Some("ui-literal-0678"),
        "+ New …" => Some("ui-literal-0679"),
        "GitHub Authentication Required" => Some("ui-literal-0680"),
        "Cloud Agent Run Cancelled" => Some("ui-literal-0681"),
        "No cloud environment was started" => Some("ui-literal-0682"),
        "Disabled by your administrator" => Some("ui-literal-0683"),
        "Enable Warp shell integration in this session" => Some("ui-literal-0684"),
        "Ask the Warp agent to assist" => Some("ui-literal-0685"),
        "Ask the Warp agent to resume" => Some("ui-literal-0686"),
        "Use AWS Bedrock?" => Some("ui-literal-0687"),
        "AWS CLI Not Installed" => Some("ui-literal-0688"),
        "Enable Warp's Vim keybindings?" => Some("ui-literal-0689"),
        "Warp can auto-expand aliases." => Some("ui-literal-0690"),
        "Shell process exited prematurely!" => Some("ui-literal-0691"),
        "Shell process exited" => Some("ui-literal-0692"),
        "Pin the plugin to the latest version in your opencode.json. OpenCode caches plugins per version spec, so changing the pin forces it to re-fetch the plugin." => Some("ui-literal-0693"),
        "Update Warp Plugin for Claude Code" => Some("ui-literal-0694"),
        "Fork conversation" => Some("ui-literal-0695"),
        "Read-only" => Some("ui-literal-0696"),
        ". Sign in to edit" => Some("ui-literal-0697"),
        "Additional metadata" => Some("ui-literal-0698"),
        "Project explorer" => Some("ui-literal-0699"),
        "Global search" => Some("ui-literal-0700"),
        "Agent conversations" => Some("ui-literal-0701"),
        "This conversation cannot be deleted" => Some("ui-literal-0702"),
        "Toggle Case Sensitivity" => Some("ui-literal-0703"),
        "Toggle Regex" => Some("ui-literal-0704"),
        "Contribute" => Some("ui-literal-0705"),
        "Open Automated Development" => Some("ui-literal-0706"),
        "Introducing 'auto (open-weights)'" => Some("ui-literal-0707"),
        "Run any agent harness in the cloud" => Some("ui-literal-0708"),
        "Multi-agent orchestration" => Some("ui-literal-0709"),
        "Agent Memory" => Some("ui-literal-0710"),
        "Are you sure you don't want AI?" => Some("ui-literal-0711"),
        "Auto" => Some("ui-literal-0712"),
        "Claude Sonnet" => Some("ui-literal-0713"),
        "GPT-4o" => Some("ui-literal-0714"),
        "CLI agent toolbar" => Some("ui-literal-0715"),
        "Introducing Oz" => Some("ui-literal-0716"),
        "Warp anything" => Some("ui-literal-0717"),
        "Open Theme Creator Modal" => Some("ui-literal-0718"),
        "Open Save Config Modal" => Some("ui-literal-0719"),
        "Quit Modal Shown" => Some("ui-literal-0720"),
        "Quit Modal Cancel Pressed" => Some("ui-literal-0721"),
        "Quit Modal Disabled" => Some("ui-literal-0722"),
        "Log Out Modal Shown" => Some("ui-literal-0723"),
        "Log Out Modal Cancel Pressed" => Some("ui-literal-0724"),
        "Opened Save As Workflow Modal" => Some("ui-literal-0725"),
        "Shared Session Modal Upgrade Pressed" => Some("ui-literal-0726"),
        "Don't Show Sharer Grant Modal Again" => Some("ui-literal-0727"),
        "Toggle Approvals Modal" => Some("ui-literal-0728"),
        "Toggle SharedBlock Title Generation" => Some("ui-literal-0729"),
        "Opened save launch configuration modal" => Some("ui-literal-0730"),
        "Opened or closed teams modal" => Some("ui-literal-0731"),
        "User opened the guided Create a tab config modal" => Some("ui-literal-0732"),
        "User submitted the guided Create a tab config modal" => Some("ui-literal-0733"),
        "Pin the plugin to the latest version in your opencode.json. OpenCode caches plugins per version spec, so changing the pin forces it to re-fetch on restart." => Some("ui-literal-0734"),
        "Restart OpenCode to activate the plugin." => Some("ui-literal-0735"),
        "Open or create your opencode.json. This can be in your project root, or the global config path:" => Some("ui-literal-0736"),
        "Warp Agent" => Some("ui-literal-0737"),
        "To use AI features, please create an account." => Some("ui-literal-0738"),
        "Restricted due to billing issue" => Some("ui-literal-0739"),
        "Unlimited" => Some("ui-literal-0740"),
        "Credits" => Some("ui-literal-0741"),
        " to get more AI usage." => Some("ui-literal-0742"),
        " for more AI usage." => Some("ui-literal-0743"),
        "Active AI" => Some("ui-literal-0744"),
        "Next Command" => Some("ui-literal-0745"),
        "Prompt Suggestions" => Some("ui-literal-0746"),
        "Natural Language Autosuggestions" => Some("ui-literal-0747"),
        "Commit & Pull Request Generation" => Some("ui-literal-0748"),
        "Let AI suggest natural language autosuggestions, based on recent commands and their outputs." => Some("ui-literal-0749"),
        "Input" => Some("ui-literal-0750"),
        "Show input hint text" => Some("ui-literal-0751"),
        "Include agent-executed commands in history" => Some("ui-literal-0752"),
        "Default prompt submission mode" => Some("ui-literal-0753"),
        "What happens when you submit a new prompt while the agent is still responding. You can override this per conversation using the auto-queue toggle." => Some("ui-literal-0754"),
        "Default long-running command submission mode" => Some("ui-literal-0755"),
        "What happens when you submit a prompt while an agent is driving an agent-requested long-running command. Queued prompts are sent to the agent when the command finishes." => Some("ui-literal-0756"),
        "Encountered an incorrect detection? " => Some("ui-literal-0757"),
        "Let us know" => Some("ui-literal-0758"),
        "Autodetect agent prompts in terminal input" => Some("ui-literal-0759"),
        "Autodetect terminal commands in agent input" => Some("ui-literal-0760"),
        "Enabling natural language detection will detect when natural language is written in the terminal input, and then automatically switch to Agent Mode for AI queries." => Some("ui-literal-0761"),
        " Encountered an incorrect input detection? " => Some("ui-literal-0762"),
        "Natural language detection" => Some("ui-literal-0763"),
        "Natural language denylist" => Some("ui-literal-0764"),
        "Commands listed here will never trigger natural language detection." => Some("ui-literal-0765"),
        "MCP Servers" => Some("ui-literal-0766"),
        "Add MCP servers to extend the Warp Agent's capabilities. MCP servers expose data sources or tools to agents through a standardized interface, essentially acting like plugins. " => Some("ui-literal-0767"),
        "Add MCP servers to extend the Warp Agent's capabilities. MCP servers expose data sources or tools to agents through a standardized interface, essentially acting like plugins. Add a custom server, or use the presets to get started with popular servers. You can also find team servers that have been shared with you here. " => Some("ui-literal-0768"),
        "Manage MCP servers" => Some("ui-literal-0769"),
        "Auto-spawn servers from third-party agents" => Some("ui-literal-0770"),
        "Automatically detect and spawn MCP servers from globally-scoped third-party AI agent configuration files (e.g. in your home directory). Servers detected inside a repository are never spawned automatically and must be enabled individually from the MCP settings page. " => Some("ui-literal-0771"),
        "Automatically detect and spawn MCP servers from globally-scoped third-party AI agent configuration files (e.g. in your home directory). Servers detected inside a repository are never spawned automatically and must be enabled individually in the \"Detected from\" sections below. " => Some("ui-literal-0772"),
        "See supported providers." => Some("ui-literal-0773"),
        "Search MCP Servers" => Some("ui-literal-0774"),
        "Once you add a MCP server, it will be shown here." => Some("ui-literal-0775"),
        "No search results found" => Some("ui-literal-0776"),
        "Available to install" => Some("ui-literal-0777"),
        "My MCPs" => Some("ui-literal-0778"),
        "Shared from Warp" => Some("ui-literal-0779"),
        "Detected from config file" => Some("ui-literal-0780"),
        "Learn more." => Some("ui-literal-0781"),
        "Shared by a team member" => Some("ui-literal-0782"),
        "From another device" => Some("ui-literal-0783"),
        "Offline" => Some("ui-literal-0784"),
        "Starting server..." => Some("ui-literal-0785"),
        "Authenticating..." => Some("ui-literal-0786"),
        "Shutting down..." => Some("ui-literal-0787"),
        "No tools available" => Some("ui-literal-0788"),
        "Show logs" => Some("ui-literal-0789"),
        "Share server" => Some("ui-literal-0790"),
        "Edit config" => Some("ui-literal-0791"),
        "Server update available" => Some("ui-literal-0792"),
        "Set up" => Some("ui-literal-0793"),
        "weekly" => Some("ui-literal-0794"),
        "monthly" => Some("ui-literal-0795"),
        "biweekly" => Some("ui-literal-0796"),
        "Agent decides" => Some("ui-literal-0797"),
        "Always ask" => Some("ui-literal-0798"),
        "Ask on first write" => Some("ui-literal-0799"),
        "Change your default model?" => Some("ui-literal-0800"),
        "New Tab" => Some("ui-literal-0801"),
        "Split Pane" => Some("ui-literal-0802"),
        "Default model updated" => Some("ui-literal-0803"),
        "auto (cost-efficient)" => Some("ui-literal-0804"),
        "no credits" => Some("ui-literal-0805"),
        "Endpoint added" => Some("ui-literal-0806"),
        "Endpoint saved" => Some("ui-literal-0807"),
        "Endpoint removed" => Some("ui-literal-0808"),
        "Paste sign-in code" => Some("ui-literal-0809"),
        "Read only" => Some("ui-literal-0810"),
        "Supervised" => Some("ui-literal-0811"),
        "Allow in specific directories" => Some("ui-literal-0812"),
        "Select coding agent" => Some("ui-literal-0813"),
        "Upgrade AI Usage" => Some("ui-literal-0814"),
        "Unknown reason" => Some("ui-literal-0815"),
        "Set the boundaries for how your Agent operates. Choose what it can access, how much autonomy it has, and when it must ask for your approval. You can also fine-tune behavior around natural language input, codebase awareness, and more." => Some("ui-literal-0816"),
        "Profiles" => Some("ui-literal-0817"),
        "Profiles let you define how your Agent operates — from the actions it can take and when it needs approval, to the models it uses for tasks like coding and planning. You can also scope them to individual projects." => Some("ui-literal-0818"),
        "Context window (tokens)" => Some("ui-literal-0819"),
        "Permissions" => Some("ui-literal-0820"),
        "Apply code diffs" => Some("ui-literal-0821"),
        "Read files" => Some("ui-literal-0822"),
        "Execute commands" => Some("ui-literal-0823"),
        "Some of your permissions are managed by your workspace." => Some("ui-literal-0824"),
        "Interact with running commands" => Some("ui-literal-0825"),
        "Command denylist" => Some("ui-literal-0826"),
        "Regular expressions to match commands that the Warp Agent should always ask permission to execute." => Some("ui-literal-0827"),
        "Command allowlist" => Some("ui-literal-0828"),
        "Regular expressions to match commands that can be automatically executed by the Warp Agent." => Some("ui-literal-0829"),
        "Directory allowlist" => Some("ui-literal-0830"),
        "Give the agent file access to certain directories." => Some("ui-literal-0831"),
        "Base model" => Some("ui-literal-0832"),
        "This model serves as the primary engine behind the Warp Agent. It powers most interactions and invokes other models for tasks like planning or code generation when necessary. Warp may automatically switch to alternate models based on model availability or for auxiliary tasks such as conversation summarization." => Some("ui-literal-0833"),
        "Codebase Context" => Some("ui-literal-0834"),
        "Allow the Warp Agent to generate an outline of your codebase that can be used for context. No code is ever stored on our servers. " => Some("ui-literal-0835"),
        "Call MCP servers" => Some("ui-literal-0836"),
        "You haven't added any MCP servers yet. Once you do, you'll be able to control how much autonomy the Warp Agent has when interacting with them. " => Some("ui-literal-0837"),
        "Add a server" => Some("ui-literal-0838"),
        "learn more about MCPs." => Some("ui-literal-0839"),
        "MCP allowlist" => Some("ui-literal-0840"),
        "Allow the Warp Agent to call these MCP servers." => Some("ui-literal-0841"),
        "MCP denylist" => Some("ui-literal-0842"),
        "The Warp Agent will always ask for permission before calling any MCP servers on this list." => Some("ui-literal-0843"),
        "Rules help the Warp Agent follow your conventions, whether for codebases or specific workflows. " => Some("ui-literal-0844"),
        "Suggested Rules" => Some("ui-literal-0845"),
        "Let AI suggest rules to save based on your interactions." => Some("ui-literal-0846"),
        "Warp Drive as agent context" => Some("ui-literal-0847"),
        "The Warp Agent can leverage your Warp Drive Contents to tailor responses to your personal and team developer workflows and environments. This includes any Workflows, Notebooks, and Environment Variables." => Some("ui-literal-0848"),
        "Knowledge" => Some("ui-literal-0849"),
        "Voice Input" => Some("ui-literal-0850"),
        "Voice input allows you to control Warp by speaking directly to your terminal (powered by " => Some("ui-literal-0851"),
        "Key for Activating Voice Input" => Some("ui-literal-0852"),
        "Press and hold to activate." => Some("ui-literal-0853"),
        "Show Oz changelog in new conversation view" => Some("ui-literal-0854"),
        "Show conversation history in tools panel" => Some("ui-literal-0855"),
        "Agent thinking display" => Some("ui-literal-0856"),
        "Controls how reasoning/thinking traces are displayed." => Some("ui-literal-0857"),
        "Controls whether orchestration messages stay expanded." => Some("ui-literal-0858"),
        "Preferred layout when opening existing agent conversations" => Some("ui-literal-0859"),
        "Show coding agent toolbar" => Some("ui-literal-0860"),
        "Show a toolbar with quick actions when running coding agents like " => Some("ui-literal-0861"),
        "Third party CLI agents" => Some("ui-literal-0862"),
        "Auto show/hide Rich Input based on agent status" => Some("ui-literal-0863"),
        "Requires the Warp plugin for your coding agent" => Some("ui-literal-0864"),
        "Auto open Rich Input when a coding agent session starts" => Some("ui-literal-0865"),
        "Auto dismiss Rich Input after prompt submission" => Some("ui-literal-0866"),
        "Submit Rich Input with Ctrl+Enter" => Some("ui-literal-0867"),
        "Add regex patterns to show the coding agent toolbar for matching commands." => Some("ui-literal-0868"),
        "This option is enforced by your organization's settings and cannot be customized." => Some("ui-literal-0869"),
        "Enable agent attribution" => Some("ui-literal-0870"),
        "Agent Attribution" => Some("ui-literal-0871"),
        "Oz can add attribution to commit messages and pull requests it creates" => Some("ui-literal-0872"),
        "Computer use in Cloud Agents" => Some("ui-literal-0873"),
        "Experimental" => Some("ui-literal-0874"),
        "Enable computer use in cloud agent conversations started from the Warp app." => Some("ui-literal-0875"),
        "Cloud handoff requires cloud conversations to be enabled." => Some("ui-literal-0876"),
        "Cloud handoff" => Some("ui-literal-0877"),
        "Cloud Handoff" => Some("ui-literal-0878"),
        "Hand off local agent conversations to a cloud agent." => Some("ui-literal-0879"),
        "Auto-handoff before sleep" => Some("ui-literal-0880"),
        "When macOS is about to sleep, automatically moves the most recently focused running local Warp Agent conversation to Cloud Mode so it can keep working." => Some("ui-literal-0881"),
        "Use & to trigger handoff" => Some("ui-literal-0882"),
        "Type & as the first character to enter cloud handoff compose mode." => Some("ui-literal-0883"),
        "OpenAI API key" => Some("ui-literal-0884"),
        "Anthropic API key" => Some("ui-literal-0885"),
        "Google API key" => Some("ui-literal-0886"),
        "Use your own API keys from model providers for Warp Agent. You can also add custom endpoints to use third-party models. Custom endpoints must support the OpenAI-compatible Chat Completions API. API keys are stored only on your device, never on Warp's servers. They're used to make requests to your chosen model provider. Using auto models or models from providers you have not provided API keys for will consume Warp credits. " => Some("ui-literal-0887"),
        "By using BYOK or custom endpoints, you agree to use them only as permitted by " => Some("ui-literal-0888"),
        "Warp's Terms of Service" => Some("ui-literal-0889"),
        ". BYOK and custom endpoints are intended for individual use and small teams. Companies or organizations with more than 10 employees should use Warp Business or Enterprise." => Some("ui-literal-0890"),
        "Connect SuperGrok subscription" => Some("ui-literal-0891"),
        "Premium or SuperGrok subscription" => Some("ui-literal-0892"),
        "Connect your SuperGrok subscription to use Grok models in the Warp Agent through your xAI account." => Some("ui-literal-0893"),
        "Connected." => Some("ui-literal-0894"),
        "Warp credit fallback" => Some("ui-literal-0895"),
        "When enabled, agent requests may be routed to one of Warp's provided models in the event of an error. Warp will prioritize using your API keys over your Warp credits." => Some("ui-literal-0896"),
        "Custom inference" => Some("ui-literal-0897"),
        "Custom endpoints" => Some("ui-literal-0898"),
        "Contact sales" => Some("ui-literal-0899"),
        " to enable bringing your own API keys on your Enterprise plan." => Some("ui-literal-0900"),
        "Upgrade to the Build plan" => Some("ui-literal-0901"),
        " to use your own API keys." => Some("ui-literal-0902"),
        "Ask your team's admin to upgrade to the Build plan to use your own API keys." => Some("ui-literal-0903"),
        "Create an account" => Some("ui-literal-0904"),
        "Warp loads and sends local AWS CLI credentials for Bedrock-supported models. This setting is managed by your organization." => Some("ui-literal-0905"),
        "Warp loads and sends local AWS CLI credentials for Bedrock-supported models." => Some("ui-literal-0906"),
        "Use AWS Bedrock credentials" => Some("ui-literal-0907"),
        "Login Command" => Some("ui-literal-0908"),
        "AWS Profile" => Some("ui-literal-0909"),
        "Automatically run login command" => Some("ui-literal-0910"),
        "When enabled, the login command will run automatically when AWS Bedrock credentials expire." => Some("ui-literal-0911"),
        "Custom Model Routers" => Some("ui-literal-0912"),
        "Custom Routers" => Some("ui-literal-0913"),
        "Automatically route tasks to specific models based on task complexity or custom rules. Custom routers will appear in your model selector menu." => Some("ui-literal-0914"),
        "Interface" => Some("ui-literal-0915"),
        "Themes" => Some("ui-literal-0916"),
        "Icon" => Some("ui-literal-0917"),
        "Window" => Some("ui-literal-0918"),
        "Panes" => Some("ui-literal-0919"),
        "Blocks" => Some("ui-literal-0920"),
        "Text" => Some("ui-literal-0921"),
        "Cursor" => Some("ui-literal-0922"),
        "Tabs" => Some("ui-literal-0923"),
        "Full-screen Apps" => Some("ui-literal-0924"),
        "About" => Some("ui-literal-0925"),
        "Account" => Some("ui-literal-0926"),
        "Billing and usage" => Some("ui-literal-0927"),
        "Appearance" => Some("ui-literal-0928"),
        "Features" => Some("ui-literal-0929"),
        "Keyboard shortcuts" => Some("ui-literal-0930"),
        "Referrals" => Some("ui-literal-0931"),
        "Scripting" => Some("ui-literal-0932"),
        "Teams" => Some("ui-literal-0933"),
        "Indexing and projects" => Some("ui-literal-0934"),
        "Editor and Code Review" => Some("ui-literal-0935"),
        "Cloud platform" => Some("ui-literal-0936"),
        "Environments" => Some("ui-literal-0937"),
        "Oz Cloud API Keys" => Some("ui-literal-0938"),
        "General" => Some("ui-literal-0939"),
        "Text Editing" => Some("ui-literal-0940"),
        "Terminal Input" => Some("ui-literal-0941"),
        "Terminal" => Some("ui-literal-0942"),
        "System" => Some("ui-literal-0943"),
        "Click to learn more in docs" => Some("ui-literal-0944"),
        "This setting is not synced to your other devices" => Some("ui-literal-0945"),
        "Background opacity" => Some("ui-literal-0946"),
        "Earn rewards by sharing Warp with friends & colleagues" => Some("ui-literal-0947"),
        "Upgrade Plan" => Some("ui-literal-0948"),
        "Generate Stripe Billing Portal Link" => Some("ui-literal-0949"),
        "Toggle Settings Sync" => Some("ui-literal-0950"),
        "Settings sync" => Some("ui-literal-0951"),
        "Upgrade to Turbo plan" => Some("ui-literal-0952"),
        "Upgrade to Lightspeed plan" => Some("ui-literal-0953"),
        "Refer a friend" => Some("ui-literal-0954"),
        "Up to date" => Some("ui-literal-0955"),
        "Check for updates" => Some("ui-literal-0956"),
        "checking for update..." => Some("ui-literal-0957"),
        "downloading update..." => Some("ui-literal-0958"),
        "Update available" => Some("ui-literal-0959"),
        "Relaunch Warp" => Some("ui-literal-0960"),
        "Updating..." => Some("ui-literal-0961"),
        "Installed update" => Some("ui-literal-0962"),
        "A new version of Warp is available but can't be installed" => Some("ui-literal-0963"),
        "Update Warp manually" => Some("ui-literal-0964"),
        "A new version of Warp is installed but can't be launched." => Some("ui-literal-0965"),
        "Version" => Some("ui-literal-0966"),
        "Not yet loaded" => Some("ui-literal-0967"),
        "Refreshing…" => Some("ui-literal-0968"),
        "Warp can automatically index code repositories as you navigate them, helping agents quickly understand context and provide solutions. Code is never stored on the server. If a codebase is unable to be indexed, Warp can still navigate your codebase and gain insights via grep and find tool calling." => Some("ui-literal-0969"),
        "To exclude specific files or directories from indexing, add them to the .warpindexingignore file in your repository directory. These files will still be accessible to AI features, but they won't be included in codebase embeddings." => Some("ui-literal-0970"),
        "When set to true, Warp will automatically index code repositories as you navigate them - helping agents quickly understand context and provide targeted solutions." => Some("ui-literal-0971"),
        "AI Features must be enabled to use codebase indexing." => Some("ui-literal-0972"),
        "maximum number of codebase indexes has been reached" => Some("ui-literal-0973"),
        "Codebase Indexing" => Some("ui-literal-0974"),
        "Code Editor and Review" => Some("ui-literal-0975"),
        "create an index for this remote path" => Some("ui-literal-0976"),
        "refresh an existing index" => Some("ui-literal-0977"),
        "Available for download" => Some("ui-literal-0978"),
        "No folders have been initialized yet." => Some("ui-literal-0979"),
        "No index created" => Some("ui-literal-0980"),
        "Syncing..." => Some("ui-literal-0981"),
        "Synced" => Some("ui-literal-0982"),
        "Codebase too large" => Some("ui-literal-0983"),
        "Stale" => Some("ui-literal-0984"),
        "No index state for codebase" => Some("ui-literal-0985"),
        "No index built" => Some("ui-literal-0986"),
        "Index limit reached" => Some("ui-literal-0987"),
        "Unavailable" => Some("ui-literal-0988"),
        "Indexing..." => Some("ui-literal-0989"),
        "Installing..." => Some("ui-literal-0990"),
        "Checking..." => Some("ui-literal-0991"),
        "Busy" => Some("ui-literal-0992"),
        "Stopped" => Some("ui-literal-0993"),
        "Not running" => Some("ui-literal-0994"),
        "Auto open code review panel" => Some("ui-literal-0995"),
        "When this setting is on, the code review panel will open on the first accepted diff of a conversation" => Some("ui-literal-0996"),
        "Show diff stats on code review button" => Some("ui-literal-0997"),
        "Show lines added and removed counts on the code review button." => Some("ui-literal-0998"),
        "Adds an IDE-style project explorer / file tree to the left side tools panel." => Some("ui-literal-0999"),
        "Adds global file search to the left side tools panel." => Some("ui-literal-1000"),
        "Show hidden files in project explorer" => Some("ui-literal-1001"),
        "Show dotfiles and hidden files (starting with .) in the project explorer." => Some("ui-literal-1002"),
        "Format on save (requires an active language server)" => Some("ui-literal-1003"),
        "Only applies when a language server is active for the file. Automatically formats the file with the language server on save; other LSP features (hover, go-to-definition, references, diagnostics) are unaffected." => Some("ui-literal-1004"),
        "Enterprise secret redaction cannot be modified." => Some("ui-literal-1005"),
        "No enterprise regexes have been configured by your organization." => Some("ui-literal-1006"),
        "Enabled by your organization." => Some("ui-literal-1007"),
        "Secret visual redaction mode" => Some("ui-literal-1008"),
        "Choose how secrets are visually presented in the block list while keeping them searchable. This setting only affects what you see in the block list." => Some("ui-literal-1009"),
        "Your administrator has enabled zero data retention for your team. User generated content will never be collected." => Some("ui-literal-1010"),
        "Read more about Warp's use of data" => Some("ui-literal-1011"),
        "Send crash reports" => Some("ui-literal-1012"),
        "Crash reports assist with debugging and stability improvements." => Some("ui-literal-1013"),
        "Store AI conversations in the cloud" => Some("ui-literal-1014"),
        "Network log console" => Some("ui-literal-1015"),
        "Enable Warp Drive" => Some("ui-literal-1016"),
        "Disable Warp Drive" => Some("ui-literal-1017"),
        "To use Warp Drive, please create an account." => Some("ui-literal-1018"),
        "Warp Drive is a workspace in your terminal where you can save Workflows, Notebooks, Prompts, and Environment Variables for personal use or to share with a team." => Some("ui-literal-1019"),
        "This shortcut conflicts with other keybinds" => Some("ui-literal-1020"),
        "Press new keyboard shortcut" => Some("ui-literal-1021"),
        "Add your own custom keybindings to existing actions below." => Some("ui-literal-1022"),
        "to reference these keybindings in a side pane at anytime." => Some("ui-literal-1023"),
        "Choose an editor to open file links" => Some("ui-literal-1024"),
        "Choose an editor to open files from the code review panel, project explorer, and global search" => Some("ui-literal-1025"),
        "Choose a layout to open files in Warp" => Some("ui-literal-1026"),
        "Group files into single editor pane" => Some("ui-literal-1027"),
        "When this setting is on, any files opened in the same tab will be automatically grouped into a single editor pane." => Some("ui-literal-1028"),
        "Open Markdown files in Warp's Markdown Viewer by default" => Some("ui-literal-1029"),
        "Environments define where your ambient agents run. Set one up in minutes via GitHub (recommended), Warp-assisted setup, or manual configuration." => Some("ui-literal-1030"),
        "You haven’t set up any environments yet." => Some("ui-literal-1031"),
        "Choose how you’d like to set up your environment:" => Some("ui-literal-1032"),
        "Select the GitHub repositories you’d like to work with and we’ll suggest a base image and config" => Some("ui-literal-1033"),
        "Choose a locally set up project and we’ll help you set up an environment based on it" => Some("ui-literal-1034"),
        "Choose how you'd like to set up your environment" => Some("ui-literal-1035"),
        "Select the GitHub repositories you'd like to work with and we'll suggest a base image and config" => Some("ui-literal-1036"),
        "Choose a locally set up project and we'll help you set up an environment based on it" => Some("ui-literal-1037"),
        "Launch agent" => Some("ui-literal-1038"),
        "Get started" => Some("ui-literal-1039"),
        "Authorize" => Some("ui-literal-1040"),
        "Retry" => Some("ui-literal-1041"),
        "Subshells" => Some("ui-literal-1042"),
        "Added commands" => Some("ui-literal-1043"),
        "Denylisted commands" => Some("ui-literal-1044"),
        "Warpify SSH Sessions" => Some("ui-literal-1045"),
        "Install SSH extension" => Some("ui-literal-1046"),
        "Controls the installation behavior for Warp's SSH extension when a remote host doesn't have it installed." => Some("ui-literal-1047"),
        "Reuse existing SSH ControlMaster" => Some("ui-literal-1048"),
        "Attach to a live SSH ControlMaster you already have configured for the destination host instead of creating a Warp-owned one. Takes effect in new tabs." => Some("ui-literal-1049"),
        "Configure whether Warp attempts to “Warpify” (add support for blocks, input modes, etc) certain shells. " => Some("ui-literal-1050"),
        "When this setting is enabled, Warp will scan blocks, the contents of Warp Drive objects, and Oz prompts for potential sensitive information and prevent saving or sending this data to any servers. You can customize this list via regexes." => Some("ui-literal-1051"),
        "We've built a native console that allows you to view all communications from Warp to external servers to ensure you feel comfortable that your work is always kept safe." => Some("ui-literal-1052"),
        "View network logging" => Some("ui-literal-1053"),
        "At any time, you may choose to delete your Warp account permanently. You will no longer be able to use Warp." => Some("ui-literal-1054"),
        "Search sessions, agents, files..." => Some("ui-literal-1055"),
        "Search tabs..." => Some("ui-literal-1056"),
        "New session" => Some("ui-literal-1057"),
        "Settings" => Some("ui-literal-1058"),
        "New worktree config" => Some("ui-literal-1059"),
        "New tab config" => Some("ui-literal-1060"),
        "New tab group" => Some("ui-literal-1061"),
        "Reopen closed session" => Some("ui-literal-1062"),
        "Default App" => Some("ui-literal-1063"),
        "command (supports regex)" => Some("ui-literal-1064"),
        "Change your current theme." => Some("ui-literal-1065"),
        "Pick a theme for when your system is in light mode." => Some("ui-literal-1066"),
        "Pick a theme for when your system is in dark mode." => Some("ui-literal-1067"),
        "Search for a command" => Some("ui-literal-1068"),
        "files" => Some("ui-literal-1069"),
        "actions" => Some("ui-literal-1070"),
        "sessions" => Some("ui-literal-1071"),
        "launch configurations" => Some("ui-literal-1072"),
        "Recent" => Some("ui-literal-1073"),
        "Open Theme Picker" => Some("ui-literal-1074"),
        "Tabs panel" => Some("ui-literal-1075"),
        "Code review panel" => Some("ui-literal-1076"),
        "What's new" => Some("ui-literal-1077"),
        "Documentation" => Some("ui-literal-1078"),
        "Feedback" => Some("ui-literal-1079"),
        "View Warp logs" => Some("ui-literal-1080"),
        "Slack" => Some("ui-literal-1081"),
        "Invite a friend" => Some("ui-literal-1082"),
        "Use" => Some("ui-literal-1083"),
        "Search by name or by keys (ex. \"cmd d\")" => Some("ui-literal-1084"),
        "Accept Autosuggestion" => Some("ui-literal-1085"),
        "Accept Prompt Suggestion" => Some("ui-literal-1086"),
        "Activate Next Pane" => Some("ui-literal-1087"),
        "Activate Next Tab" => Some("ui-literal-1088"),
        "Activate Previous Pane" => Some("ui-literal-1089"),
        "Activate Previous Tab" => Some("ui-literal-1090"),
        "Add Cursor Above" => Some("ui-literal-1091"),
        "Add Cursor Below" => Some("ui-literal-1092"),
        "Add Selection for Next Occurrence" => Some("ui-literal-1093"),
        "Attach Selected Block as Agent Context" => Some("ui-literal-1094"),
        "Attach Selected Text as Agent Context" => Some("ui-literal-1095"),
        "Backward Tabulation Within an Executing Command" => Some("ui-literal-1096"),
        "Bookmark Selected Block" => Some("ui-literal-1097"),
        "Expand Selected Blocks Above" => Some("ui-literal-1098"),
        "Expand Selected Blocks Below" => Some("ui-literal-1099"),
        "Search history" => Some("ui-literal-1100"),
        "Search workflows" => Some("ui-literal-1101"),
        "Search prompts" => Some("ui-literal-1102"),
        "Search notebooks" => Some("ui-literal-1103"),
        "Search plans" => Some("ui-literal-1104"),
        "e.g. replace string in file" => Some("ui-literal-1105"),
        "Search actions" => Some("ui-literal-1106"),
        "Search sessions" => Some("ui-literal-1107"),
        "Search tabs" => Some("ui-literal-1108"),
        "Search conversations" => Some("ui-literal-1109"),
        "Search launch configurations" => Some("ui-literal-1110"),
        "Search objects in drive" => Some("ui-literal-1111"),
        "Search environment variables" => Some("ui-literal-1112"),
        "Search prompt history" => Some("ui-literal-1113"),
        "Search files" => Some("ui-literal-1114"),
        "Search commands" => Some("ui-literal-1115"),
        "Search blocks" => Some("ui-literal-1116"),
        "Search code symbols" => Some("ui-literal-1117"),
        "Search AI rules" => Some("ui-literal-1118"),
        "Search code repos" => Some("ui-literal-1119"),
        "Search diff sets" => Some("ui-literal-1120"),
        "Search static slash commands" => Some("ui-literal-1121"),
        "Search skills" => Some("ui-literal-1122"),
        "Search base models" => Some("ui-literal-1123"),
        "Search full terminal use models" => Some("ui-literal-1124"),
        "Search conversations in current directory" => Some("ui-literal-1125"),
        "history" => Some("ui-literal-1126"),
        "workflows" => Some("ui-literal-1127"),
        "prompts" => Some("ui-literal-1128"),
        "notebooks" => Some("ui-literal-1129"),
        "plans" => Some("ui-literal-1130"),
        "AI command suggestions" => Some("ui-literal-1131"),
        "tabs" => Some("ui-literal-1132"),
        "conversations" => Some("ui-literal-1133"),
        "Warp Drive" => Some("ui-literal-1134"),
        "environment variables" => Some("ui-literal-1135"),
        "prompt history" => Some("ui-literal-1136"),
        "commands" => Some("ui-literal-1137"),
        "blocks" => Some("ui-literal-1138"),
        "code" => Some("ui-literal-1139"),
        "rules" => Some("ui-literal-1140"),
        "repos" => Some("ui-literal-1141"),
        "diff sets" => Some("ui-literal-1142"),
        "slash commands" => Some("ui-literal-1143"),
        "skills" => Some("ui-literal-1144"),
        "base models" => Some("ui-literal-1145"),
        "full terminal use models" => Some("ui-literal-1146"),
        "current directory conversations" => Some("ui-literal-1147"),
        "Prompts" => Some("ui-literal-1148"),
        "(Experimental) Toggle Classic Completions Mode" => Some("ui-literal-1149"),
        "Add Current Folder as Project" => Some("ui-literal-1150"),
        "Alternate Terminal Paste" => Some("ui-literal-1151"),
        "Ask Warp AI" => Some("ui-literal-1152"),
        "Ask Warp AI About Selection" => Some("ui-literal-1153"),
        "Ask Warp AI About Last Block" => Some("ui-literal-1154"),
        "Check for Updates" => Some("ui-literal-1155"),
        "Clear Blocks" => Some("ui-literal-1156"),
        "Clear and Reset AI Context Menu Query" => Some("ui-literal-1157"),
        "Clear Command Editor" => Some("ui-literal-1158"),
        "Clear Screen" => Some("ui-literal-1159"),
        "Clear Selected Lines" => Some("ui-literal-1160"),
        "Close Current Session" => Some("ui-literal-1161"),
        "Close Window" => Some("ui-literal-1162"),
        "Close All Tabs" => Some("ui-literal-1163"),
        "Close Focused Panel" => Some("ui-literal-1164"),
        "Close Other Tabs" => Some("ui-literal-1165"),
        "Close Saved Tabs" => Some("ui-literal-1166"),
        "Close Tabs to the Right" => Some("ui-literal-1167"),
        "Close the Current Tab" => Some("ui-literal-1168"),
        "Command Search" => Some("ui-literal-1169"),
        "Copy Access Token to Clipboard" => Some("ui-literal-1170"),
        "Copy and Clear Selected Lines" => Some("ui-literal-1171"),
        "Copy Command" => Some("ui-literal-1172"),
        "Copy Command and Output" => Some("ui-literal-1173"),
        "Copy Command Output" => Some("ui-literal-1174"),
        "Copy Git Branch" => Some("ui-literal-1175"),
        "Copy Rich-Text Buffer" => Some("ui-literal-1176"),
        "Copy Rich-Text Selection" => Some("ui-literal-1177"),
        "Create a New Personal Folder" => Some("ui-literal-1178"),
        "Create a New Personal Notebook" => Some("ui-literal-1179"),
        "Create a New Personal Prompt" => Some("ui-literal-1180"),
        "Create a New Personal Workflow" => Some("ui-literal-1181"),
        "Create a New Team Folder" => Some("ui-literal-1182"),
        "Create a New Team Notebook" => Some("ui-literal-1183"),
        "Create a New Team Prompt" => Some("ui-literal-1184"),
        "Create a New Team Workflow" => Some("ui-literal-1185"),
        "Create New Personal Environment Variables" => Some("ui-literal-1186"),
        "Create New Project" => Some("ui-literal-1187"),
        "Create New Tab" => Some("ui-literal-1188"),
        "Create New Tab Group" => Some("ui-literal-1189"),
        "Create New Team Environment Variables" => Some("ui-literal-1190"),
        "Create or Edit Link" => Some("ui-literal-1191"),
        "Create Tab Group From Active or Selected Tab(s)" => Some("ui-literal-1192"),
        "Cursor at Buffer End" => Some("ui-literal-1193"),
        "Cursor at Buffer Start" => Some("ui-literal-1194"),
        "Cut All Left" => Some("ui-literal-1195"),
        "Cut All Right" => Some("ui-literal-1196"),
        "Cut Word Left" => Some("ui-literal-1197"),
        "Cut Word Right" => Some("ui-literal-1198"),
        "Cycle to Next Orchestration Session" => Some("ui-literal-1199"),
        "Cycle to Previous Orchestration Session" => Some("ui-literal-1200"),
        "De-Select Shell Commands" => Some("ui-literal-1201"),
        "Decrease Font Size" => Some("ui-literal-1202"),
        "Decrease Notebook Font Size" => Some("ui-literal-1203"),
        "Decrease Zoom Level" => Some("ui-literal-1204"),
        "Delete All Left" => Some("ui-literal-1205"),
        "Delete All Right" => Some("ui-literal-1206"),
        "Delete the Next Character" => Some("ui-literal-1207"),
        "Delete the Next Word" => Some("ui-literal-1208"),
        "Delete the Previous Character" => Some("ui-literal-1209"),
        "Delete the Previous Word" => Some("ui-literal-1210"),
        "Delete to End of Line" => Some("ui-literal-1211"),
        "Delete to Line End Within an Executing Command" => Some("ui-literal-1212"),
        "Delete to Line Start Within an Executing Command" => Some("ui-literal-1213"),
        "Delete to Start of Line" => Some("ui-literal-1214"),
        "Delete Word Left" => Some("ui-literal-1215"),
        "Delete Word Left Within an Executing Command" => Some("ui-literal-1216"),
        "Delete Word Right" => Some("ui-literal-1217"),
        "Edit Code Diff" => Some("ui-literal-1218"),
        "Edit Requested Command" => Some("ui-literal-1219"),
        "End" => Some("ui-literal-1220"),
        "Exit Vim Insert Mode" => Some("ui-literal-1221"),
        "Export All Warp Drive Objects" => Some("ui-literal-1222"),
        "Extend Selection Down" => Some("ui-literal-1223"),
        "Extend Selection Left" => Some("ui-literal-1224"),
        "Extend Selection One Word Left" => Some("ui-literal-1225"),
        "Extend Selection One Word Right" => Some("ui-literal-1226"),
        "Extend Selection Right" => Some("ui-literal-1227"),
        "Extend Selection Up" => Some("ui-literal-1228"),
        "Find in Notebook" => Some("ui-literal-1229"),
        "Find in Terminal" => Some("ui-literal-1230"),
        "Find in Code Editor" => Some("ui-literal-1231"),
        "Find the Next Occurrence of Your Search Query" => Some("ui-literal-1232"),
        "Find the Previous Occurrence of Your Search Query" => Some("ui-literal-1233"),
        "Find Within Selected Block" => Some("ui-literal-1234"),
        "Focus Terminal Input From Warp AI" => Some("ui-literal-1235"),
        "Focus Terminal Input From File" => Some("ui-literal-1236"),
        "Focus Terminal Input From Notebook" => Some("ui-literal-1237"),
        "Focus Next Match" => Some("ui-literal-1238"),
        "Focus Previous Match" => Some("ui-literal-1239"),
        "Focus Terminal Input" => Some("ui-literal-1240"),
        "Fold" => Some("ui-literal-1241"),
        "Fold Selected Ranges" => Some("ui-literal-1242"),
        "Go to Line" => Some("ui-literal-1243"),
        "History Search" => Some("ui-literal-1244"),
        "Home" => Some("ui-literal-1245"),
        "Import External Settings" => Some("ui-literal-1246"),
        "Import To Personal Drive" => Some("ui-literal-1247"),
        "Import To Team Drive" => Some("ui-literal-1248"),
        "Increase Font Size" => Some("ui-literal-1249"),
        "Increase Notebook Font Size" => Some("ui-literal-1250"),
        "Increase Zoom Level" => Some("ui-literal-1251"),
        "Initiate Project for Warp" => Some("ui-literal-1252"),
        "Insert Command Correction" => Some("ui-literal-1253"),
        "Insert a Newline" => Some("ui-literal-1254"),
        "Insert Last Word of Previous Command" => Some("ui-literal-1255"),
        "Insert Newline" => Some("ui-literal-1256"),
        "Insert Non-Expanding Space" => Some("ui-literal-1257"),
        "Inspect Command" => Some("ui-literal-1258"),
        "Install Oz CLI Globally for Use Outside of Warp" => Some("ui-literal-1259"),
        "Install Warp Control CLI Globally for Use Outside of Warp" => Some("ui-literal-1260"),
        "Install Update and Relaunch" => Some("ui-literal-1261"),
        "Invite People..." => Some("ui-literal-1262"),
        "Join Our Slack Community (Opens External Link)" => Some("ui-literal-1263"),
        "Jump to Latest Agent Message" => Some("ui-literal-1264"),
        "Jump to Latest Agent Task" => Some("ui-literal-1265"),
        "Launch Configuration Palette" => Some("ui-literal-1266"),
        "Left Panel: Agent Conversations" => Some("ui-literal-1267"),
        "Left Panel: Global Search" => Some("ui-literal-1268"),
        "Left Panel: Project Explorer" => Some("ui-literal-1269"),
        "Left Panel: Warp Drive" => Some("ui-literal-1270"),
        "Load Agent Mode Conversation (From Debug Link in Clipboard)" => Some("ui-literal-1271"),
        "Log Editor State" => Some("ui-literal-1272"),
        "Log Out" => Some("ui-literal-1273"),
        "Move Backward One Subword" => Some("ui-literal-1274"),
        "Move Backward One Word" => Some("ui-literal-1275"),
        "Move Forward One Subword" => Some("ui-literal-1276"),
        "Move Forward One Word" => Some("ui-literal-1277"),
        "Move Cursor Down" => Some("ui-literal-1278"),
        "Move Cursor End Within an Executing Command" => Some("ui-literal-1279"),
        "Move Cursor Home Within an Executing Command" => Some("ui-literal-1280"),
        "Move Cursor Left" => Some("ui-literal-1281"),
        "Move Cursor One Word Left" => Some("ui-literal-1282"),
        "Move Cursor One Word Right" => Some("ui-literal-1283"),
        "Move Cursor One Word to the Left Within an Executing Command" => Some("ui-literal-1284"),
        "Move Cursor One Word to the Right Within an Executing Command" => Some("ui-literal-1285"),
        "Move Cursor Right" => Some("ui-literal-1286"),
        "Move Cursor to End of Line" => Some("ui-literal-1287"),
        "Move Cursor to Start of Line" => Some("ui-literal-1288"),
        "Move Cursor to the Bottom" => Some("ui-literal-1289"),
        "Move Cursor to the Top" => Some("ui-literal-1290"),
        "Move Cursor Up" => Some("ui-literal-1291"),
        "Move Tab Left" => Some("ui-literal-1292"),
        "Move Tab Right" => Some("ui-literal-1293"),
        "Move to End of Line" => Some("ui-literal-1294"),
        "Move to End of Paragraph" => Some("ui-literal-1295"),
        "Move to Line End" => Some("ui-literal-1296"),
        "Move to Line Start" => Some("ui-literal-1297"),
        "Move to Start of Line" => Some("ui-literal-1298"),
        "Move to Start of Paragraph" => Some("ui-literal-1299"),
        "Move to the End of the Buffer" => Some("ui-literal-1300"),
        "Move to the End of the Paragraph" => Some("ui-literal-1301"),
        "Move to the Start of the Buffer" => Some("ui-literal-1302"),
        "Move to the Start of the Paragraph" => Some("ui-literal-1303"),
        "New Agent Tab" => Some("ui-literal-1304"),
        "New Cloud Agent Tab" => Some("ui-literal-1305"),
        "New File" => Some("ui-literal-1306"),
        "New Terminal Tab" => Some("ui-literal-1307"),
        "New Agent Conversation" => Some("ui-literal-1308"),
        "Open AI Command Suggestions" => Some("ui-literal-1309"),
        "Open AI Rules" => Some("ui-literal-1310"),
        "Open Left Panel" => Some("ui-literal-1311"),
        "Open MCP Servers" => Some("ui-literal-1312"),
        "Open Settings" => Some("ui-literal-1313"),
        "Open Settings: AI" => Some("ui-literal-1314"),
        "Open Settings: About" => Some("ui-literal-1315"),
        "Open Settings: Account" => Some("ui-literal-1316"),
        "Open Settings: Appearance" => Some("ui-literal-1317"),
        "Open Settings: Billing and Usage" => Some("ui-literal-1318"),
        "Open Settings: Code" => Some("ui-literal-1319"),
        "Open Settings: Environments" => Some("ui-literal-1320"),
        "Open Settings: Features" => Some("ui-literal-1321"),
        "Open Settings: Keyboard Shortcuts" => Some("ui-literal-1322"),
        "Open Settings: MCP Servers" => Some("ui-literal-1323"),
        "Open Settings: Privacy" => Some("ui-literal-1324"),
        "Open Settings: Referrals" => Some("ui-literal-1325"),
        "Open Settings: Shared Blocks" => Some("ui-literal-1326"),
        "Open Settings: Teams" => Some("ui-literal-1327"),
        "Open Settings: Warpify" => Some("ui-literal-1328"),
        "Open Block Context Menu" => Some("ui-literal-1329"),
        "Open Completions Menu" => Some("ui-literal-1330"),
        "Open Global Search" => Some("ui-literal-1331"),
        "Open Keybindings Editor" => Some("ui-literal-1332"),
        "Open Repository" => Some("ui-literal-1333"),
        "Open Settings File" => Some("ui-literal-1334"),
        "Open Tab Configs Menu" => Some("ui-literal-1335"),
        "Open View Tree Debugger" => Some("ui-literal-1336"),
        "Paste the Last Deleted Text" => Some("ui-literal-1337"),
        "Pin Current Tab" => Some("ui-literal-1338"),
        "Pin Current Tab Group" => Some("ui-literal-1339"),
        "Quit Warp" => Some("ui-literal-1340"),
        "Redo" => Some("ui-literal-1341"),
        "Reinput Selected Commands" => Some("ui-literal-1342"),
        "Reinput Selected Commands as Root" => Some("ui-literal-1343"),
        "Reload File" => Some("ui-literal-1344"),
        "Remove Active or Selected Tab(s) From Group" => Some("ui-literal-1345"),
        "Remove the Previous Character" => Some("ui-literal-1346"),
        "Rename the Current Pane" => Some("ui-literal-1347"),
        "Rename the Current Tab" => Some("ui-literal-1348"),
        "Reopen Closed Session" => Some("ui-literal-1349"),
        "Reset Font Size to Default" => Some("ui-literal-1350"),
        "Reset Notebook Font Size" => Some("ui-literal-1351"),
        "Reset Zoom Level to Default" => Some("ui-literal-1352"),
        "Resize Pane > Move Divider Down" => Some("ui-literal-1353"),
        "Resize Pane > Move Divider Left" => Some("ui-literal-1354"),
        "Resize Pane > Move Divider Right" => Some("ui-literal-1355"),
        "Resize Pane > Move Divider Up" => Some("ui-literal-1356"),
        "Restart Warp AI" => Some("ui-literal-1357"),
        "Run Selected Commands" => Some("ui-literal-1358"),
        "Sample Process" => Some("ui-literal-1359"),
        "Save All Unsaved Files in Code Review" => Some("ui-literal-1360"),
        "Save File As" => Some("ui-literal-1361"),
        "Save New Launch Configuration" => Some("ui-literal-1362"),
        "Save Workflow" => Some("ui-literal-1363"),
        "Scroll Down Half a Page (Vim)" => Some("ui-literal-1364"),
        "Scroll Terminal Output Down One Line" => Some("ui-literal-1365"),
        "Scroll Terminal Output Down One Page" => Some("ui-literal-1366"),
        "Scroll Terminal Output Up One Line" => Some("ui-literal-1367"),
        "Scroll Terminal Output Up One Page" => Some("ui-literal-1368"),
        "Scroll to Bottom of Selected Block" => Some("ui-literal-1369"),
        "Scroll to Top of Selected Block" => Some("ui-literal-1370"),
        "Scroll Up Half a Page (Vim)" => Some("ui-literal-1371"),
        "Search Warp Drive" => Some("ui-literal-1372"),
        "Select To Line End" => Some("ui-literal-1373"),
        "Select To Line Start" => Some("ui-literal-1374"),
        "Select All" => Some("ui-literal-1375"),
        "Select All Blocks" => Some("ui-literal-1376"),
        "Select All Text" => Some("ui-literal-1377"),
        "Select and Move to the Bottom" => Some("ui-literal-1378"),
        "Select and Move to the Top" => Some("ui-literal-1379"),
        "Select Down" => Some("ui-literal-1380"),
        "Select Next Block" => Some("ui-literal-1381"),
        "Select Next Command" => Some("ui-literal-1382"),
        "Select One Character to the Left" => Some("ui-literal-1383"),
        "Select One Character to the Right" => Some("ui-literal-1384"),
        "Select One Subword to the Left" => Some("ui-literal-1385"),
        "Select One Subword to the Right" => Some("ui-literal-1386"),
        "Select One Word to the Left" => Some("ui-literal-1387"),
        "Select One Word to the Right" => Some("ui-literal-1388"),
        "Select Previous Block" => Some("ui-literal-1389"),
        "Select Previous Command" => Some("ui-literal-1390"),
        "Select Shell Command at Cursor" => Some("ui-literal-1391"),
        "Select the Closest Bookmark Down" => Some("ui-literal-1392"),
        "Select the Closest Bookmark Up" => Some("ui-literal-1393"),
        "Select to End of Line" => Some("ui-literal-1394"),
        "Select to End of Paragraph" => Some("ui-literal-1395"),
        "Select to Start of Line" => Some("ui-literal-1396"),
        "Select to Start of Paragraph" => Some("ui-literal-1397"),
        "Select Up" => Some("ui-literal-1398"),
        "Send Feedback (Opens External Link)" => Some("ui-literal-1399"),
        "Set Input Mode to Agent Mode" => Some("ui-literal-1400"),
        "Set Input Mode to Terminal Mode" => Some("ui-literal-1401"),
        "Share Current Session" => Some("ui-literal-1402"),
        "Share Pane" => Some("ui-literal-1403"),
        "Share Selected Block" => Some("ui-literal-1404"),
        "Show Warp Network Log" => Some("ui-literal-1405"),
        "Show Find Bar in Code Review" => Some("ui-literal-1406"),
        "Split Pane Down" => Some("ui-literal-1407"),
        "Split Pane Left" => Some("ui-literal-1408"),
        "Split Pane Right" => Some("ui-literal-1409"),
        "Split Pane Up" => Some("ui-literal-1410"),
        "Stop Synchronizing Any Panes" => Some("ui-literal-1411"),
        "Stop Sharing Current Session" => Some("ui-literal-1412"),
        "Submit the Input" => Some("ui-literal-1413"),
        "Switch Focus to Left Panel" => Some("ui-literal-1414"),
        "Switch Focus to Right Panel" => Some("ui-literal-1415"),
        "Switch Panes Down" => Some("ui-literal-1416"),
        "Switch Panes Left" => Some("ui-literal-1417"),
        "Switch Panes Right" => Some("ui-literal-1418"),
        "Switch Panes Up" => Some("ui-literal-1419"),
        "Switch to 1st Tab" => Some("ui-literal-1420"),
        "Switch to 2nd Tab" => Some("ui-literal-1421"),
        "Switch to 3rd Tab" => Some("ui-literal-1422"),
        "Switch to 4th Tab" => Some("ui-literal-1423"),
        "Switch to 5th Tab" => Some("ui-literal-1424"),
        "Switch to 6th Tab" => Some("ui-literal-1425"),
        "Switch to 7th Tab" => Some("ui-literal-1426"),
        "Switch to 8th Tab" => Some("ui-literal-1427"),
        "Switch to Last Tab" => Some("ui-literal-1428"),
        "Take Control of Running Command" => Some("ui-literal-1429"),
        "Terminal Session" => Some("ui-literal-1430"),
        "Toggle Agent Conversation List View" => Some("ui-literal-1431"),
        "Toggle Auto-Execute Mode" => Some("ui-literal-1432"),
        "Toggle CLI Agent Rich Input" => Some("ui-literal-1433"),
        "Toggle Conversation Details Panel" => Some("ui-literal-1434"),
        "Toggle Files Palette" => Some("ui-literal-1435"),
        "Toggle Hide CLI Responses" => Some("ui-literal-1436"),
        "Toggle Maximize Active Pane" => Some("ui-literal-1437"),
        "Toggle Maximize Code Review Panel" => Some("ui-literal-1438"),
        "Toggle PTY Recording for Session" => Some("ui-literal-1439"),
        "Toggle Queue Next Prompt" => Some("ui-literal-1440"),
        "Toggle Resource Center" => Some("ui-literal-1441"),
        "Toggle Sticky Command Header in Active Pane" => Some("ui-literal-1442"),
        "Toggle Synchronizing All Panes in All Tabs" => Some("ui-literal-1443"),
        "Toggle Synchronizing All Panes in Current Tab" => Some("ui-literal-1444"),
        "Toggle Warp AI" => Some("ui-literal-1445"),
        "Toggle Warp Drive" => Some("ui-literal-1446"),
        "Toggle Block Filter on Selected or Last Block" => Some("ui-literal-1447"),
        "Toggle Case-Sensitive Search" => Some("ui-literal-1448"),
        "Toggle Code Review" => Some("ui-literal-1449"),
        "Toggle Command Palette" => Some("ui-literal-1450"),
        "Toggle Comment" => Some("ui-literal-1451"),
        "Toggle File Navigation in Code Review" => Some("ui-literal-1452"),
        "Toggle Fullscreen" => Some("ui-literal-1453"),
        "Toggle Hidden Files in Project Explorer" => Some("ui-literal-1454"),
        "Toggle Inline Code Styling" => Some("ui-literal-1455"),
        "Toggle Keyboard Shortcuts" => Some("ui-literal-1456"),
        "Toggle Navigation Palette" => Some("ui-literal-1457"),
        "Toggle Notification Mailbox" => Some("ui-literal-1458"),
        "Toggle Project Explorer" => Some("ui-literal-1459"),
        "Toggle Regular Expression Search" => Some("ui-literal-1460"),
        "Toggle Rich-Text Debug Mode" => Some("ui-literal-1461"),
        "Toggle Sticky Command Header" => Some("ui-literal-1462"),
        "Toggle Strikethrough Styling" => Some("ui-literal-1463"),
        "Toggle Team Workflows Modal" => Some("ui-literal-1464"),
        "Toggle the Agent Management View" => Some("ui-literal-1465"),
        "Toggle Underline Styling" => Some("ui-literal-1466"),
        "Toggle Vertical Tabs Panel" => Some("ui-literal-1467"),
        "Trigger Auto Detection" => Some("ui-literal-1468"),
        "Trigger a Panic (For Testing Sentry-Rust)" => Some("ui-literal-1469"),
        "Turn Notifications Off" => Some("ui-literal-1470"),
        "Turn Notifications On" => Some("ui-literal-1471"),
        "Undo Global Oz CLI Installation (Oz Will Still Work Within Warp)" => Some("ui-literal-1472"),
        "Undo Global Warp Control CLI Installation (Warpctrl Will Still Work Within Warp)" => Some("ui-literal-1473"),
        "Unfold" => Some("ui-literal-1474"),
        "Unpin Current Tab" => Some("ui-literal-1475"),
        "Unpin Current Tab Group" => Some("ui-literal-1476"),
        "View Warp Logs" => Some("ui-literal-1477"),
        "View Latest Changelog" => Some("ui-literal-1478"),
        "View Privacy Policy (Opens External Link)" => Some("ui-literal-1479"),
        "View User Docs (Opens External Link)" => Some("ui-literal-1480"),
        "Warpify Subshell" => Some("ui-literal-1481"),
        "Workflows" => Some("ui-literal-1482"),
        "Write Current Codebase Index Snapshot" => Some("ui-literal-1483"),
        "Write Heap Profile to Disk" => Some("ui-literal-1484"),
        "[Debug] Enter Onboarding State" => Some("ui-literal-1485"),
        "[Debug] Generate Codebase Index" => Some("ui-literal-1486"),
        "[Debug] Install OpenCode Warp Plugin" => Some("ui-literal-1487"),
        "[Debug] Log Review Comment Send Status for Active Tab" => Some("ui-literal-1488"),
        "[Debug] Onboarding Callout: Modality - No Project" => Some("ui-literal-1489"),
        "[Debug] Onboarding Callout: Modality - Project" => Some("ui-literal-1490"),
        "[Debug] Onboarding Callout: Modality - Terminal" => Some("ui-literal-1491"),
        "[Debug] Onboarding Callout: WarpInput - No Project" => Some("ui-literal-1492"),
        "[Debug] Onboarding Callout: WarpInput - Project" => Some("ui-literal-1493"),
        "[Debug] Open Auto-Handoff Sleep Modal" => Some("ui-literal-1494"),
        "[Debug] Open Build Plan Migration Modal" => Some("ui-literal-1495"),
        "[Debug] Open Free AI Removal Modal" => Some("ui-literal-1496"),
        "[Debug] Open OpenWarp Launch Modal" => Some("ui-literal-1497"),
        "[Debug] Open Orchestration Launch Modal" => Some("ui-literal-1498"),
        "[Debug] Open Oz Launch Modal" => Some("ui-literal-1499"),
        "[Debug] Open Session Config Modal" => Some("ui-literal-1500"),
        "[Debug] Reset Auto-Handoff Sleep Modal State" => Some("ui-literal-1501"),
        "[Debug] Reset Build Plan Migration Modal State" => Some("ui-literal-1502"),
        "[Debug] Reset Free AI Removal Modal State" => Some("ui-literal-1503"),
        "[Debug] Reset OpenWarp Launch Modal State" => Some("ui-literal-1504"),
        "[Debug] Reset Orchestration Launch Modal State" => Some("ui-literal-1505"),
        "[Debug] Reset Oz Launch Modal State" => Some("ui-literal-1506"),
        "[Debug] Start HOA Onboarding Flow" => Some("ui-literal-1507"),
        "[Debug] Trigger Auto-Handoff to Cloud" => Some("ui-literal-1508"),
        "[Debug] Un-Dismiss AWS Login Banner" => Some("ui-literal-1509"),
        "[Debug] Use Local OpenCode Warp Plugin (Testing Only)" => Some("ui-literal-1510"),
        "[a11y] Set Concise Accessibility Announcements" => Some("ui-literal-1511"),
        "[a11y] Set Verbose Accessibility Announcements" => Some("ui-literal-1512"),
        "Clear" => Some("ui-literal-1513"),
        "No suggestions" => Some("ui-literal-1514"),
        "Ignore this suggestion" => Some("ui-literal-1515"),
        "Open in Warp Desktop?" => Some("ui-literal-1516"),
        "Future links will automatically open on desktop." => Some("ui-literal-1517"),
        "Download Warp Desktop?" => Some("ui-literal-1518"),
        "Always open {object_kind} on the web?" => Some("ui-literal-1519"),
        "You can change this at any time in settings." => Some("ui-literal-1520"),
        "Font family defined" => Some("ui-literal-1521"),
        "To toggle this panel" => Some("ui-literal-1522"),
        "Go to settings > keyboard shortcuts to configure custom keybindings" => Some("ui-literal-1523"),
        "launch_config.yaml" => Some("ui-literal-1524"),
        "Save Current Configuration" => Some("ui-literal-1525"),
        "Saved successfully to " => Some("ui-literal-1526"),
        "\nThe YAML file is saved to " => Some("ui-literal-1527"),
        "Link SSO" => Some("ui-literal-1528"),
        "Enter auth token" => Some("ui-literal-1529"),
        "Paste your auth token below" => Some("ui-literal-1530"),
        "Paste your auth token from the browser to get complete login." => Some("ui-literal-1531"),
        " Not the first time? See our " => Some("ui-literal-1532"),
        "troubleshooting docs" => Some("ui-literal-1533"),
        "Request to log in failed." => Some("ui-literal-1534"),
        "Request to sign up failed." => Some("ui-literal-1535"),
        "The redirect URL pasted did not originate from this app. Please click the button below to try again." => Some("ui-literal-1536"),
        "Auth Token" => Some("ui-literal-1537"),
        "Sign in on your browser to continue" => Some("ui-literal-1538"),
        "Project Explorer" => Some("ui-literal-1539"),
        "New Team Notebook" => Some("ui-literal-1540"),
        "New Personal Notebook" => Some("ui-literal-1541"),
        "New Team Workflow" => Some("ui-literal-1542"),
        "New Personal Workflow" => Some("ui-literal-1543"),
        "New Team Folder" => Some("ui-literal-1544"),
        "New Personal Folder" => Some("ui-literal-1545"),
        "Global Search" => Some("ui-literal-1546"),
        "Agent conversation list view" => Some("ui-literal-1547"),
        "Command Palette" => Some("ui-literal-1548"),
        "move tab up" => Some("ui-literal-1549"),
        "move tab down" => Some("ui-literal-1550"),
        "close tabs below" => Some("ui-literal-1551"),
        "Navigation Palette" => Some("ui-literal-1552"),
        "New Team Environment Variables" => Some("ui-literal-1553"),
        "New Personal Environment Variables" => Some("ui-literal-1554"),
        "New Personal Prompt" => Some("ui-literal-1555"),
        "New Team Prompt" => Some("ui-literal-1556"),
        "Appearance..." => Some("ui-literal-1557"),
        "View Shared Blocks..." => Some("ui-literal-1558"),
        "Configure Keyboard Shortcuts..." => Some("ui-literal-1559"),
        "About Warp" => Some("ui-literal-1560"),
        "Open Team Settings" => Some("ui-literal-1561"),
        "Configure Warpify..." => Some("ui-literal-1562"),
        "Close session" => Some("ui-literal-1563"),
        "Close session?" => Some("ui-literal-1564"),
        "You are about to close a session that is currently being shared. Closing it will end sharing for everyone." => Some("ui-literal-1565"),
        "This conversation will be permanently deleted. This action cannot be undone." => Some("ui-literal-1566"),
        "Rewinding does not affect files edited manually or via shell commands." => Some("ui-literal-1567"),
        "Are you sure you want to rewind? This will restore your code and conversation to before this point, and cancel any commands the agent is currently running. A copy of the original conversation will be saved in your conversation history." => Some("ui-literal-1568"),
        "Search repos" => Some("ui-literal-1569"),
        "Check out the latest version and try again." => Some("ui-literal-1570"),
        "Ask Warp AI to explain errors, suggest commands or write scripts." => Some("ui-literal-1571"),
        "Warp doesn't currently support your default shell, falling back to zsh.  " => Some("ui-literal-1572"),
        "Add as context" => Some("ui-literal-1573"),
        "This file has saved changes that are not reflected here." => Some("ui-literal-1574"),
        "Discard this version" => Some("ui-literal-1575"),
        "Remote host disconnected. You will not be able to see updates and save changes." => Some("ui-literal-1576"),
        "{severity_text}: " => Some("ui-literal-1577"),
        "Accept and save" => Some("ui-literal-1578"),
        "Close Workflow" => Some("ui-literal-1579"),
        "Sign in to edit" => Some("ui-literal-1580"),
        "Restore workflow from trash" => Some("ui-literal-1581"),
        "Command edited." => Some("ui-literal-1582"),
        "to cycle parameters" => Some("ui-literal-1583"),
        "Purchasing these credits would take you over your monthly spend limit. " => Some("ui-literal-1584"),
        " to continue." => Some("ui-literal-1585"),
        "When enabled, " => Some("ui-literal-1586"),
        " will automatically purchase your selected package when you run out. " => Some("ui-literal-1587"),
        "Billed to API" => Some("ui-literal-1588"),
        "Loading prompt..." => Some("ui-literal-1589"),
        "Share block" => Some("ui-literal-1590"),
        "Seems like your shell is taking a while to start...  " => Some("ui-literal-1591"),
        "Show initialization block" => Some("ui-literal-1592"),
        "Seems like your completions are not working (" => Some("ui-literal-1593"),
        "more info" => Some("ui-literal-1594"),
        "). Enabling the SSH extension in " => Some("ui-literal-1595"),
        " may resolve this issue." => Some("ui-literal-1596"),
        "Your shell configuration is incompatible with Warp...  " => Some("ui-literal-1597"),
        "Did you intend " => Some("ui-literal-1598"),
        " to move the cursor?" => Some("ui-literal-1599"),
        "Yes, use Emacs-style bindings" => Some("ui-literal-1600"),
        "No, keep IDE bindings" => Some("ui-literal-1601"),
        "You seem to be running an older (unsupported) version, please follow " => Some("ui-literal-1602"),
        " to update to the latest version." => Some("ui-literal-1603"),
        "Pure is not yet supported in Warp. You might consider one of the \n                        supported prompts as an alternative.  " => Some("ui-literal-1604"),
        "If you installed Warp using " => Some("ui-literal-1605"),
        " or a compatible tool, the pre-filled command will update Warp for you." => Some("ui-literal-1606"),
        "\nThe command below includes a one-time configuration of the Warp package repository and PGP signing key." => Some("ui-literal-1607"),
        "\nThe " => Some("ui-literal-1608"),
        " function ensures the Warp package repository is enabled, as we've detected you recently upgraded your distribution." => Some("ui-literal-1609"),
        "\nReview the command below, then " => Some("ui-literal-1610"),
        "press enter" => Some("ui-literal-1611"),
        " to install the update and re-launch Warp.  " => Some("ui-literal-1612"),
        "Please report any issues" => Some("ui-literal-1613"),
        "(no response body: {e:#})" => Some("ui-literal-1614"),
        "Search API keys" => Some("ui-literal-1615"),
        "Create and manage API keys to allow other Oz cloud agents to access your Warp account.\nFor more information, visit the " => Some("ui-literal-1616"),
        "No API Keys" => Some("ui-literal-1617"),
        "Create a key to manage external access to Warp" => Some("ui-literal-1618"),
        "No API keys match your search" => Some("ui-literal-1619"),
        "No repos selected yet" => Some("ui-literal-1620"),
        "All locally indexed repos are already selected." => Some("ui-literal-1621"),
        "Select repos for your environment" => Some("ui-literal-1622"),
        "Remove endpoint?" => Some("ui-literal-1623"),
        "Your new team name" => Some("ui-literal-1624"),
        "Transfer team ownership?" => Some("ui-literal-1625"),
        "Contact Admin to request access" => Some("ui-literal-1626"),
        "Repo(s)" => Some("ui-literal-1627"),
        "Auth with GitHub" => Some("ui-literal-1628"),
        "Type owner/repo and press Enter to add, or select from dropdown." => Some("ui-literal-1629"),
        "Missing a repo?" => Some("ui-literal-1630"),
        "Configure access on GitHub" => Some("ui-literal-1631"),
        "No repositories found" => Some("ui-literal-1632"),
        "Open image at {docker_hub_url}" => Some("ui-literal-1633"),
        "e.g., Zach's external models" => Some("ui-literal-1634"),
        "Please include 'https://'" => Some("ui-literal-1635"),
        "e.g., sk-..." => Some("ui-literal-1636"),
        "e.g., GLM-5-FP8" => Some("ui-literal-1637"),
        "e.g., GLM-5" => Some("ui-literal-1638"),
        "Provide your endpoint details below. You can add as many models from the endpoint as you'd like and can also provide aliases for the model picker in your input." => Some("ui-literal-1639"),
        "+ Add model" => Some("ui-literal-1640"),
        "Add endpoint" => Some("ui-literal-1641"),
        "Are you sure you want to transfer team ownership to {}? You will no longer be the owner and will not be able to take any administrative actions for this team." => Some("ui-literal-1642"),
        "{discount}% off" => Some("ui-literal-1643"),
        "Overage spending limit" => Some("ui-literal-1644"),
        "Monthly spending limit" => Some("ui-literal-1645"),
        "New agent" => Some("ui-literal-1646"),
        "Buy more" => Some("ui-literal-1647"),
        "Monthly overage spending limit" => Some("ui-literal-1648"),
        "${cost_dollars:.2}" => Some("ui-literal-1649"),
        "${price_dollars:.2}" => Some("ui-literal-1650"),
        "Reloading would exceed your monthly limit. " => Some("ui-literal-1651"),
        "Total overages" => Some("ui-literal-1652"),
        "Kick off an agent task to view usage history here." => Some("ui-literal-1653"),
        " to regain access to AI features." => Some("ui-literal-1654"),
        "Contact your team admin to resolve billing issues." => Some("ui-literal-1655"),
        " for a more flexible pricing model." => Some("ui-literal-1656"),
        " or " => Some("ui-literal-1657"),
        "bring your own key" => Some("ui-literal-1658"),
        " for increased access to AI features." => Some("ui-literal-1659"),
        " for more AI credits." => Some("ui-literal-1660"),
        " for security features like SSO and automatically applied zero data retention." => Some("ui-literal-1661"),
        " for custom limits and dedicated support." => Some("ui-literal-1662"),
        " for more credits and access to more models." => Some("ui-literal-1663"),
        "e.g. ~/code-repos/repo" => Some("ui-literal-1664"),
        "Commands, comma separated" => Some("ui-literal-1665"),
        "e.g. ls .*" => Some("ui-literal-1666"),
        "e.g. rm .*" => Some("ui-literal-1667"),
        "Add MCP servers to extend the Warp Agent's capabilities. \n            MCP servers expose data sources or tools to agents through a standardized interface, essentially acting like plugins. " => Some("ui-literal-1668"),
        ", or " => Some("ui-literal-1669"),
        "aws login" => Some("ui-literal-1670"),
        " to purchase add-on credits." => Some("ui-literal-1671"),
        "Staging IAP credentials" => Some("ui-literal-1672"),
        "No environments match your search." => Some("ui-literal-1674"),
        "Width %" => Some("ui-literal-1677"),
        "Height %" => Some("ui-literal-1678"),
        "Delete environment?" => Some("ui-literal-1679"),
        "Configure whether Warp attempts to “Warpify” (add support for blocks, \n                    input modes, etc) certain shells. " => Some("ui-literal-1680"),
        " {count}" => Some("ui-literal-1681"),
        "This setting is managed by your organization." => Some("ui-literal-1682"),
        "Select directory" => Some("ui-literal-1683"),
        "Select a git repository to enable worktree support" => Some("ui-literal-1684"),
        "Automatically create a worktree when opening a new tab" => Some("ui-literal-1685"),
        "You must select that you want to automatically create a \n                         worktree in order to select this" => Some("ui-literal-1686"),
        "Auto-generate worktree branch name" => Some("ui-literal-1687"),
        "Create your first tab config" => Some("ui-literal-1688"),
        "Default: {default_value}" => Some("ui-literal-1689"),
        "This tab config will be permanently deleted. This action cannot be undone." => Some("ui-literal-1690"),
        "New worktree" => Some("ui-literal-1691"),
        "Autogenerate worktree branch name" => Some("ui-literal-1692"),
        "Make default" => Some("ui-literal-1693"),
        "Already the default" => Some("ui-literal-1694"),
        "Copy transcript to clipboard" => Some("ui-literal-1695"),
        "Character limit exceeded." => Some("ui-literal-1696"),
        "{buffer_len} / {PROMPT_CHARACTER_LIMIT}" => Some("ui-literal-1697"),
        "Credits used: {num_requests_used} / {request_limit}." => Some("ui-literal-1698"),
        "{next_refresh_time} until refresh." => Some("ui-literal-1699"),
        "utils_tests.rs" => Some("ui-literal-1700"),
        "Rules are custom prompts that describe when to use a specific model. Warp intelligently matches your tasks against these rules." => Some("ui-literal-1701"),
        "Rules are matched top to bottom — rules higher in the list take precedence over those below." => Some("ui-literal-1702"),
        " routing chooses a model based on Warp's classification of the task's difficulty." => Some("ui-literal-1703"),
        " routing chooses a model based on custom prompts." => Some("ui-literal-1704"),
        "Describe when to use this model…" => Some("ui-literal-1705"),
        "Created by {} • {}" => Some("ui-literal-1706"),
        "Environment setup commands" => Some("ui-literal-1707"),
        "Environment details" => Some("ui-literal-1708"),
        "Name: {environment_name}" => Some("ui-literal-1709"),
        "Save and auto-sync this plan to your Warp Drive" => Some("ui-literal-1710"),
        "Change git branch" => Some("ui-literal-1711"),
        "View pull request" => Some("ui-literal-1712"),
        "Change working directory" => Some("ui-literal-1713"),
        "Working directory" => Some("ui-literal-1714"),
        "Install nvm to enable version switching" => Some("ui-literal-1715"),
        "This menu helps you switch between Node.js versions — but it requires nvm to be installed." => Some("ui-literal-1716"),
        "No node versions installed" => Some("ui-literal-1717"),
        "Try installing versions with nvm" => Some("ui-literal-1718"),
        "Close Welcome Tips" => Some("ui-literal-1719"),
        "Items in the trash will be deleted forever after 30 days." => Some("ui-literal-1720"),
        "Syncing Warp Drive" => Some("ui-literal-1721"),
        "{banner_line_1} {SHARED_OBJECT_LIMIT_HIT_BANNER_LINE}" => Some("ui-literal-1722"),
        "{PAYMENT_ISSUE_BANNER_LINE_1} {banner_line_2}" => Some("ui-literal-1723"),
        "dir   " => Some("ui-literal-1724"),
        "executable   " => Some("ui-literal-1725"),
        "Restore notebook from trash" => Some("ui-literal-1726"),
        "Copy notebook contents into your personal workspace" => Some("ui-literal-1727"),
        "Copy to Personal" => Some("ui-literal-1728"),
        "Copy notebook contents to your clipboard" => Some("ui-literal-1729"),
        "Copy All" => Some("ui-literal-1730"),
        "Refresh notebook" => Some("ui-literal-1731"),
        "From GitHub" => Some("ui-literal-1732"),
        "{outdated_count} outdated" => Some("ui-literal-1733"),
        "Search diff sets or branches to compare…" => Some("ui-literal-1734"),
        "No matches" => Some("ui-literal-1735"),
        "Hide file navigation" => Some("ui-literal-1736"),
        "Show file navigation" => Some("ui-literal-1737"),
        "Cannot detect diffs for this folder" => Some("ui-literal-1738"),
        "As you or the Agent make changes, you'll be able to track them here." => Some("ui-literal-1739"),
        "Repo is initialized with a {file_name} file." => Some("ui-literal-1740"),
        "Binary file - no diff available" => Some("ui-literal-1741"),
        "File renamed without changes" => Some("ui-literal-1742"),
        "Unable to load file content" => Some("ui-literal-1743"),
        "No file selected" => Some("ui-literal-1744"),
        "No files to discard" => Some("ui-literal-1745"),
        "Don't show me again" => Some("ui-literal-1746"),
        "Cycle suggestions" => Some("ui-literal-1747"),
        "alt-cmdorctrl-[" => Some("ui-literal-1748"),
        "alt-cmdorctrl-]" => Some("ui-literal-1749"),
        "Search files and directories" => Some("ui-literal-1750"),
        "error inserting text: {error}" => Some("ui-literal-1751"),
        "Include unstaged" => Some("ui-literal-1752"),
        "Commit message" => Some("ui-literal-1753"),
        "{total_files} {}" => Some("ui-literal-1754"),
        "Included commits" => Some("ui-literal-1755"),
        "PR successfully created." => Some("ui-literal-1756"),
        "Insert block" => Some("ui-literal-1757"),
        "Link (web or file)" => Some("ui-literal-1758"),
        "Apply link" => Some("ui-literal-1759"),
        "Preparing..." => Some("ui-literal-1760"),
        "Live session started at {} on {}" => Some("ui-literal-1761"),
        "Enter custom Docker image name:" => Some("ui-literal-1762"),
        "e.g. https://us.api.openai.com/v1 for a regional endpoint" => Some("ui-literal-1763"),
        "AWS Region:" => Some("ui-literal-1764"),
        "AWS Access Key ID:" => Some("ui-literal-1765"),
        "Getting started with Oz cloud agents" => Some("ui-literal-1766"),
        "Start Oz cloud agents directly in Warp from an integration (Linear, Slack), with an event (GitHub, built-in schedule), or programmatically with the Oz SDK or CLI." => Some("ui-literal-1767"),
        "Check out the " => Some("ui-literal-1768"),
        " to learn more." => Some("ui-literal-1769"),
        "Quick start: Visit oz.warp.dev for a UI-based setup experience." => Some("ui-literal-1770"),
        "Manual setup: Create a Slack or Linear integration with the Oz CLI" => Some("ui-literal-1771"),
        "Create an environment" => Some("ui-literal-1772"),
        "First, set up an environment to create an integration." => Some("ui-literal-1773"),
        "Or, supply your own existing docker image." => Some("ui-literal-1774"),
        "Create an integration" => Some("ui-literal-1775"),
        "Choose your agent" => Some("ui-literal-1776"),
        "Loading cloud agent runs" => Some("ui-literal-1777"),
        "Loading agents..." => Some("ui-literal-1778"),
        "No results matched your filters" => Some("ui-literal-1779"),
        "View details" => Some("ui-literal-1780"),
        "Manage privacy settings" => Some("ui-literal-1781"),
        "Cancel summarization" => Some("ui-literal-1782"),
        "Continue summarization" => Some("ui-literal-1783"),
        "Cancel summarization?" => Some("ui-literal-1784"),
        "Summarization is already running. If you cancel now, the request may still incur cost, any progress so far will be lost, and restarting will take longer.\n\nAre you sure you want to cancel?" => Some("ui-literal-1785"),
        "The matrix theme is now available at" => Some("ui-literal-1786"),
        "Follow up with existing conversation" => Some("ui-literal-1787"),
        "OpenAI automatically applies long-context pricing when context exceeds 272,000 tokens. " => Some("ui-literal-1788"),
        "(no response body: {err:#})" => Some("ui-literal-1789"),
        "Context window" => Some("ui-literal-1790"),
        "The base model's working memory — how many tokens of your conversation, code, and documents it can consider at once. Larger windows enable longer conversations and more coherent responses over bigger codebases, at the cost of higher latency and compute usage." => Some("ui-literal-1791"),
        "Plan auto-sync" => Some("ui-literal-1792"),
        "The plans this agent creates will be automatically added and synced to Warp Drive." => Some("ui-literal-1793"),
        "Call web tools" => Some("ui-literal-1794"),
        "The agent may use web search when helpful for completing tasks." => Some("ui-literal-1795"),
        "e.g. \"YOLO code\"" => Some("ui-literal-1796"),
        "Untitled conversation" => Some("ui-literal-1797"),
        "is valid markdown" => Some("ui-literal-1798"),
        "Run your agent task in an isolated cloud environment." => Some("ui-literal-1799"),
        "Use cloud agents to run parallel agents, build agents that run autonomously, and check in on your agents from anywhere. " => Some("ui-literal-1800"),
        "RECENT ACTIVITY" => Some("ui-literal-1801"),
        "1 update" => Some("ui-literal-1802"),
        "{pct:.precision$}%" => Some("ui-literal-1803"),
        "Includes other request context and temporary instructions added to help the agent better respond." => Some("ui-literal-1804"),
        "use your own API keys" => Some("ui-literal-1805"),
        "Add credits" => Some("ui-literal-1806"),
        "No URLs fetched" => Some("ui-literal-1807"),
        "Agent location" => Some("ui-literal-1808"),
        "Running `{}`..." => Some("ui-literal-1809"),
        "AWS credentials expired or missing" => Some("ui-literal-1810"),
        "Failed to authenticate with AWS Bedrock when using {}. \n                     Run `{}` to refresh credentials." => Some("ui-literal-1811"),
        "Always run automatically" => Some("ui-literal-1812"),
        "Agents ({})" => Some("ui-literal-1813"),
        "Manage suggested code banner settings" => Some("ui-literal-1814"),
        "Settings > AI" => Some("ui-literal-1815"),
        "No URLs found" => Some("ui-literal-1816"),
        "{prefix} " => Some("ui-literal-1817"),
        "Your profile is set to always ask for permission to execute commands." => Some("ui-literal-1818"),
        "Error: {e}" => Some("ui-literal-1819"),
        "Type your answer and press Enter" => Some("ui-literal-1820"),
        "{error_message} " => Some("ui-literal-1821"),
        "Authenticate GitHub" => Some("ui-literal-1822"),
        "Cloud agent run cancelled" => Some("ui-literal-1823"),
        "Manage Agent permissions" => Some("ui-literal-1824"),
        "{verb} " => Some("ui-literal-1825"),
        "{target_kind} " => Some("ui-literal-1826"),
        ": {q}" => Some("ui-literal-1827"),
        "Resume conversation" => Some("ui-literal-1828"),
        "This suggestion is being edited in another tab." => Some("ui-literal-1829"),
        "Grep for " => Some("ui-literal-1830"),
        "Grepping for " => Some("ui-literal-1831"),
        " in {display_path} cancelled" => Some("ui-literal-1832"),
        " in {display_path}" => Some("ui-literal-1833"),
        "Cancelled grep for the following patterns in {display_path}" => Some("ui-literal-1834"),
        "Grep for the following patterns in {display_path}" => Some("ui-literal-1835"),
        "Grepping for the following patterns in {display_path}" => Some("ui-literal-1836"),
        "Search for files that match " => Some("ui-literal-1837"),
        "Finding files that match " => Some("ui-literal-1838"),
        " in {path} cancelled" => Some("ui-literal-1839"),
        " in {path}" => Some("ui-literal-1840"),
        "Cancelled search for files that match the following patterns in {path}" => Some("ui-literal-1841"),
        "Find files that match the following patterns in {path}" => Some("ui-literal-1842"),
        "Finding files that match the following patterns in {path}" => Some("ui-literal-1843"),
        "Comment addressed: \"{content}\"" => Some("ui-literal-1844"),
        "Good response" => Some("ui-literal-1845"),
        "Bad response" => Some("ui-literal-1846"),
        "Continue conversation" => Some("ui-literal-1847"),
        "Show credit usage details" => Some("ui-literal-1848"),
        "Debug output" => Some("ui-literal-1849"),
        "Sending message to " => Some("ui-literal-1850"),
        ": {subject}" => Some("ui-literal-1851"),
        "Started agent " => Some("ui-literal-1852"),
        ": {error}" => Some("ui-literal-1853"),
        " cancelled." => Some("ui-literal-1854"),
        "{WARP_GLYPH} " => Some("ui-literal-1855"),
        "Ask the agent to check this command now, skipping its timer." => Some("ui-literal-1857"),
        "Provided API key is not valid" => Some("ui-literal-1858"),
        "Failed to authenticate with {provider} when using {model_name}. \n                     Double-check that your API key is correct." => Some("ui-literal-1859"),
        "Send Feedback" => Some("ui-literal-1860"),
        "Debug information: {debug_info}" => Some("ui-literal-1861"),
        "Install the Warp plugin to enable rich agent notifications within Warp" => Some("ui-literal-1862"),
        "Remote-server tarball download failed with status {status}: {body}" => Some("ui-literal-1863"),
        "Restore environment variables from trash" => Some("ui-literal-1864"),
        "Close Env Var Collection" => Some("ui-literal-1865"),
        "I'm looking for..." => Some("ui-literal-1866"),
        "Example queries" => Some("ui-literal-1867"),
        "Loading results..." => Some("ui-literal-1868"),
        "Code symbols indexing..." => Some("ui-literal-1869"),
        "{} · Tab {}" => Some("ui-literal-1870"),
        "Fork current conversation" => Some("ui-literal-1871"),
        "Create a file named {}…" => Some("ui-literal-1872"),
        "Slash command: {}" => Some("ui-literal-1873"),
        "e.g. \"Google API Key\"" => Some("ui-literal-1874"),
        "\\bAIza[0-9A-Za-z-_]{35}\\b" => Some("ui-literal-1875"),
        "Name (optional)" => Some("ui-literal-1876"),
        "Regex pattern" => Some("ui-literal-1877"),
        "Add regex" => Some("ui-literal-1878"),
        "Invalid regex" => Some("ui-literal-1879"),
        "Warp API Key" => Some("ui-literal-1880"),
        "This secret key is shown only once. Copy and store it securely." => Some("ui-literal-1881"),
        "Creating…" => Some("ui-literal-1882"),
        "Create key" => Some("ui-literal-1883"),
        "No agents available. Create one first." => Some("ui-literal-1884"),
        "Create agent" => Some("ui-literal-1885"),
        "Executable path" => Some("ui-literal-1886"),
        "Grace period (seconds)" => Some("ui-literal-1887"),
        "Directory path" => Some("ui-literal-1888"),
        "Install {name}" => Some("ui-literal-1889"),
        "No MCP server selected" => Some("ui-literal-1890"),
        "Update {name}" => Some("ui-literal-1891"),
        "No updates available" => Some("ui-literal-1892"),
        "Warp will prevent use of premium models when this dollar limit is reached. Resets on a monthly basis." => Some("ui-literal-1893"),
        "Note that AI credits made near your chosen limit may exceed it by a few dollars." => Some("ui-literal-1894"),
        "This is an automated agent on your team." => Some("ui-literal-1895"),
        "({} credits)" => Some("ui-literal-1896"),
        "Limit: {}" => Some("ui-literal-1897"),
        " {trailing_copy}" => Some("ui-literal-1898"),
        "Other team members' usage across add-on, pay-as-you-go, and cloud-only credits." => Some("ui-literal-1899"),
        "Change role" => Some("ui-literal-1900"),
        "Copy text or cancel active process" => Some("ui-literal-1901"),
        "Attach Selection as Agent Context" => Some("ui-literal-1902"),
        "Copy error" => Some("ui-literal-1903"),
        "New terminal session" => Some("ui-literal-1904"),
        "{description} " => Some("ui-literal-1905"),
        "Waiting for password input" => Some("ui-literal-1906"),
        " to " => Some("ui-literal-1907"),
        "File Uploads" => Some("ui-literal-1908"),
        "Hide details" => Some("ui-literal-1909"),
        "Show details" => Some("ui-literal-1910"),
        "Do not show again" => Some("ui-literal-1911"),
        "Customizable in appearance settings." => Some("ui-literal-1912"),
        "Offline, trying to reconnect..." => Some("ui-literal-1913"),
        "Request edit access" => Some("ui-literal-1914"),
        "You're viewing a snapshot" => Some("ui-literal-1915"),
        "This shared conversation shows the state when you opened it. \n                 If the agent is still running, refresh to see the latest progress." => Some("ui-literal-1916"),
        "Re-generate AGENTS.md file" => Some("ui-literal-1917"),
        "View index status" => Some("ui-literal-1918"),
        "Skip for now" => Some("ui-literal-1919"),
        "Delete secret" => Some("ui-literal-1920"),
        "Start a new Oz cloud agent" => Some("ui-literal-1921"),
        "Use Oz cloud agents to run parallel agents, build agents that run autonomously, and check in on your agents from anywhere. " => Some("ui-literal-1922"),
        "Cloud agents require an environment that they'll run in to get their task done. Create your first environment below. You'll be able to edit the environment later, or add new environments when you need them." => Some("ui-literal-1923"),
        "Search secrets or create a new one" => Some("ui-literal-1924"),
        "No secrets found. Save to use this value directly or click the key to add a secret." => Some("ui-literal-1925"),
        "Your credentials are encrypted end-to-end. " => Some("ui-literal-1926"),
        "Your agent is currently running on a {} machine. " => Some("ui-literal-1927"),
        " for more powerful cloud agents." => Some("ui-literal-1928"),
        "Failed to start environment" => Some("ui-literal-1929"),
        "Please authenticate with GitHub to continue" => Some("ui-literal-1930"),
        "Authenticate with GitHub" => Some("ui-literal-1931"),
        "Your Warp admin has enabled AWS Bedrock for your team." => Some("ui-literal-1932"),
        "The AWS CLI is required to authenticate with your organization's AWS Bedrock. Install it to continue." => Some("ui-literal-1933"),
        "The output from Warp's initialization script is visible above to assist with debugging." => Some("ui-literal-1934"),
        "Continue sharing" => Some("ui-literal-1935"),
        "Are you still there?" => Some("ui-literal-1936"),
        "No code to be restored" => Some("ui-literal-1937"),
        "{}% off!" => Some("ui-literal-1938"),
        "{display_name} is not available for free users. " => Some("ui-literal-1939"),
        "Project Skill" => Some("ui-literal-1940"),
        "Cancel request" => Some("ui-literal-1941"),
        "Make Editor" => Some("ui-literal-1942"),
        "Start sharing" => Some("ui-literal-1943"),
        "Got unexpected message from shared session viewer websocket" => Some("ui-literal-1944"),
        "alias name" => Some("ui-literal-1945"),
        "Add a workflow argument" => Some("ui-literal-1946"),
        "Add environment variables" => Some("ui-literal-1947"),
        "Project explorer unavailable" => Some("ui-literal-1948"),
        "Comment imported from GitHub" => Some("ui-literal-1949"),
        "Line number:Column" => Some("ui-literal-1950"),
        "Go to line" => Some("ui-literal-1951"),
        "Set up a reusable starting point for your tabs. Pick a repo, choose a session type, and optionally attach a worktree. Use it whenever you want to open a tab with this setup." => Some("ui-literal-1952"),
        "Switch back to horizontal tabs" => Some("ui-literal-1953"),
        "Meet your new agent inbox" => Some("ui-literal-1954"),
        "Warp pipes through notifications from any CLI coding agent into a unified notification center that works across all coding agents and harnesses. " => Some("ui-literal-1955"),
        "Introducing universal agent support: level up any coding agent with Warp" => Some("ui-literal-1956"),
        "View options" => Some("ui-literal-1957"),
        "No tabs match your search." => Some("ui-literal-1958"),
        "+ {hidden_count} more" => Some("ui-literal-1959"),
        "View as" => Some("ui-literal-1960"),
        "Tab item" => Some("ui-literal-1961"),
        "Bring your own AI" => Some("ui-literal-1962"),
        "View pricing" => Some("ui-literal-1963"),
        "Learn more on our " => Some("ui-literal-1964"),
        "pricing page" => Some("ui-literal-1965"),
        "You’re out of credits" => Some("ui-literal-1966"),
        "To continue using AI, please upgrade your plan." => Some("ui-literal-1967"),
        "Access to " => Some("ui-literal-1968"),
        "Reload Credits" => Some("ui-literal-1969"),
        "Extended cloud agents access" => Some("ui-literal-1970"),
        "Upgrade plan" => Some("ui-literal-1971"),
        "Close panel" => Some("ui-literal-1972"),
        "Use Codex models in Warp" => Some("ui-literal-1973"),
        "Codex is OpenAI's most advanced agentic coding model for real-world engineering." => Some("ui-literal-1974"),
        "Use Codex directly in Oz and leverage \n            features like in-app code review, agent session sharing and file editing." => Some("ui-literal-1975"),
        "Infinitely scalable coding agent — run in local sessions or in the cloud." => Some("ui-literal-1976"),
        "No conversations yet" => Some("ui-literal-1977"),
        "Your active and past conversations with local and ambient agents will appear here." => Some("ui-literal-1978"),
        "No matching conversations" => Some("ui-literal-1979"),
        "Run Connection Lost" => Some("ui-literal-1980"),
        "Give Warp the option to automatically move active local agents to the cloud when \n             your computer sleeps." => Some("ui-literal-1981"),
        "Search in files" => Some("ui-literal-1982"),
        "You, our community, can participate in building Warp using an agent-first workflow." => Some("ui-literal-1983"),
        "Orchestrate any agent, anywhere" => Some("ui-literal-1984"),
        "We've made major improvements to Warp's cloud agent orchestration platform, Oz." => Some("ui-literal-1985"),
        "Reset to Warp defaults" => Some("ui-literal-1986"),
        "Some settings will take effect when you open a new session." => Some("ui-literal-1987"),
        "No font family" => Some("ui-literal-1988"),
        "{spacing}{view_name} ({view_id:?})" => Some("ui-literal-1989"),
        "{} {chevron}" => Some("ui-literal-1990"),
        "Sign in to continue" => Some("ui-literal-1991"),
        "Open {uri} in your browser" => Some("ui-literal-1992"),
        "and enter code: {code}" => Some("ui-literal-1993"),
        "Starting terminal…" => Some("ui-literal-1995"),
        "Login failed: {message}" => Some("ui-literal-1996"),
        "Choose if you'd like to use Warp Agent or third party agents." => Some("ui-literal-1997"),
        "State of the art agent harness deeply integrated into the terminal." => Some("ui-literal-1998"),
        "Use agents like Claude Code, Codex, and Gemini." => Some("ui-literal-1999"),
        "A modern terminal with state of the art agents built in." => Some("ui-literal-2000"),
        "Save with a recurring plan, or explore Warp's AI before committing." => Some("ui-literal-2001"),
        "Starting at $18 / mo, available with monthly or annual plans. Includes base credits, \n             frontier models, cloud agents, collaboration, and more." => Some("ui-literal-2002"),
        "Explore Warp's built-in AI features before committing to a plan, or bring your own \n             inference." => Some("ui-literal-2003"),
        "Tailor your features and UI to your working style." => Some("ui-literal-2004"),
        "Click or use arrow keys to select, Enter to confirm." => Some("ui-literal-2005"),
        "Sync light/dark theme with OS" => Some("ui-literal-2006"),
        "How do you want to work?" => Some("ui-literal-2007"),
        "Get AI features to accelerate terminal and agent-driven workflows:" => Some("ui-literal-2008"),
        "A modern terminal optimized for speed, context, and control without AI." => Some("ui-literal-2009"),
        "Select your Warp Agent's defaults." => Some("ui-literal-2010"),
        "Autonomy settings are configured as part of your team workspace." => Some("ui-literal-2011"),
        "Select defaults for using agents like Claude Code, Codex, and Gemini." => Some("ui-literal-2012"),
        "Open in terminal session" => Some("ui-literal-2013"),
        "Edit Markdown file" => Some("ui-literal-2014"),
        "again to start new conversation" => Some("ui-literal-2015"),
        "to resume conversation" => Some("ui-literal-2016"),
        "open conversation" => Some("ui-literal-2017"),
        "for code review" => Some("ui-literal-2018"),
        "to fork and continue" => Some("ui-literal-2019"),
        " new pane" => Some("ui-literal-2020"),
        " new tab" => Some("ui-literal-2021"),
        "to hide help" => Some("ui-literal-2022"),
        "autodetected shell command, " => Some("ui-literal-2023"),
        " to override" => Some("ui-literal-2024"),
        "to exit shell mode" => Some("ui-literal-2025"),
        "Starting shell..." => Some("ui-literal-2026"),
        "autodetected shell command" => Some("ui-literal-2027"),
        "for help" => Some("ui-literal-2028"),
        "for commands" => Some("ui-literal-2029"),
        "send task to the cloud" => Some("ui-literal-2030"),
        "to hand off to cloud" => Some("ui-literal-2031"),
        "to dismiss" => Some("ui-literal-2032"),
        "start a new agent conversation" => Some("ui-literal-2033"),
        "start a new cloud agent conversation" => Some("ui-literal-2034"),
        "switch model" => Some("ui-literal-2035"),
        "go back to terminal" => Some("ui-literal-2036"),
        "to index this codebase and generate an AGENTS.md for optimal performance" => Some("ui-literal-2037"),
        "prev" => Some("ui-literal-2038"),
        "next" => Some("ui-literal-2039"),
        " new conversation" => Some("ui-literal-2040"),
        " plan with agent" => Some("ui-literal-2041"),
        " to continue conversation" => Some("ui-literal-2042"),
        " new /agent conversation" => Some("ui-literal-2043"),
        "/agent for new conversation" => Some("ui-literal-2044"),
        " to execute" => Some("ui-literal-2045"),
        " to send" => Some("ui-literal-2046"),
        " (autodetected) " => Some("ui-literal-2047"),
        "again to send to agent" => Some("ui-literal-2048"),
        "couldn't find original conversation directory " => Some("ui-literal-2049"),
        " change repos" => Some("ui-literal-2050"),
        "changed directory to continue conversation " => Some("ui-literal-2051"),
        "cycle past commands and conversations" => Some("ui-literal-2052"),
        "open code review" => Some("ui-literal-2053"),
        "autodetect agent prompts in terminal sessions" => Some("ui-literal-2054"),
        " to navigate" => Some("ui-literal-2055"),
        " to cycle tabs" => Some("ui-literal-2056"),
        " cd to repo" => Some("ui-literal-2057"),
        " open plan" => Some("ui-literal-2058"),
        "cannot start new conversation while terminal command is running" => Some("ui-literal-2059"),
        "rewind" => Some("ui-literal-2060"),
        " to select" => Some("ui-literal-2061"),
        " select and save to profile" => Some("ui-literal-2062"),
        " continue in this pane" => Some("ui-literal-2063"),
        " to remove" => Some("ui-literal-2064"),
        "selected text attached as context" => Some("ui-literal-2065"),
        "No skills found" => Some("ui-literal-2066"),
        "Input your shell command, press enter to execute. Press cmd-up to navigate to output of previously executed commands. Press cmd-l to re-focus command input." => Some("ui-literal-2067"),
        "Send a prompt below to start a new conversation" => Some("ui-literal-2068"),
        "Warp anything e.g. Deploy my React app to Vercel and set up environment variables" => Some("ui-literal-2069"),
        "Warp anything e.g. Help me debug why my Python tests are failing in CI" => Some("ui-literal-2070"),
        "Warp anything e.g. Set up a new microservice with Docker and create the deployment pipeline" => Some("ui-literal-2071"),
        "Warp anything e.g. Find and fix the memory leak in my Node.js application" => Some("ui-literal-2072"),
        "Warp anything e.g. Create a backup script for my PostgreSQL database and schedule it" => Some("ui-literal-2073"),
        "Warp anything e.g. Help me migrate my data from MySQL to PostgreSQL" => Some("ui-literal-2074"),
        "Warp anything e.g. Set up monitoring and alerts for my AWS infrastructure" => Some("ui-literal-2075"),
        "Warp anything e.g. Build a REST API for my mobile app using FastAPI" => Some("ui-literal-2076"),
        "Warp anything e.g. Help me optimize my SQL queries that are running slowly" => Some("ui-literal-2077"),
        "Warp anything e.g. Create a GitHub Actions workflow to automatically deploy on merge" => Some("ui-literal-2078"),
        "Warp anything e.g. Set up Redis caching for my web application" => Some("ui-literal-2079"),
        "Warp anything e.g. Help me troubleshoot why my Kubernetes pods keep crashing" => Some("ui-literal-2080"),
        "Warp anything e.g. Build a data pipeline to process CSV files and load them into BigQuery" => Some("ui-literal-2081"),
        "Warp anything e.g. Set up SSL certificates and configure HTTPS for my domain" => Some("ui-literal-2082"),
        "Warp anything e.g. Help me refactor this legacy code to use modern design patterns" => Some("ui-literal-2083"),
        "Warp anything e.g. Create unit tests for my authentication service" => Some("ui-literal-2084"),
        "Warp anything e.g. Set up log aggregation with ELK stack for my distributed system" => Some("ui-literal-2085"),
        "Warp anything e.g. Help me implement OAuth2 authentication in my Express.js app" => Some("ui-literal-2086"),
        "Warp anything e.g. Optimize my Docker images to reduce build times and size" => Some("ui-literal-2087"),
        "Warp anything e.g. Set up A/B testing infrastructure for my web application" => Some("ui-literal-2088"),
        " to dismiss" => Some("ui-literal-2089"),
        "/open-repo" => Some("ui-literal-2090"),
        "Steer the running agent, or backspace to exit" => Some("ui-literal-2091"),
        "Queue a follow up for the running agent, or backspace to exit" => Some("ui-literal-2092"),
        "Ask a follow up, or backspace to exit" => Some("ui-literal-2093"),
        "Agent management panel" => Some("ui-literal-2094"),
        "Runs" => Some("ui-literal-2095"),
        "Status" => Some("ui-literal-2096"),
        "Source" => Some("ui-literal-2097"),
        "Created on" => Some("ui-literal-2098"),
        "Has artifact" => Some("ui-literal-2099"),
        "Harness" => Some("ui-literal-2100"),
        "Environment" => Some("ui-literal-2101"),
        "Created by" => Some("ui-literal-2102"),
        "Working" => Some("ui-literal-2103"),
        "Failed" => Some("ui-literal-2104"),
        "Last 24 hours" => Some("ui-literal-2105"),
        "Past 3 days" => Some("ui-literal-2106"),
        "Last week" => Some("ui-literal-2107"),
        "Pull Request" => Some("ui-literal-2108"),
        "Screenshot" => Some("ui-literal-2109"),
        "File" => Some("ui-literal-2110"),
        "None" => Some("ui-literal-2111"),
        "Session expired" => Some("ui-literal-2112"),
        "No session available" => Some("ui-literal-2113"),
        "Sessions expire after one week and cannot be opened." => Some("ui-literal-2114"),
        "Executor" => Some("ui-literal-2115"),
        "Run time" => Some("ui-literal-2116"),
        _ => None,
    }
}

/// Converts plain name/value pairs into Fluent formatting arguments.
///
/// # Parameters
/// - `args`: Fluent argument name/value pairs.
///
/// # Returns
/// Fluent argument collection ready for `format_pattern`.
fn build_args<'a>(args: &'a [(&str, String)]) -> FluentArgs<'a> {
    let mut fluent_args = FluentArgs::with_capacity(args.len());
    for (key, value) in args {
        fluent_args.set(*key, value.as_str());
    }
    fluent_args
}

/// Builds a Fluent bundle for one language from embedded resource files.
///
/// # Parameters
/// - `language`: Language used for the bundle locale identifier and diagnostics.
/// - `resources`: Embedded Fluent resource sources to register in order.
///
/// # Returns
/// Fluent bundle containing all messages for the target language.
fn build_bundle(language: Language, resources: &[&str]) -> FluentBundle<FluentResource> {
    let lang_id: LanguageIdentifier = language
        .locale()
        .parse()
        .expect("bundled locale identifiers must be valid");
    let mut bundle = FluentBundle::new(vec![lang_id]);

    for source in resources {
        let resource = FluentResource::try_new((*source).to_owned())
            .expect("bundled Fluent resources must parse");
        if let Err(errors) = bundle.add_resource(resource) {
            panic!(
                "failed to add bundled Fluent resource for {}: {errors:?}",
                language.locale()
            );
        }
    }

    bundle
}
