use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const VERSION: u32 = 1;
const MAX_BYTES: u64 = 1024 * 1024;
const MAX_SAVED_LAYOUTS: usize = 12;
const DEFAULT_TITLE: &str = "Overview";
const LEGACY_TITLE: &str = "Your Mac, at a glance";
/// The ranges the recorder offers, in seconds. Samples are kept for the longest
/// one, so changing the range never discards history.
pub const HISTORY_RANGES: [u32; 3] = [60, 300, 900];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    #[default]
    Processor,
    Memory,
    Graphics,
    Network,
    Storage,
    Battery,
    Processes,
}
impl SectionKind {
    pub const ALL: [Self; 7] = [
        Self::Processor,
        Self::Memory,
        Self::Graphics,
        Self::Network,
        Self::Storage,
        Self::Battery,
        Self::Processes,
    ];
    pub fn title(&self) -> &'static str {
        match self {
            Self::Processor => "Processor",
            Self::Memory => "Memory",
            Self::Graphics => "Graphics",
            Self::Network => "Network",
            Self::Storage => "Storage",
            Self::Battery => "Battery & power",
            Self::Processes => "Apps & processes",
        }
    }
    pub fn description(&self) -> &'static str {
        match self {
            Self::Processor => "How much work your Mac is doing",
            Self::Memory => "Space your apps are using right now",
            Self::Graphics => "Work used to draw screens, video and games",
            Self::Network => "Data your Mac is receiving and sending",
            Self::Storage => "Space available on your drives",
            Self::Battery => "Charge, power and cooling information",
            Self::Processes => "Apps and background tasks using your Mac's resources",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Span {
    #[default]
    Half,
    Full,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SectionConfig {
    pub kind: SectionKind,
    pub title: String,
    pub description: String,
    pub visible: bool,
    pub span: Span,
    pub graph_height: f32,
    pub details: bool,
}
impl SectionConfig {
    pub fn new(kind: SectionKind) -> Self {
        Self {
            kind,
            title: kind.title().into(),
            description: kind.description().into(),
            visible: true,
            span: if matches!(kind, SectionKind::Processes | SectionKind::Battery) {
                Span::Half
            } else {
                Span::Full
            },
            graph_height: 64.0,
            details: false,
        }
    }
}
impl Default for SectionConfig {
    fn default() -> Self {
        Self::new(SectionKind::Processor)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    #[default]
    Comfortable,
    Compact,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuBarReading {
    #[default]
    Icon,
    Cpu,
    Memory,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontChoice {
    #[default]
    System,
    Monospace,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub name: String,
    pub background: [u8; 3],
    pub surface: [u8; 3],
    pub text: [u8; 3],
    pub muted: [u8; 3],
    pub divider: [u8; 3],
    pub accent: [u8; 3],
    pub cpu: [u8; 3],
    pub memory: [u8; 3],
    pub gpu: [u8; 3],
    pub network: [u8; 3],
    pub storage: [u8; 3],
    pub battery: [u8; 3],
    pub danger: [u8; 3],
    pub font: FontChoice,
    pub font_size: f32,
}
impl Theme {
    pub fn dark() -> Self {
        Self {
            name: "Graphite".into(),
            background: [27, 28, 31],
            surface: [33, 34, 38],
            text: [236, 237, 240],
            muted: [164, 166, 174],
            divider: [54, 56, 62],
            accent: [64, 196, 190],
            cpu: [64, 159, 227],
            memory: [33, 178, 131],
            gpu: [210, 149, 42],
            network: [159, 128, 238],
            storage: [226, 98, 142],
            battery: [134, 177, 58],
            danger: [255, 107, 98],
            font: FontChoice::System,
            font_size: 13.0,
        }
    }
    pub fn light() -> Self {
        Self {
            name: "Paper".into(),
            background: [241, 241, 244],
            surface: [255, 255, 255],
            text: [29, 30, 34],
            muted: [92, 95, 104],
            divider: [224, 225, 230],
            accent: [8, 122, 130],
            cpu: [31, 120, 200],
            memory: [14, 138, 98],
            gpu: [178, 106, 0],
            network: [122, 85, 214],
            storage: [198, 63, 120],
            battery: [95, 138, 18],
            danger: [200, 50, 43],
            ..Self::dark()
        }
    }
    pub fn black() -> Self {
        Self {
            name: "Black".into(),
            background: [0, 0, 0],
            surface: [14, 14, 16],
            text: [240, 241, 244],
            muted: [160, 162, 170],
            divider: [40, 41, 46],
            ..Self::dark()
        }
    }
    pub fn midnight() -> Self {
        Self {
            name: "Midnight".into(),
            background: [14, 20, 33],
            surface: [20, 28, 44],
            text: [230, 236, 248],
            muted: [150, 165, 190],
            divider: [38, 48, 68],
            ..Self::dark()
        }
    }
    pub fn ember() -> Self {
        Self {
            name: "Ember".into(),
            background: [26, 23, 22],
            surface: [35, 31, 30],
            text: [245, 239, 230],
            muted: [177, 162, 151],
            divider: [62, 52, 47],
            accent: [242, 160, 105],
            ..Self::dark()
        }
    }
    /// Colors of the presets shipped before the recorder redesign. A saved theme
    /// that still matches one has not been customized, so it can move to the
    /// current preset without touching anyone's own colors.
    fn migrated_preset(&self) -> Option<Self> {
        type Legacy = ([u8; 3], [u8; 3], [u8; 3], [u8; 3]);
        const GRAPHITE: Legacy = ([19, 23, 28], [26, 31, 38], [43, 51, 61], [131, 171, 248]);
        const EARLY_GRAPHITE: Legacy = ([20, 21, 24], [29, 30, 34], [49, 51, 58], [159, 143, 255]);
        const PAPER: Legacy = (
            [242, 243, 246],
            [255, 255, 255],
            [216, 219, 227],
            [107, 80, 206],
        );
        const MIDNIGHT: Legacy = ([13, 20, 31], [19, 29, 44], [35, 49, 68], [112, 176, 242]);
        let colors = (self.background, self.surface, self.divider, self.accent);
        let preset = match self.name.as_str() {
            "Graphite" if colors == GRAPHITE || colors == EARLY_GRAPHITE => Self::dark(),
            "Paper" if colors == PAPER => Self::light(),
            "Midnight" if colors == MIDNIGHT => Self::midnight(),
            _ => return None,
        };
        Some(Self {
            font: self.font,
            font_size: self.font_size,
            ..preset
        })
    }
    fn normalize(&mut self) {
        if let Some(theme) = self.migrated_preset() {
            *self = theme;
        }
        self.name = bounded_name(&self.name, "Custom theme");
        self.font_size = finite_clamp(self.font_size, 11.0, 20.0, 13.0);
    }
}
impl Default for Theme {
    fn default() -> Self {
        Self::black()
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NamedLayout {
    pub name: String,
    pub sections: Vec<SectionConfig>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub version: u32,
    pub dashboard_title: String,
    pub dashboard_subtitle: String,
    pub custom_font_path: Option<PathBuf>,
    pub theme: Theme,
    pub density: Density,
    pub refresh_secs: u64,
    pub smooth_charts: bool,
    pub history_secs: u32,
    pub sections: Vec<SectionConfig>,
    pub saved_layouts: Vec<NamedLayout>,
    pub previous_layout: Option<Vec<SectionConfig>>,
    /// Follow the system's light or dark setting with Paper and Graphite.
    pub follow_system: bool,
    /// Left, top, width and height of the window when it last moved.
    pub window: Option<[f32; 4]>,
    pub menu_bar_reading: MenuBarReading,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            version: VERSION,
            dashboard_title: DEFAULT_TITLE.into(),
            dashboard_subtitle: "Understand what is using your Mac’s resources.".into(),
            custom_font_path: None,
            theme: Theme::default(),
            density: Density::default(),
            refresh_secs: 2,
            smooth_charts: false,
            history_secs: 300,
            sections: default_sections(),
            saved_layouts: Vec::new(),
            previous_layout: None,
            follow_system: false,
            window: None,
            menu_bar_reading: MenuBarReading::default(),
        }
    }
}
impl Preferences {
    pub fn normalize(&mut self) {
        self.version = VERSION;
        if self.dashboard_title == LEGACY_TITLE {
            self.dashboard_title = DEFAULT_TITLE.into();
        }
        self.dashboard_title = bounded_name(&self.dashboard_title, DEFAULT_TITLE);
        self.dashboard_subtitle = bounded_text(
            &self.dashboard_subtitle,
            "Understand what is using your Mac’s resources.",
            200,
        );
        self.theme.normalize();
        self.refresh_secs = self.refresh_secs.clamp(1, 10);
        self.history_secs = *HISTORY_RANGES
            .iter()
            .min_by_key(|range| range.abs_diff(self.history_secs))
            .unwrap_or(&300);
        self.window = self
            .window
            .filter(|w| w.iter().all(|v| v.is_finite()) && w[2] >= 900. && w[3] >= 600.);
        normalize_sections(&mut self.sections);
        migrate_stock_layout(&mut self.sections);
        if let Some(previous) = &mut self.previous_layout {
            normalize_sections(previous);
        }
        let mut names = HashSet::new();
        self.saved_layouts.retain_mut(|layout| {
            layout.name = bounded_name(&layout.name, "My layout");
            if !names.insert(layout.name.to_lowercase()) {
                return false;
            }
            normalize_sections(&mut layout.sections);
            true
        });
    }
    pub fn save_named_layout(&mut self, name: &str) -> Result<(), String> {
        let name: String = name.trim().chars().take(80).collect();
        if name.is_empty() {
            return Err("Enter a name for your layout.".into());
        }
        let key = name.to_lowercase();
        let existing = self.saved_layouts.iter().position(|layout| {
            layout
                .name
                .trim()
                .chars()
                .take(80)
                .collect::<String>()
                .to_lowercase()
                == key
        });
        if existing.is_none() && self.saved_layouts.len() >= MAX_SAVED_LAYOUTS {
            return Err("All 12 saved layout slots are in use. Replace a layout by using its name, or delete one first.".into());
        }
        let mut sections = self.sections.clone();
        normalize_sections(&mut sections);
        let layout = NamedLayout { name, sections };
        if let Some(index) = existing {
            self.saved_layouts[index] = layout;
        } else {
            self.saved_layouts.push(layout);
        }
        Ok(())
    }
    pub fn remember_layout(&mut self) {
        self.previous_layout = Some(self.sections.clone());
    }
    pub fn restore_previous_layout(&mut self) {
        if let Some(previous) = &mut self.previous_layout {
            std::mem::swap(&mut self.sections, previous);
        }
    }
    pub fn load_named_layout(&mut self, index: usize) {
        if let Some(layout) = self.saved_layouts.get(index) {
            let sections = layout.sections.clone();
            self.remember_layout();
            self.sections = sections;
        }
    }
    pub fn reset_layout(&mut self) {
        self.remember_layout();
        self.sections = default_sections();
    }
    pub fn apply_preset(&mut self, name: &str) {
        if !matches!(name, "Balanced" | "Focus" | "Compact") {
            return;
        }
        self.remember_layout();
        match name {
            "Balanced" => {
                self.sections = default_sections();
                self.density = Density::Comfortable;
            }
            "Focus" => {
                self.density = Density::Comfortable;
                self.sections = [
                    SectionKind::Processor,
                    SectionKind::Memory,
                    SectionKind::Processes,
                    SectionKind::Graphics,
                    SectionKind::Network,
                    SectionKind::Storage,
                    SectionKind::Battery,
                ]
                .into_iter()
                .map(|kind| {
                    let mut section = SectionConfig::new(kind);
                    section.visible = matches!(
                        kind,
                        SectionKind::Processor | SectionKind::Memory | SectionKind::Processes
                    );
                    section.span = Span::Full;
                    section
                })
                .collect();
            }
            "Compact" => {
                self.sections = default_sections();
                for section in &mut self.sections {
                    section.graph_height = 44.0;
                    section.details = false;
                }
                self.density = Density::Compact;
            }
            _ => {}
        }
    }
}
fn default_sections() -> Vec<SectionConfig> {
    [
        SectionKind::Processor,
        SectionKind::Memory,
        SectionKind::Graphics,
        SectionKind::Network,
        SectionKind::Storage,
        SectionKind::Processes,
        SectionKind::Battery,
    ]
    .into_iter()
    .map(SectionConfig::new)
    .collect()
}
/// Moves a layout nobody has arranged onto the recorder. It must still have the old
/// default order, spans and wording, which the current defaults do not use, so a
/// layout arranged since then is never touched. Chart height is kept, as it may
/// have come from a density preset.
fn migrate_stock_layout(sections: &mut Vec<SectionConfig>) {
    let stock = sections.len() == SectionKind::ALL.len()
        && sections
            .iter()
            .zip(SectionKind::ALL)
            .all(|(section, kind)| {
                let title = match kind {
                    SectionKind::Network => "Internet & network",
                    SectionKind::Battery => "Battery & cooling",
                    _ => kind.title(),
                };
                let span = if kind == SectionKind::Processes {
                    Span::Full
                } else {
                    Span::Half
                };
                section.kind == kind
                    && section.title == title
                    && section.description == kind.description()
                    && section.span == span
                    && section.visible
                    && !section.details
            });
    if !stock {
        return;
    }
    let height = sections[0].graph_height;
    let uniform = sections.iter().all(|s| s.graph_height == height);
    *sections = default_sections();
    if uniform && height < 64. {
        for section in sections.iter_mut() {
            section.graph_height = height;
        }
    }
}
fn finite_clamp(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}
fn bounded_name(value: &str, fallback: &str) -> String {
    bounded_text(value, fallback, 80)
}
fn bounded_text(value: &str, fallback: &str, limit: usize) -> String {
    let name: String = value.trim().chars().take(limit).collect();
    if name.is_empty() {
        fallback.into()
    } else {
        name
    }
}
fn normalize_sections(sections: &mut Vec<SectionConfig>) {
    let mut seen = HashSet::new();
    sections.retain_mut(|section| {
        if !seen.insert(section.kind) {
            return false;
        }
        section.title = bounded_name(&section.title, section.kind.title());
        section.description = bounded_text(&section.description, section.kind.description(), 200);
        section.graph_height = finite_clamp(section.graph_height, 40.0, 240.0, 64.0);
        true
    });
    for kind in SectionKind::ALL {
        if seen.insert(kind) {
            sections.push(SectionConfig::new(kind));
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Store {
    path: PathBuf,
}
impl Store {
    pub fn new() -> Self {
        let directory = std::env::var_os("SYSLY_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("Library/Application Support/Sysly")
            });
        Self::at(directory.join("preferences.json"))
    }
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn load(&self) -> (Preferences, Option<String>) {
        match read_preferences(&self.path) {
            Ok(mut preferences) => {
                preferences.normalize();
                (preferences, None)
            }
            Err(error) if !self.path.exists() => (
                Preferences::default(),
                if error.kind() == std::io::ErrorKind::NotFound {
                    None
                } else {
                    Some(error.to_string())
                },
            ),
            Err(error) => (
                Preferences::default(),
                Some(format!(
                    "Could not read settings: {error}. Defaults are in use; the original will be backed up before saving."
                )),
            ),
        }
    }
    pub fn save(&self, preferences: &Preferences) -> Result<(), String> {
        if self.path.exists() && read_preferences(&self.path).is_err() {
            let backup = sibling(&self.path, "backup");
            fs::copy(&self.path, &backup).map_err(|error| {
                format!(
                    "Could not preserve original settings at {}: {error}",
                    backup.display()
                )
            })?;
        }
        let mut preferences = preferences.clone();
        preferences.normalize();
        write_json(&self.path, &preferences)
    }
}
impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}
fn read_preferences(path: &Path) -> std::io::Result<Preferences> {
    let bytes = read_bounded(path)?;
    let preferences: Preferences = serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
    if preferences.version != VERSION {
        return Err(std::io::Error::other(format!(
            "Unsupported settings version {} (this app supports {VERSION})",
            preferences.version
        )));
    }
    Ok(preferences)
}
fn read_bounded(path: &Path) -> std::io::Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(std::io::Error::other("File exceeds the 1 MiB limit"));
    }
    Ok(bytes)
}
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.{timestamp}.{suffix}", std::process::id()));
    path.with_file_name(name)
}
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Export exceeds the 1 MiB limit".into());
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = sibling(path, "tmp");
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| format!("Could not write {}: {error}", path.display()))
}
#[derive(Serialize, Deserialize)]
struct ThemeFile {
    kind: String,
    version: u32,
    theme: Theme,
}
#[derive(Serialize, Deserialize)]
struct LayoutFile {
    kind: String,
    version: u32,
    sections: Vec<SectionConfig>,
}
fn validate_envelope(kind: &str, version: u32, expected: &str) -> Result<(), String> {
    if kind != expected {
        return Err(format!("Expected a {expected} file, found {kind}"));
    }
    if version != VERSION {
        return Err(format!(
            "Unsupported {expected} version {version}; expected {VERSION}"
        ));
    }
    Ok(())
}
pub fn import_theme(path: &Path) -> Result<Theme, String> {
    let bytes = read_bounded(path).map_err(|error| error.to_string())?;
    let mut file: ThemeFile =
        serde_json::from_slice(&bytes).map_err(|error| format!("Invalid theme: {error}"))?;
    validate_envelope(&file.kind, file.version, "sysly_theme")?;
    file.theme.normalize();
    Ok(file.theme)
}
pub fn export_theme(path: &Path, theme: &Theme) -> Result<(), String> {
    let mut theme = theme.clone();
    theme.normalize();
    write_json(
        path,
        &ThemeFile {
            kind: "sysly_theme".into(),
            version: VERSION,
            theme,
        },
    )
}
pub fn import_layout(path: &Path) -> Result<Vec<SectionConfig>, String> {
    let bytes = read_bounded(path).map_err(|error| error.to_string())?;
    let mut file: LayoutFile =
        serde_json::from_slice(&bytes).map_err(|error| format!("Invalid layout: {error}"))?;
    validate_envelope(&file.kind, file.version, "sysly_layout")?;
    normalize_sections(&mut file.sections);
    Ok(file.sections)
}
pub fn export_layout(path: &Path, sections: &[SectionConfig]) -> Result<(), String> {
    let mut sections = sections.to_vec();
    normalize_sections(&mut sections);
    write_json(
        path,
        &LayoutFile {
            kind: "sysly_layout".into(),
            version: VERSION,
            sections,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = sibling(&std::env::temp_dir().join("sysly-preferences-test"), "dir");
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn legacy_sections() -> Vec<SectionConfig> {
        SectionKind::ALL
            .into_iter()
            .map(|kind| {
                let mut section = SectionConfig::new(kind);
                section.span = if kind == SectionKind::Processes {
                    Span::Full
                } else {
                    Span::Half
                };
                section.graph_height = 48.;
                match kind {
                    SectionKind::Network => section.title = "Internet & network".into(),
                    SectionKind::Battery => section.title = "Battery & cooling".into(),
                    _ => {}
                }
                section
            })
            .collect()
    }

    #[test]
    fn stock_layouts_move_to_the_recorder_and_stay_there() {
        let mut preferences = Preferences {
            sections: legacy_sections(),
            ..Preferences::default()
        };
        preferences.normalize();
        assert_eq!(preferences.sections[0].span, Span::Full);
        assert_eq!(preferences.sections[0].graph_height, 48.);
        assert_eq!(preferences.sections[3].title, "Network");
        assert_eq!(preferences.sections[5].kind, SectionKind::Processes);
        let once = preferences.sections.clone();
        preferences.normalize();
        assert_eq!(preferences.sections, once);
    }

    #[test]
    fn arranged_layouts_are_left_alone() {
        let mut renamed = legacy_sections();
        renamed[0].title = "Chip".into();
        let mut moved = legacy_sections();
        moved.swap(0, 1);
        let mut hidden = legacy_sections();
        hidden[2].visible = false;
        for sections in [renamed, moved, hidden] {
            let mut preferences = Preferences {
                sections: sections.clone(),
                ..Preferences::default()
            };
            preferences.normalize();
            assert_eq!(preferences.sections, sections);
        }
    }

    #[test]
    fn new_installs_start_on_black_and_saved_themes_stay() {
        let preferences = Preferences::default();
        assert_eq!(preferences.theme, Theme::black());
        assert_eq!(preferences.theme.background, [0, 0, 0]);
        assert!(!preferences.follow_system);

        let temp = Temp::new();
        let store = Store::at(temp.0.join("preferences.json"));
        let saved = Preferences {
            theme: Theme::dark(),
            ..Preferences::default()
        };
        fs::write(&store.path, serde_json::to_vec(&saved).unwrap()).unwrap();
        assert_eq!(store.load().0.theme, Theme::dark());
    }

    #[test]
    fn only_untouched_stock_themes_are_migrated() {
        let mut stock = Theme {
            background: [20, 21, 24],
            surface: [29, 30, 34],
            divider: [49, 51, 58],
            accent: [159, 143, 255],
            font_size: 15.,
            ..Theme::dark()
        };
        stock.normalize();
        assert_eq!(stock.background, Theme::dark().background);
        assert_eq!(stock.font_size, 15.);
        let mut custom = Theme {
            background: [1, 2, 3],
            ..Theme::dark()
        };
        custom.normalize();
        assert_eq!(custom.background, [1, 2, 3]);
    }

    #[test]
    fn customizations_survive_storage_and_exports() {
        let temp = Temp::new();
        let store = Store::at(temp.0.join("preferences.json"));
        assert_eq!(store.load(), (Preferences::default(), None));
        let mut preferences = Preferences::default();
        preferences.sections.swap(0, 5);
        preferences.sections[0].title = "My battery".into();
        preferences.sections[0].span = Span::Full;
        preferences.sections[0].graph_height = 175.0;
        preferences.theme = Theme::midnight();
        preferences.theme.text = [190, 210, 230];
        preferences.theme.font = FontChoice::Monospace;
        preferences.saved_layouts.push(NamedLayout {
            name: "Daily".into(),
            sections: preferences.sections.clone(),
        });
        store.save(&preferences).unwrap();
        assert_eq!(store.load(), (preferences.clone(), None));
        let theme_path = temp.0.join("theme.json");
        export_theme(&theme_path, &preferences.theme).unwrap();
        assert_eq!(import_theme(&theme_path).unwrap(), preferences.theme);
        let layout_path = temp.0.join("layout.json");
        export_layout(&layout_path, &preferences.sections).unwrap();
        assert_eq!(import_layout(&layout_path).unwrap(), preferences.sections);
    }
    #[test]
    fn version_one_defaults_to_sample_updates_and_persists_smoothing() {
        let temp = Temp::new();
        let path = temp.0.join("preferences.json");
        let store = Store::at(path.clone());
        let mut legacy = serde_json::to_value(Preferences::default()).unwrap();
        legacy.as_object_mut().unwrap().remove("smooth_charts");
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();

        let (mut preferences, notice) = store.load();
        assert_eq!(notice, None);
        assert_eq!(preferences.version, 1);
        assert!(!preferences.smooth_charts);
        for enabled in [true, false] {
            preferences.smooth_charts = enabled;
            store.save(&preferences).unwrap();
            assert_eq!(store.load(), (preferences.clone(), None));
            let stored: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(stored["version"], 1);
            assert_eq!(stored["smooth_charts"], enabled);
        }
    }
    #[test]
    fn invalid_and_future_settings_are_preserved() {
        for original in [b"not json".to_vec(), br#"{"version":42}"#.to_vec()] {
            let temp = Temp::new();
            let path = temp.0.join("preferences.json");
            fs::write(&path, &original).unwrap();
            let store = Store::at(path.clone());
            let (preferences, notice) = store.load();
            assert!(notice.is_some());
            assert_eq!(fs::read(&path).unwrap(), original);
            store.save(&preferences).unwrap();
            assert_eq!(store.load(), (Preferences::default(), None));
            let backups: Vec<_> = fs::read_dir(&temp.0)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "backup"))
                .collect();
            assert_eq!(backups.len(), 1);
            assert_eq!(fs::read(&backups[0]).unwrap(), original);
            store.save(&preferences).unwrap();
            assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 2);
        }
    }
    #[test]
    fn normalize_repairs_bounds_and_preserves_hidden_order() {
        let mut preferences = Preferences {
            refresh_secs: 0,
            history_secs: 5000,
            sections: vec![
                SectionConfig::new(SectionKind::Memory),
                SectionConfig::new(SectionKind::Memory),
            ],
            ..Preferences::default()
        };
        preferences.theme.font_size = f32::NAN;
        preferences.sections[0].title = "é".repeat(100);
        preferences.sections[0].graph_height = f32::INFINITY;
        preferences.sections[0].visible = false;
        preferences.saved_layouts = (0..20)
            .map(|index| NamedLayout {
                name: format!("Layout {index}"),
                sections: Vec::new(),
            })
            .collect();
        preferences
            .saved_layouts
            .insert(0, preferences.saved_layouts[0].clone());
        preferences.normalize();
        assert_eq!(preferences.refresh_secs, 1);
        assert_eq!(preferences.history_secs, 900);
        assert_eq!(preferences.theme.font_size, 13.0);
        assert_eq!(preferences.sections.len(), 7);
        assert_eq!(preferences.sections[0].kind, SectionKind::Memory);
        assert!(!preferences.sections[0].visible);
        assert_eq!(preferences.sections[0].title.chars().count(), 80);
        assert_eq!(preferences.sections[0].graph_height, 64.0);
        assert_eq!(preferences.saved_layouts.len(), 20);
        for section in &mut preferences.sections {
            section.visible = false;
        }
        preferences.normalize();
        assert!(preferences.sections.iter().all(|section| !section.visible));
        preferences.reset_layout();
        assert!(preferences.sections.iter().all(|section| section.visible));
    }
    #[test]
    fn automatic_layout_backups_never_evict_named_layouts() {
        let mut preferences = Preferences::default();
        for index in 0..12 {
            preferences.sections[0].graph_height = 50.0 + index as f32;
            preferences
                .save_named_layout(&format!("Layout {index}"))
                .unwrap();
        }
        let saved = preferences.saved_layouts.clone();
        preferences.apply_preset("Focus");
        preferences.remember_layout();
        preferences.load_named_layout(5);
        assert_eq!(preferences.sections, saved[5].sections);
        preferences.reset_layout();
        preferences.normalize();
        assert_eq!(preferences.saved_layouts, saved);
    }
    #[test]
    fn layout_names_replace_consistently_across_storage() {
        let temp = Temp::new();
        let store = Store::at(temp.0.join("preferences.json"));
        let mut preferences = Preferences::default();
        preferences.save_named_layout("Daily").unwrap();
        preferences.sections.swap(0, 3);
        preferences.sections[0].graph_height = 181.0;
        preferences.save_named_layout(" daily ").unwrap();
        assert_eq!(preferences.saved_layouts.len(), 1);
        assert_eq!(preferences.saved_layouts[0].name, "daily");
        assert_eq!(preferences.saved_layouts[0].sections, preferences.sections);
        store.save(&preferences).unwrap();
        assert_eq!(store.load(), (preferences, None));
    }
    #[test]
    fn full_layout_capacity_rejects_new_names_without_mutation() {
        let mut preferences = Preferences::default();
        for index in 0..12 {
            preferences
                .save_named_layout(&format!("Layout {index}"))
                .unwrap();
        }
        let saved = preferences.saved_layouts.clone();
        assert!(
            preferences
                .save_named_layout("One more")
                .unwrap_err()
                .contains("12")
        );
        assert!(preferences.save_named_layout("   ").is_err());
        assert_eq!(preferences.saved_layouts, saved);
        preferences.sections[0].title = "Replacement".into();
        preferences.save_named_layout(" LAYOUT 0 ").unwrap();
        assert_eq!(preferences.saved_layouts.len(), 12);
        assert_eq!(
            preferences.saved_layouts[0].sections[0].title,
            "Replacement"
        );
    }
    #[test]
    fn previous_layout_restores_geometry_and_survives_storage() {
        let temp = Temp::new();
        let store = Store::at(temp.0.join("preferences.json"));
        let mut preferences = Preferences::default();
        preferences.sections.swap(0, 5);
        preferences.sections[0].span = Span::Full;
        preferences.sections[0].graph_height = 173.0;
        preferences.sections[1].visible = false;
        let original = preferences.sections.clone();
        preferences.apply_preset("Compact");
        let compact = preferences.sections.clone();
        store.save(&preferences).unwrap();
        let (mut loaded, notice) = store.load();
        assert_eq!(notice, None);
        loaded.restore_previous_layout();
        assert_eq!(loaded.sections, original);
        loaded.restore_previous_layout();
        assert_eq!(loaded.sections, compact);
    }
    #[test]
    fn version_one_without_previous_layout_remains_compatible() {
        let mut json = serde_json::to_value(Preferences::default()).unwrap();
        json.as_object_mut().unwrap().remove("previous_layout");
        let mut loaded: Preferences = serde_json::from_value(json).unwrap();
        assert!(loaded.previous_layout.is_none());
        loaded.previous_layout = Some(vec![]);
        loaded.normalize();
        assert_eq!(
            loaded.previous_layout.unwrap().len(),
            SectionKind::ALL.len()
        );
    }
    #[test]
    fn imports_reject_wrong_versions_colors_and_oversized_files() {
        let temp = Temp::new();
        let path = temp.0.join("import.json");
        for contents in [
            r#"{"kind":"sysly_theme","version":2,"theme":{}}"#,
            r#"{"kind":"sysly_theme","version":1,"theme":{"text":[256,0,0]}}"#,
            r#"{"kind":"sysly_layout","version":1,"sections":[]}"#,
        ] {
            fs::write(&path, contents).unwrap();
            assert!(import_theme(&path).is_err());
        }
        fs::write(&path, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
        assert!(import_layout(&path).unwrap_err().contains("1 MiB"));
    }
}
