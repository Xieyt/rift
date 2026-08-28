use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::bail;
use regex::RegexBuilder;
pub use rift_protocol::{AnimationEasing, ConfigCommand, LayoutMode, WorkspaceSelector};
use serde::{Deserialize, Serialize};

use super::collections::HashMap;
use crate::actor::wm_controller::WmCommand;
use crate::sys::hotkey::{Hotkey, HotkeySpec};

pub const MAX_WORKSPACES: usize = 128;

// TODO: when to remove these?
const DEPRECATED_MAP: &[(&str, &str)] = &[
    ("stack_windows", "toggle_stack"),
    ("unstack_windows", "toggle_stack"),
    ("toggle_tile_orientation", "toggle_orientation"),
];

// Keep the non-panicking home dir: upstream still does `.unwrap()` here, which
// aborts the WM when $HOME is unset (FORK.md §8 stability item 2). ConfigCommand
// itself has moved to rift-protocol.
fn home_dir() -> PathBuf { dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp")) }
pub fn data_dir() -> PathBuf { home_dir().join(".rift") }
pub fn restore_file() -> PathBuf { data_dir().join("layout.ron") }
pub fn config_file() -> PathBuf { home_dir().join(".config").join("rift").join("config.toml") }

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct VirtualWorkspaceSettings {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "default_workspace_count")]
    pub default_workspace_count: usize,
    #[serde(default = "yes")]
    pub auto_assign_windows: bool,
    #[serde(default = "yes")]
    pub preserve_focus_per_workspace: bool,
    #[serde(default = "no")]
    pub workspace_auto_back_and_forth: bool,
    #[serde(default, alias = "prevent_wrapping_around")]
    pub prevent_wrapping: bool,
    #[serde(default = "default_workspace_names")]
    pub workspace_names: Vec<String>,
    #[serde(default)]
    pub default_workspace: usize,
    #[serde(default)]
    pub reapply_app_rules_on_title_change: bool,
    #[serde(default)]
    pub app_rules: Vec<AppWorkspaceRule>,
    #[serde(default)]
    pub workspace_rules: Vec<WorkspaceLayoutRule>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceLayoutRule {
    /// Target workspace by index or name
    pub workspace: WorkspaceSelector,
    /// Layout mode to use for this workspace
    pub layout: LayoutMode,
}

// Allow specifying a workspace by numeric index or by name in the config.
// This supports both `workspace = 2` and `workspace = "coding"` in app rules.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct AppWorkspaceRule {
    /// Application bundle identifier (e.g., "com.apple.Terminal")
    pub app_id: Option<String>,
    /// Target workspace index (0 based) OR workspace name. If None, window goes to active workspace.
    pub workspace: Option<WorkspaceSelector>,
    /// Whether windows should be floating in this workspace
    #[serde(default)]
    pub floating: bool,
    /// Initial normalized position for a floating window. `(0, 0)` is the top-left
    /// and `(1, 1)` is the bottom-right of the available screen area.
    pub position: Option<AppRulePosition>,
    /// Preferred window size in logical pixels.
    pub size: Option<AppRuleSize>,
    /// Focus the window after applying this rule, switching virtual workspaces if needed.
    #[serde(default)]
    pub focus: bool,
    /// An explicit management override. `false` makes the window invisible to Rift;
    /// `true` overrides normal manageability heuristics for a visible window. When
    /// omitted, the matching rule leaves Rift's normal manageability decision intact.
    #[serde(default)]
    pub manage: Option<bool>,
    /// Optional: Application name pattern (alternative to app_id)
    pub app_name: Option<String>,
    /// Optional: Regular expression to match window title (applies to window.title)
    ///
    /// If present, this regex will be used when attempting to match a window by
    /// title.
    pub title_regex: Option<String>,
    /// Optional: Substring to search for in window title (applies to window.title)
    ///
    /// If present, rift will internally treat this as a substring match and will
    /// construct a regex to match titles containing this substring. This allows
    /// people who don't want to write full regexes to match by a simple substring.
    pub title_substring: Option<String>,

    /// Optional: Accessibility role to match (AXRole). If present, it must be a
    /// non-empty string and will be compared against the accessibility role
    /// reported by the AX APIs for a window (exact string match).
    pub ax_role: Option<String>,

    /// Optional: Accessibility subrole to match (AXSubrole). If present, it must be a
    /// non-empty string and will be compared against the accessibility subrole
    /// reported by the AX APIs for a window (exact string match).
    pub ax_subrole: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct AppRulePosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(deny_unknown_fields)]
pub struct AppRuleSize {
    pub w: Option<f64>,
    pub h: Option<f64>,
}

impl Default for VirtualWorkspaceSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            default_workspace_count: default_workspace_count(),
            auto_assign_windows: true,
            preserve_focus_per_workspace: true,
            workspace_auto_back_and_forth: false,
            prevent_wrapping: false,
            workspace_names: default_workspace_names(),
            default_workspace: 0,
            reapply_app_rules_on_title_change: false,
            app_rules: Vec::new(),
            workspace_rules: Vec::new(),
        }
    }
}

impl VirtualWorkspaceSettings {
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.default_workspace_count == 0 {
            issues.push("default_workspace_count must be at least 1".to_string());
        }
        if self.default_workspace_count > MAX_WORKSPACES {
            issues.push(format!(
                "default_workspace_count should not exceed {} for performance reasons",
                MAX_WORKSPACES
            ));
        }

        if self.workspace_names.len() > self.default_workspace_count {
            issues.push("More workspace names provided than default_workspace_count".to_string());
        }

        if self.default_workspace >= self.default_workspace_count {
            issues.push(format!(
                "default_workspace ({}) must be less than default_workspace_count ({})",
                self.default_workspace, self.default_workspace_count
            ));
        }

        // Validate rules and check duplicates in a single pass
        let mut seen_app_ids = crate::common::collections::HashSet::default();
        let mut seen_app_names = crate::common::collections::HashSet::default();
        let mut seen_title_regexes = crate::common::collections::HashSet::default();
        let mut seen_title_substrings = crate::common::collections::HashSet::default();
        let mut seen_ax_roles = crate::common::collections::HashSet::default();
        let mut seen_ax_subroles = crate::common::collections::HashSet::default();

        for (index, rule) in self.app_rules.iter().enumerate() {
            let app_id_empty = rule.app_id.as_ref().map_or(true, |id| id.is_empty());
            if app_id_empty
                && rule.app_name.is_none()
                && rule.title_regex.is_none()
                && rule.title_substring.is_none()
                && rule.ax_role.is_none()
                && rule.ax_subrole.is_none()
            {
                issues.push(format!(
                    "App rule {} has no app_id, app_name, title_regex, or title_substring specified",
                    index
                ));
            }

            if let Some(ref workspace) = rule.workspace {
                if let WorkspaceSelector::Index(idx) = workspace {
                    if *idx >= self.default_workspace_count {
                        issues.push(format!(
                            "App rule {} references workspace {} but only {} workspaces will be created",
                            index, idx, self.default_workspace_count
                        ));
                    }
                }
            }

            if let Some(position) = rule.position {
                if !position.x.is_finite()
                    || !position.y.is_finite()
                    || !(0.0..=1.0).contains(&position.x)
                    || !(0.0..=1.0).contains(&position.y)
                {
                    issues.push(format!(
                        "App rule {} position x and y must be finite values between 0 and 1",
                        index
                    ));
                }
                if !rule.floating {
                    issues.push(format!(
                        "App rule {} specifies position, but position only applies when floating = true",
                        index
                    ));
                }
            }

            if let Some(size) = rule.size {
                if size.w.is_none() && size.h.is_none() {
                    issues.push(format!(
                        "App rule {} size must specify at least one of w or h",
                        index
                    ));
                }
                if size.w.is_some_and(|value| !value.is_finite() || value <= 0.0)
                    || size.h.is_some_and(|value| !value.is_finite() || value <= 0.0)
                {
                    issues.push(format!(
                        "App rule {} size dimensions must be finite positive values",
                        index
                    ));
                }
            }

            if let Some(ref app_id) = rule.app_id {
                if !app_id.is_empty() && !app_id.contains('.') {
                    issues.push(format!(
                        "App rule {} has suspicious app_id '{}' (should be bundle identifier like 'com.example.app')",
                        index, app_id
                    ));
                }

                let has_specific_match = rule.app_name.is_some()
                    || rule.title_regex.is_some()
                    || rule.title_substring.is_some()
                    || rule.ax_role.is_some()
                    || rule.ax_subrole.is_some();
                if !app_id.is_empty() && !has_specific_match && !seen_app_ids.insert(app_id) {
                    issues.push(format!("Duplicate app_id '{}' in rule {}", app_id, index));
                }
            }

            if let Some(ref app_name) = rule.app_name {
                if !seen_app_names.insert(app_name) {
                    issues.push(format!("Duplicate app_name '{}' in rule {}", app_name, index));
                }
            }

            if let Some(ref title_re) = rule.title_regex {
                if title_re.is_empty() {
                    issues.push(format!("App rule {} has empty title_regex", index));
                } else if let Err(error) =
                    RegexBuilder::new(title_re).case_insensitive(true).build()
                {
                    issues.push(format!(
                        "App rule {} has invalid title_regex '{}': {}",
                        index, title_re, error
                    ));
                } else if !seen_title_regexes.insert(title_re) {
                    issues.push(format!("Duplicate title_regex '{}' in rule {}", title_re, index));
                }
            }

            if rule.manage == Some(false)
                && (rule.workspace.is_some()
                    || rule.floating
                    || rule.position.is_some()
                    || rule.size.is_some()
                    || rule.focus)
            {
                issues.push(format!(
                    "App rule {} sets manage = false, so its workspace, floating, position, size, and focus effects are ignored",
                    index
                ));
            }

            if let Some(ref title_sub) = rule.title_substring {
                if title_sub.is_empty() {
                    issues.push(format!("App rule {} has empty title_substring", index));
                } else if !seen_title_substrings.insert(title_sub) {
                    issues.push(format!(
                        "Duplicate title_substring '{}' in rule {}",
                        title_sub, index
                    ));
                }
            }

            if let Some(ref ax_role) = rule.ax_role {
                if ax_role.is_empty() {
                    issues.push(format!("App rule {} has empty ax_role", index));
                } else if !seen_ax_roles.insert(ax_role) {
                    issues.push(format!("Duplicate ax_role '{}' in rule {}", ax_role, index));
                }
            }

            if let Some(ref ax_sub) = rule.ax_subrole {
                if ax_sub.is_empty() {
                    issues.push(format!("App rule {} has empty ax_subrole", index));
                } else if !seen_ax_subroles.insert(ax_sub) {
                    issues.push(format!("Duplicate ax_subrole '{}' in rule {}", ax_sub, index));
                }
            }
        }

        issues
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    settings: Settings,
    keys: HashMap<String, WmCommand>,
    #[serde(default)]
    virtual_workspaces: VirtualWorkspaceSettings,
    /// Modifier combinations that can be reused in key bindings
    /// e.g., "comb1" = "Alt + Shift" allows using "comb1 + C" in keys
    #[serde(default)]
    modifier_combinations: HashMap<String, String>,
}

fn migrate_legacy_resize_bindings(document: &mut toml::Value) -> bool {
    let Some(keys) = document.get_mut("keys").and_then(toml::Value::as_table_mut) else {
        return false;
    };

    let mut migrated = false;
    for (_, command) in keys.iter_mut() {
        let legacy_name = match command.as_str() {
            Some("resize_window_grow") => "resize_window_grow",
            Some("resize_window_shrink") => "resize_window_shrink",
            _ => continue,
        };
        *command = toml::Value::Table(toml::map::Map::from_iter([(
            legacy_name.to_string(),
            toml::Value::String("horizontal".to_string()),
        )]));
        migrated = true;
    }
    migrated
}

fn parse_config_file(buf: &str) -> Result<ConfigFile, toml::de::Error> {
    toml::from_str(buf).or_else(|original_error| {
        let Ok(mut document) = toml::from_str::<toml::Value>(buf) else {
            return Err(original_error);
        };
        if !migrate_legacy_resize_bindings(&mut document) {
            return Err(original_error);
        }
        document.try_into()
    })
}

#[derive(Serialize, Clone, Debug)]
pub struct Config {
    pub settings: Settings,
    pub keys: Vec<(Hotkey, WmCommand)>,
    #[serde(default)]
    pub key_specs: Vec<(String, WmCommand)>,
    pub virtual_workspaces: VirtualWorkspaceSettings,
}

impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Config, D::Error>
    where D: serde::Deserializer<'de> {
        #[derive(Deserialize)]
        struct ConfigSerde {
            settings: Settings,
            keys: Vec<(Hotkey, WmCommand)>,
            #[serde(default)]
            key_specs: Vec<(String, WmCommand)>,
            virtual_workspaces: VirtualWorkspaceSettings,
        }

        let config = ConfigSerde::deserialize(deserializer)?;
        let key_specs = if config.key_specs.is_empty() && !config.keys.is_empty() {
            config
                .keys
                .iter()
                .map(|(hotkey, command)| (hotkey.to_string(), command.clone()))
                .collect()
        } else {
            config.key_specs
        };

        Ok(Config {
            settings: config.settings,
            keys: config.keys,
            key_specs,
            virtual_workspaces: config.virtual_workspaces,
        })
    }
}

unsafe impl Send for Config {}
unsafe impl Sync for Config {}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default = "no")]
    pub animate: bool,
    #[serde(default = "default_animation_duration")]
    pub animation_duration: f64,
    #[serde(default = "default_animation_fps")]
    pub animation_fps: f64,
    #[serde(default)]
    pub animation_easing: AnimationEasing,
    #[serde(default = "yes")]
    pub default_disable: bool,
    #[serde(default = "yes")]
    pub mouse_follows_focus: bool,
    #[serde(default = "yes")]
    pub mouse_hides_on_focus: bool,
    #[serde(default = "yes")]
    pub focus_follows_mouse: bool,
    /// Hotkey that disables focus-follows-mouse while held.
    /// Accepts either a full hotkey (e.g. "Ctrl + A") or a modifier-only spec (e.g. "Ctrl")
    #[serde(default)]
    pub focus_follows_mouse_disable_hotkey: Option<HotkeySpec>,
    /// Apps that should not trigger automatic workspace switching when activated.
    /// List of bundle identifiers (e.g., "com.apple.Spotlight") that often
    /// inappropriately steal focus and shouldn't cause workspace switches.
    #[serde(default)]
    pub auto_focus_blacklist: Vec<String>,
    #[serde(default)]
    pub layout: LayoutSettings,
    #[serde(default)]
    pub ui: UiSettings,
    /// Trackpad gesture settings
    #[serde(default)]
    pub gestures: GestureSettings,

    #[serde(default)]
    pub window_snapping: WindowSnappingSettings,

    /// Commands to run on startup (e.g., for subscribing to events)
    #[serde(default)]
    pub run_on_start: Vec<String>,

    /// Whether to reapply app rules when a window title changes.
    /// Enable hot-reloading of the config file when it changes
    #[serde(default = "yes")]
    pub hot_reload: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct UiSettings {
    #[serde(default)]
    pub menu_bar: MenuBarSettings,
    #[serde(default)]
    pub stack_line: StackLineSettings,
    #[serde(default)]
    pub mission_control: MissionControlSettings,
    #[serde(default)]
    pub hints_bar: HintsBarSettings,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct GestureSettings {
    /// Enable horizontal swipes to switch virtual workspaces
    #[serde(default = "no")]
    pub enabled: bool,
    /// If true, consume horizontal swipe events owned by Rift so macOS and the
    /// foreground app do not also handle them.
    #[serde(default = "yes")]
    pub consume_dock_swipe: bool,
    /// Invert horizontal direction (swap next/prev)
    #[serde(default)]
    pub invert_horizontal_swipe: bool,
    /// Maximum absolute Y delta allowed for the gesture to count as horizontal
    #[serde(default = "default_swipe_vertical_tolerance")]
    pub swipe_vertical_tolerance: f64,
    /// If true, attempt to skip empty workspaces on swipe (if supported)
    #[serde(default)]
    pub skip_empty: bool,
    /// Number of fingers required for swipe (default = 3)
    #[serde(default = "default_swipe_fingers")]
    pub fingers: usize,
    /// Normalized horizontal distance (0..1) required to fire a swipe
    #[serde(default = "default_distance_pct")]
    pub distance_pct: f64,
    /// Enable haptic feedback when a swipe commits
    #[serde(default = "yes")]
    pub haptics_enabled: bool,
    /// Haptic feedback pattern (generic | alignment | level_change)
    #[serde(default)]
    pub haptic_pattern: HapticPattern,
}

impl Default for GestureSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            consume_dock_swipe: true,
            invert_horizontal_swipe: false,
            swipe_vertical_tolerance: default_swipe_vertical_tolerance(),
            skip_empty: true,
            fingers: default_swipe_fingers(),
            distance_pct: default_distance_pct(),
            haptics_enabled: true,
            haptic_pattern: HapticPattern::LevelChange,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default, Copy)]
#[serde(deny_unknown_fields)]
pub struct WindowSnappingSettings {
    #[serde(default = "default_drag_swap_fraction")]
    pub drag_swap_fraction: f64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum MenuBarDisplayMode {
    #[default]
    All,
    Active,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActiveWorkspaceLabel {
    #[default]
    Index,
    Name,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceDisplayStyle {
    #[default]
    Layout,
    Label,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct MenuBarSettings {
    #[serde(default = "no")]
    pub enabled: bool,
    #[serde(default = "no")]
    pub show_empty: bool,
    #[serde(default)]
    pub mode: MenuBarDisplayMode,
    #[serde(default)]
    pub active_label: ActiveWorkspaceLabel,
    #[serde(default)]
    pub display_style: WorkspaceDisplayStyle,
    #[serde(default = "default_layout_folder")]
    pub layout_folder: PathBuf,
}

impl MenuBarSettings {
    pub fn resolved_layout_folder(&self) -> PathBuf {
        let Ok(relative) = self.layout_folder.strip_prefix("~") else {
            return self.layout_folder.clone();
        };
        dirs::home_dir()
            .map(|home| home.join(relative))
            .unwrap_or_else(|| self.layout_folder.clone())
    }
}

impl Default for MenuBarSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            show_empty: false,
            mode: MenuBarDisplayMode::default(),
            active_label: ActiveWorkspaceLabel::default(),
            display_style: WorkspaceDisplayStyle::default(),
            layout_folder: default_layout_folder(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct StackLineSettings {
    #[serde(default = "no")]
    pub enabled: bool,
    #[serde(default)]
    pub hover: StackLineHoverMode,
    #[serde(default = "default_stack_line_thickness")]
    pub thickness: f64,
    /// Outline thickness (in px) of the indicator background and the active-tab
    /// segment. 0 disables the outline.
    #[serde(default = "default_stack_line_border_width")]
    pub border_width: f64,
    #[serde(default)]
    pub horiz_placement: HorizontalPlacement,
    #[serde(default)]
    pub vert_placement: VerticalPlacement,
    /// Distance to position the stack line away from the window edge (in points)
    /// This creates spacing between the window and the stack line
    #[serde(default = "default_stack_line_spacing")]
    pub spacing: f64,
    /// Draw window titles as tab labels on the stack-line indicator
    /// (niri-style tabbed columns). Needs a larger `thickness` to fit text.
    #[serde(default = "no")]
    pub show_titles: bool,
    /// Indicator colors as hex ("#RRGGBB" or "#RRGGBBAA"); unset uses built-in
    /// defaults. `active_color` = focused-tab fill, `inactive_color` = bar
    /// background, `active_title_color` / `inactive_title_color` = tab labels.
    #[serde(default)]
    pub active_color: Option<String>,
    #[serde(default)]
    pub inactive_color: Option<String>,
    #[serde(default)]
    pub active_title_color: Option<String>,
    #[serde(default)]
    pub inactive_title_color: Option<String>,
    /// Title font size in points; unset defaults to ~60% of `thickness` (8–15).
    #[serde(default)]
    pub font_size: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum StackLineHoverMode {
    Click,
    #[default]
    Hover,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct MissionControlSettings {
    #[serde(default = "no")]
    pub enabled: bool,
    #[serde(default = "no")]
    pub fade_enabled: bool,
    #[serde(default = "default_mission_control_fade_duration_ms")]
    pub fade_duration_ms: f64,
}

fn default_mission_control_fade_duration_ms() -> f64 { 180.0 }

fn default_drag_swap_fraction() -> f64 { 0.3 }

fn default_master_stack_ratio() -> f64 { 0.6 }

fn default_master_stack_count() -> usize { 1 }

fn default_scrolling_column_width_ratio() -> f64 { 0.7 }

fn default_scrolling_min_column_width_ratio() -> f64 { 0.3 }

fn default_scrolling_max_column_width_ratio() -> f64 { 0.9 }

fn default_scrolling_width_presets() -> Vec<f64> { vec![0.33, 0.5, 0.66] }

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum HorizontalPlacement {
    #[default]
    Top,
    Bottom,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerticalPlacement {
    #[default]
    Left,
    Right,
}

impl StackLineSettings {
    pub fn thickness(&self) -> f64 { if self.enabled { self.thickness } else { 0.0 } }
}

/// When the scrolling-strip hint bar is shown.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum HintsBarVisibility {
    /// Passive minimap shown whenever a scrolling workspace is active.
    #[default]
    Always,
    /// Hidden until toggled on with the `toggle_hints_bar` command.
    OnDemand,
    /// Hidden, but flashes for `auto_hide_ms` whenever the strip changes.
    Auto,
}

/// How the hint bar occupies vertical space.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum HintsBarPlacement {
    /// Floats over the bottom edge of the tiled windows (no reflow).
    #[default]
    Overlay,
    /// Reserves its own strip; the scrolling tiling area shrinks to fit.
    Reserve,
}

/// Which edge the hint bar sits on.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum HintsBarPosition {
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

impl HintsBarPosition {
    /// True for the left/right edges — the bar stacks columns vertically.
    pub fn is_vertical(&self) -> bool {
        matches!(self, HintsBarPosition::Left | HintsBarPosition::Right)
    }
}

/// How much per-column detail the hint bar renders.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum HintsBarDensity {
    /// Hint letter + app name on one row.
    #[default]
    Compact,
    /// Hint letter, app name, and window title.
    Full,
    /// Position pills only, no text.
    Dots,
}

/// What the bar does when its natural chip row exceeds `max_width_ratio`.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum HintsBarOverflow {
    /// Compress every chip uniformly so the row always fits (current behavior).
    #[default]
    Scale,
    /// Wrap the overflow onto a second row of the same (taller) top bar.
    Row,
    /// Wrap the overflow into a second bar on the opposite edge.
    Bar,
}

/// How the focused multi-window column's detail is shown.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum HintsBarDetailStyle {
    /// A floating card next to the focused chip: icon + name rows.
    #[default]
    Popover,
    /// A second hint-bar-style strip of the focused column's windows, docked to
    /// its own `position` (left/right render vertically, like the main bar).
    Bar,
}

/// Where a bar's content sits along its docked edge.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum HintsBarAlign {
    /// Top (left/right edge) or left (bottom/top edge).
    Start,
    /// Centered along the edge.
    Center,
    /// Bottom (left/right edge) or right (bottom/top edge).
    #[default]
    End,
}

/// How a top bar handles a display notch when it sits on the notch row.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum HintsBarNotch {
    /// Chips flow around it: fill left, skip the notch, resume on the right.
    #[default]
    Flow,
    /// Chips stop before the notch (clamped to the aligned side).
    Stop,
    /// Ignore the notch; chips may render under it.
    Ignore,
}

/// The focused-column detail: how it's shown, and (for `bar`) which edge it
/// docks to, where it sits along that edge, and its padding.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct HintsBarDetailSettings {
    #[serde(default)]
    pub style: HintsBarDetailStyle,
    /// `bar` only: `bottom`/`top` (horizontal) or `left`/`right` (vertical stack).
    #[serde(default)]
    pub position: HintsBarPosition,
    /// `bar` only: where it sits along its edge — `start`/`center`/`end`.
    #[serde(default)]
    pub align: HintsBarAlign,
    /// `bar` only: per-side padding (px) from the display edges.
    #[serde(default)]
    pub pad: HintsBarPad,
}

/// Per-side padding in px (gap from each display edge). Used by the hint bar and
/// its focused-column detail bar.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct HintsBarPad {
    #[serde(default)]
    pub top: f64,
    #[serde(default)]
    pub bottom: f64,
    #[serde(default)]
    pub left: f64,
    #[serde(default)]
    pub right: f64,
}

/// Scrolling-strip hint bar: a horizontal minimap of every column in the
/// active niri-style workspace, on-screen and off. Click a segment to focus
/// that column.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct HintsBarSettings {
    #[serde(default = "no")]
    pub enabled: bool,
    #[serde(default)]
    pub position: HintsBarPosition,
    #[serde(default)]
    pub placement: HintsBarPlacement,
    #[serde(default)]
    pub visibility: HintsBarVisibility,
    #[serde(default)]
    pub density: HintsBarDensity,
    /// Bar height in points.
    #[serde(default = "default_hints_bar_height")]
    pub height: f64,
    /// Auto-hide delay (ms) after the strip last changed, for `visibility = auto`.
    #[serde(default = "default_hints_bar_auto_hide_ms")]
    pub auto_hide_ms: u64,
    /// Home-row keys dealt across the strip left -> right (also the click order).
    #[serde(default = "default_hints_bar_keys")]
    pub keys: String,
    /// Colors as hex ("#RRGGBB" or "#RRGGBBAA"); unset uses built-in defaults.
    /// `background` = bar fill, `focused_color` = focused-column fill,
    /// `viewport_color` = on-screen-region tint, `label_color` / `hint_color`
    /// = app-name and hint-letter text.
    #[serde(default)]
    pub background: Option<String>,
    #[serde(default)]
    pub focused_color: Option<String>,
    #[serde(default)]
    pub viewport_color: Option<String>,
    #[serde(default)]
    pub label_color: Option<String>,
    #[serde(default)]
    pub hint_color: Option<String>,
    /// Text size in points; unset scales to the bar height.
    #[serde(default)]
    pub font_size: Option<f64>,
    /// Background blur radius (frosted glass). 0 disables. Pair with a
    /// translucent `background` alpha (e.g. "#1f1f24aa") for a see-through bar.
    #[serde(default = "default_hints_bar_blur")]
    pub blur: u32,
    /// Show each window's title instead of its app name on the chips/pill.
    #[serde(default = "no")]
    pub show_titles: bool,
    /// Show a leading badge with the active workspace index.
    #[serde(default = "no")]
    pub show_workspace: bool,
    /// Per-side padding (px) insetting the whole bar from the display edges.
    #[serde(default)]
    pub pad: HintsBarPad,
    /// Which end of the bar the chips sit at: `start`/`center`/`end`.
    #[serde(default = "default_align_center")]
    pub align: HintsBarAlign,
    /// How the bar handles a display notch when it sits on the notch row:
    /// `flow` (around it), `stop` (before it), or `ignore`.
    #[serde(default)]
    pub notch: HintsBarNotch,
    /// Fraction of the display width the chip row may fill before overflowing
    /// (1.0 = full width, the default). See `overflow`.
    #[serde(default = "default_hints_bar_max_width_ratio")]
    pub max_width_ratio: f64,
    /// What to do when the chips exceed `max_width_ratio`: `scale` (compress,
    /// default), `row` (second row on the top bar), or `bar` (second bar on the
    /// opposite edge).
    #[serde(default)]
    pub overflow: HintsBarOverflow,
    /// Focused-column detail: `[settings.ui.hints_bar.detail]` — `style`
    /// (`popover`/`bar`), plus `position`/`align`/`pad` for `bar`.
    #[serde(default)]
    pub detail: HintsBarDetailSettings,
}

impl Default for HintsBarSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            position: HintsBarPosition::default(),
            placement: HintsBarPlacement::default(),
            visibility: HintsBarVisibility::default(),
            density: HintsBarDensity::default(),
            height: default_hints_bar_height(),
            auto_hide_ms: default_hints_bar_auto_hide_ms(),
            keys: default_hints_bar_keys(),
            background: None,
            focused_color: None,
            viewport_color: None,
            label_color: None,
            hint_color: None,
            font_size: None,
            blur: default_hints_bar_blur(),
            show_titles: false,
            show_workspace: false,
            pad: HintsBarPad::default(),
            align: HintsBarAlign::Center,
            notch: HintsBarNotch::default(),
            max_width_ratio: default_hints_bar_max_width_ratio(),
            overflow: HintsBarOverflow::default(),
            detail: HintsBarDetailSettings::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum WindowInsertionPoint {
    /// Insert a new window immediately after the current selection.
    #[default]
    NextToSelection,
    /// Append a new window at the end of the layout tree.
    EndOfTree,
}

/// Options understood by every layout system.
///
/// These fields are flattened into both `[settings.layout]` and every
/// per-layout table. A per-layout value overrides the layout-wide value.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct BaseLayoutSettings {
    /// Where newly managed windows are inserted.
    #[serde(default)]
    pub window_insertion_point: Option<WindowInsertionPoint>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct TraditionalLayoutSettings {
    #[serde(flatten)]
    pub base: BaseLayoutSettings,
    /// Use Sway-style sibling normalization when inserting nodes. New nodes receive the
    /// average sibling weight instead of splitting the selected node's share.
    #[serde(default = "yes")]
    pub equalize_nodes: bool,
}

impl Default for TraditionalLayoutSettings {
    fn default() -> Self {
        Self {
            base: BaseLayoutSettings::default(),
            equalize_nodes: true,
        }
    }
}

impl HintsBarSettings {
    /// Vertical space the bar removes from the scrolling tiling area: only
    /// non-zero when enabled AND reserving its own strip.
    pub fn reserved_thickness(&self) -> f64 {
        // Reserve is horizontal-only for now (bottom/top); vertical bars float.
        if self.enabled
            && self.placement == HintsBarPlacement::Reserve
            && !self.position.is_vertical()
        {
            self.height.max(0.0)
        } else {
            0.0
        }
    }
}

fn default_hints_bar_height() -> f64 { 26.0 }
fn default_hints_bar_auto_hide_ms() -> u64 { 1000 }
fn default_hints_bar_keys() -> String { "asdfghjkl;".to_string() }
fn default_hints_bar_blur() -> u32 { 0 }
fn default_hints_bar_max_width_ratio() -> f64 { 1.0 }
fn default_align_center() -> HintsBarAlign { HintsBarAlign::Center }

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct BspLayoutSettings {
    #[serde(flatten)]
    pub base: BaseLayoutSettings,
}
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct LayoutSettings {
    /// Settings inherited by every layout type unless overridden by its table.
    #[serde(flatten)]
    pub base: BaseLayoutSettings,
    /// Layout mode: "traditional", "bsp", "stack", "master_stack", or "scrolling"
    #[serde(default)]
    pub mode: LayoutMode,
    /// Traditional layout configuration
    #[serde(default)]
    pub traditional: TraditionalLayoutSettings,
    /// BSP layout configuration
    #[serde(default)]
    pub bsp: BspLayoutSettings,
    /// Stack system configuration
    #[serde(default)]
    pub stack: StackSettings,
    /// Master/stack layout configuration
    #[serde(default)]
    pub master_stack: MasterStackSettings,
    /// Gap configuration for window spacing
    #[serde(default)]
    pub gaps: GapSettings,
    /// Scrolling layout configuration (niri-style columns)
    #[serde(default)]
    pub scrolling: ScrollingLayoutSettings,
    /// Reserve space (px) at the top / bottom of the tiling area for an external
    /// bar (e.g. sketchybar), so tiled windows never sit under it. Applies to
    /// every layout, independent of the built-in hint bar.
    #[serde(default)]
    pub external_bar_top: f64,
    #[serde(default)]
    pub external_bar_bottom: f64,
    /// Bring the focused window's application to the foreground when focus moves
    /// (keyboard focus changes, workspace switches). Per-command `activate` flags
    /// override this upward. Default: true.
    #[serde(default = "yes")]
    pub activate_on_focus: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct ScrollingLayoutSettings {
    #[serde(flatten)]
    pub base: BaseLayoutSettings,
    /// Whether to animate window transitions in this layout.
    #[serde(default)]
    pub animate: Option<bool>,
    /// Default width of the active column, as a fraction of the screen width.
    #[serde(default = "default_scrolling_column_width_ratio")]
    pub column_width_ratio: f64,
    /// Minimum column width ratio allowed by resize commands.
    #[serde(default = "default_scrolling_min_column_width_ratio")]
    pub min_column_width_ratio: f64,
    /// Maximum column width ratio allowed by resize commands.
    #[serde(default = "default_scrolling_max_column_width_ratio")]
    pub max_column_width_ratio: f64,
    /// Column width ratios cycled by the `cycle_column_width` command
    /// (niri-style preset cycling of the selected column).
    #[serde(default = "default_scrolling_width_presets")]
    pub width_presets: Vec<f64>,
    /// Alignment for the focused column (left, center, right).
    #[serde(default)]
    pub alignment: ScrollingAlignment,
    /// Horizontal focus navigation behavior:
    /// - niri: reveal only as needed based on navigation direction.
    /// - anchored: always align focused column to `alignment`.
    #[serde(default)]
    pub focus_navigation_style: ScrollingFocusNavigationStyle,
    /// Trackpad gestures for scrolling layout
    #[serde(default)]
    pub gestures: ScrollingGestureSettings,
}

impl Default for ScrollingLayoutSettings {
    fn default() -> Self {
        Self {
            base: BaseLayoutSettings::default(),
            animate: None,
            column_width_ratio: default_scrolling_column_width_ratio(),
            min_column_width_ratio: default_scrolling_min_column_width_ratio(),
            max_column_width_ratio: default_scrolling_max_column_width_ratio(),
            width_presets: default_scrolling_width_presets(),
            alignment: ScrollingAlignment::default(),
            focus_navigation_style: ScrollingFocusNavigationStyle::default(),
            gestures: ScrollingGestureSettings::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum MasterStackSide {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScrollingAlignment {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScrollingFocusNavigationStyle {
    #[default]
    Niri,
    Anchored,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct MasterStackSettings {
    #[serde(flatten)]
    pub base: BaseLayoutSettings,
    /// Fraction of space reserved for the master area (0.05..0.95)
    #[serde(default = "default_master_stack_ratio")]
    pub master_ratio: f64,
    /// Number of windows kept in the master area (>= 1)
    #[serde(default = "default_master_stack_count")]
    pub master_count: usize,
    /// Which side the master area occupies
    #[serde(default)]
    pub master_side: MasterStackSide,
    /// Where new windows are inserted when the master area is already full
    #[serde(default = "default_master_stack_new_window_placement")]
    pub new_window_placement: MasterStackNewWindowPlacement,
    /// Orientation arrangement for the master area (override default derived from master_side)
    #[serde(default)]
    pub master_arrangement: Option<crate::layout_engine::Orientation>,
    /// Orientation arrangement for the stack area (override default derived from master_side)
    #[serde(default)]
    pub stack_arrangement: Option<crate::layout_engine::Orientation>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum MasterStackNewWindowPlacement {
    Master,
    Stack,
    Focused,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub struct ScrollingGestureSettings {
    /// Enable horizontal scroll gestures to switch columns
    #[serde(default = "no")]
    pub enabled: bool,
    /// Invert horizontal direction (swap left/right)
    #[serde(default)]
    pub invert_horizontal: bool,
    /// Maximum absolute Y delta allowed for the gesture to count as horizontal
    #[serde(default = "default_swipe_vertical_tolerance")]
    pub vertical_tolerance: f64,
    /// Number of fingers required for scroll gesture
    #[serde(default = "default_swipe_fingers")]
    pub fingers: usize,
    /// Normalized horizontal distance (0..1) required to fire a scroll step
    #[serde(default = "default_distance_pct")]
    pub distance_pct: f64,
    /// Horizontal scroll gesture speed multiplier (>1 = faster strip movement). Default 1.0.
    #[serde(default = "default_scroll_speed")]
    pub scroll_speed: f64,
    /// If true, scrolling past the end of the strip will trigger a workspace switch
    #[serde(default = "no")]
    pub propagate_to_workspace_swipe: bool,
    /// Amount of overscroll (in steps) required to trigger a workspace switch
    #[serde(default = "default_overscroll_threshold")]
    pub workspace_switch_threshold: f64,
}

impl Default for ScrollingGestureSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            invert_horizontal: false,
            vertical_tolerance: default_swipe_vertical_tolerance(),
            fingers: default_swipe_fingers(),
            distance_pct: default_distance_pct(),
            scroll_speed: default_scroll_speed(),
            propagate_to_workspace_swipe: false,
            workspace_switch_threshold: default_overscroll_threshold(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum StackDefaultOrientation {
    Perpendicular,
    Same,
    Horizontal,
    Vertical,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub struct StackSettings {
    #[serde(flatten)]
    pub base: BaseLayoutSettings,
    /// Stack offset - how much each stacked window is offset (in pixels)
    /// With the enhanced stacking system, this creates meaningful visible edges
    /// for each window in the stack while the focused window remains fully visible.
    /// Recommended values: 30-50 pixels for good visibility.
    #[serde(default = "default_stack_offset")]
    pub stack_offset: f64,

    /// Default orientation behavior when stacking windows.
    /// Options:
    /// - "perpendicular" (default): choose the perpendicular orientation to the parent layout
    /// - "same": use the same orientation as the parent layout
    /// - "horizontal"/"vertical": explicitly use a specific orientation
    #[serde(default = "default_stack_orientation")]
    pub default_orientation: StackDefaultOrientation,
}

/// Gap configuration for window spacing
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct GapSettings {
    /// Outer gaps (space between windows and screen edges)
    #[serde(default)]
    pub outer: OuterGaps,
    /// Inner gaps (space between windows)
    #[serde(default)]
    pub inner: InnerGaps,
    /// Display-specific gap overrides keyed by display UUID
    #[serde(default)]
    pub per_display: HashMap<String, GapOverride>,
}

/// Outer gap configuration (space between windows and screen edges)
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct OuterGaps {
    /// Gap at the top of the screen
    #[serde(default)]
    pub top: f64,
    /// Gap at the left of the screen
    #[serde(default)]
    pub left: f64,
    /// Gap at the bottom of the screen
    #[serde(default)]
    pub bottom: f64,
    /// Gap at the right of the screen
    #[serde(default)]
    pub right: f64,
}

/// Inner gap configuration (space between windows)
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct InnerGaps {
    /// Horizontal gap between windows
    #[serde(default)]
    pub horizontal: f64,
    /// Vertical gap between windows
    #[serde(default)]
    pub vertical: f64,
}

/// Overrides for gaps on a per-display basis
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct GapOverride {
    /// Override outer gaps completely for the display
    #[serde(default)]
    pub outer: Option<OuterGaps>,
    /// Override inner gaps completely for the display
    #[serde(default)]
    pub inner: Option<InnerGaps>,
}

impl Default for StackSettings {
    fn default() -> Self {
        Self {
            base: BaseLayoutSettings::default(),
            stack_offset: default_stack_offset(),
            default_orientation: default_stack_orientation(),
        }
    }
}

impl Default for MasterStackSettings {
    fn default() -> Self {
        Self {
            base: BaseLayoutSettings::default(),
            master_ratio: default_master_stack_ratio(),
            master_count: default_master_stack_count(),
            master_side: MasterStackSide::Left,
            new_window_placement: default_master_stack_new_window_placement(),
            master_arrangement: None,
            stack_arrangement: None,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.animation_duration < 0.0 {
            issues.push(format!(
                "animation_duration must be non-negative, got {}",
                self.animation_duration
            ));
        }

        if self.animation_fps <= 0.0 {
            issues.push(format!(
                "animation_fps must be positive, got {}",
                self.animation_fps
            ));
        }

        issues.extend(self.layout.validate());

        if self.gestures.swipe_vertical_tolerance < 0.0 {
            issues.push(format!(
                "gestures.swipe_vertical_tolerance must be non-negative, got {}",
                self.gestures.swipe_vertical_tolerance
            ));
        }

        issues
    }
}

impl LayoutSettings {
    pub fn base_for(&self, mode: LayoutMode) -> &BaseLayoutSettings {
        match mode {
            LayoutMode::Traditional => &self.traditional.base,
            LayoutMode::Bsp => &self.bsp.base,
            LayoutMode::Stack => &self.stack.base,
            LayoutMode::MasterStack => &self.master_stack.base,
            LayoutMode::Scrolling => &self.scrolling.base,
        }
    }

    pub fn window_insertion_point_for(&self, mode: LayoutMode) -> WindowInsertionPoint {
        self.base_for(mode)
            .window_insertion_point
            .or(self.base.window_insertion_point)
            .unwrap_or_default()
    }

    pub fn resolved_base_for(&self, mode: LayoutMode) -> BaseLayoutSettings {
        BaseLayoutSettings {
            window_insertion_point: Some(self.window_insertion_point_for(mode)),
        }
    }

    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        issues.extend(self.stack.validate());

        issues.extend(self.master_stack.validate());

        issues.extend(self.gaps.validate());

        issues.extend(self.scrolling.validate());

        issues
    }
}

impl ScrollingLayoutSettings {
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if !(0.0..=1.0).contains(&self.column_width_ratio) {
            issues.push(format!(
                "layout.scrolling.column_width_ratio must be between 0.0 and 1.0, got {}",
                self.column_width_ratio
            ));
        }

        if !(0.0..=1.0).contains(&self.min_column_width_ratio) {
            issues.push(format!(
                "layout.scrolling.min_column_width_ratio must be between 0.0 and 1.0, got {}",
                self.min_column_width_ratio
            ));
        }

        if !(0.0..=1.0).contains(&self.max_column_width_ratio) {
            issues.push(format!(
                "layout.scrolling.max_column_width_ratio must be between 0.0 and 1.0, got {}",
                self.max_column_width_ratio
            ));
        }

        if self.min_column_width_ratio > self.max_column_width_ratio {
            issues.push(format!(
                "layout.scrolling.min_column_width_ratio ({}) must be <= max_column_width_ratio ({})",
                self.min_column_width_ratio, self.max_column_width_ratio
            ));
        }

        if !(self.min_column_width_ratio..=self.max_column_width_ratio)
            .contains(&self.column_width_ratio)
        {
            issues.push(format!(
                "layout.scrolling.column_width_ratio ({}) must be within min/max bounds",
                self.column_width_ratio
            ));
        }

        for p in &self.width_presets {
            if !(0.0..=1.0).contains(p) {
                issues.push(format!(
                    "layout.scrolling.width_presets entries must be between 0.0 and 1.0, got {}",
                    p
                ));
            }
        }

        if self.gestures.vertical_tolerance < 0.0 {
            issues.push(format!(
                "layout.scrolling.gestures.vertical_tolerance must be non-negative, got {}",
                self.gestures.vertical_tolerance
            ));
        }

        issues
    }
}

impl StackSettings {
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.stack_offset < 0.0 {
            issues.push(format!(
                "stack_offset must be non-negative, got {}",
                self.stack_offset
            ));
        }

        issues
    }
}

impl MasterStackSettings {
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if !(0.05..=0.95).contains(&self.master_ratio) {
            issues.push(format!(
                "master_stack.master_ratio must be between 0.05 and 0.95, got {}",
                self.master_ratio
            ));
        }

        if self.master_count == 0 {
            issues.push("master_stack.master_count must be at least 1".to_string());
        }

        issues
    }
}

impl GapSettings {
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        // Validate outer gaps
        issues.extend(self.outer.validate());

        // Validate inner gaps
        issues.extend(self.inner.validate());

        for (uuid, overrides) in &self.per_display {
            if let Some(outer) = &overrides.outer {
                for issue in outer.validate() {
                    issues.push(format!("per_display[{uuid}] {issue}"));
                }
            }
            if let Some(inner) = &overrides.inner {
                for issue in inner.validate() {
                    issues.push(format!("per_display[{uuid}] {issue}"));
                }
            }
        }

        issues
    }

    pub fn effective_for_display(&self, display_uuid: Option<&str>) -> GapSettings {
        let mut resolved = GapSettings {
            outer: self.outer.clone(),
            inner: self.inner.clone(),
            per_display: HashMap::default(),
        };
        if let Some(uuid) = display_uuid {
            if let Some(overrides) = self.per_display.get(uuid) {
                if let Some(outer_override) = &overrides.outer {
                    resolved.outer = outer_override.clone();
                }
                if let Some(inner_override) = &overrides.inner {
                    resolved.inner = inner_override.clone();
                }
            }
        }
        resolved
    }
}

impl OuterGaps {
    /// Validates outer gap configuration values and returns a list of issues found.
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.top < 0.0 {
            issues.push(format!("outer.top gap must be non-negative, got {}", self.top));
        }

        if self.left < 0.0 {
            issues.push(format!("outer.left gap must be non-negative, got {}", self.left));
        }

        if self.bottom < 0.0 {
            issues.push(format!(
                "outer.bottom gap must be non-negative, got {}",
                self.bottom
            ));
        }

        if self.right < 0.0 {
            issues.push(format!(
                "outer.right gap must be non-negative, got {}",
                self.right
            ));
        }

        issues
    }
}

impl InnerGaps {
    /// Validates inner gap configuration values and returns a list of issues found.
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.horizontal < 0.0 {
            issues.push(format!(
                "inner.horizontal gap must be non-negative, got {}",
                self.horizontal
            ));
        }

        if self.vertical < 0.0 {
            issues.push(format!(
                "inner.vertical gap must be non-negative, got {}",
                self.vertical
            ));
        }

        issues
    }
}

fn yes() -> bool { true }

fn default_stack_offset() -> f64 { 40.0 }

pub fn default_stack_orientation() -> StackDefaultOrientation {
    StackDefaultOrientation::Perpendicular
}

fn default_master_stack_new_window_placement() -> MasterStackNewWindowPlacement {
    MasterStackNewWindowPlacement::Master
}

fn default_animation_duration() -> f64 { 0.3 }

fn default_animation_fps() -> f64 { 100.0 }

#[allow(dead_code)]
pub fn no() -> bool { false }

fn default_layout_folder() -> PathBuf { PathBuf::from("~/.config/rift/layouts") }

fn default_workspace_count() -> usize { 4 }

fn default_workspace_names() -> Vec<String> {
    vec![
        "Main".to_string(),
        "Development".to_string(),
        "Communication".to_string(),
        "Utilities".to_string(),
    ]
}

// Interpreted as normalized fraction when <= 1.0. If > 1.0 and <= 100.0,
// it is treated as a percentage (e.g. 40.0 -> 0.40).
fn default_swipe_vertical_tolerance() -> f64 { 0.4 }
fn default_swipe_fingers() -> usize { 3 }
fn default_distance_pct() -> f64 { 0.08 }
fn default_overscroll_threshold() -> f64 { 0.15 }
fn default_scroll_speed() -> f64 { 1.0 }

fn default_stack_line_spacing() -> f64 { 1.0 }
fn default_stack_line_thickness() -> f64 { 20.0 }
fn default_stack_line_border_width() -> f64 { 0.5 }

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum HapticPattern {
    Generic,
    Alignment,
    #[default]
    LevelChange,
}

impl Config {
    pub fn read(path: &Path) -> anyhow::Result<Config> {
        let buf = std::fs::read_to_string(path)?;
        Self::parse(&buf)
    }

    pub fn default() -> Config { Self::parse(include_str!("../../rift.default.toml")).unwrap() }

    /// Save the current config to a file
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        let config_file = ConfigFile {
            settings: self.settings.clone(),
            keys: self
                .key_specs
                .iter()
                .map(|(hotkey, command)| (hotkey.clone(), command.clone()))
                .collect(),
            virtual_workspaces: self.virtual_workspaces.clone(),
            modifier_combinations: HashMap::default(),
        };

        let toml_string = toml::to_string_pretty(&config_file)?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, toml_string.as_bytes())?;

        Ok(())
    }

    /// Validates the entire configuration and returns a list of issues found.
    pub fn validate(&self) -> Vec<String> {
        let mut issues = Vec::new();

        // Validate settings
        issues.extend(self.settings.validate());

        // Validate virtual workspace settings
        issues.extend(self.virtual_workspaces.validate());

        issues
    }

    fn normalize_hotkey_string(key: &str) -> String {
        let mut out = String::with_capacity(key.len());
        let mut word = String::new();

        for ch in key.chars() {
            if ch.is_alphabetic() {
                word.push(ch);
            } else {
                if !word.is_empty() {
                    let token = if word.len() == 1 {
                        word.to_ascii_uppercase()
                    } else {
                        match word.to_lowercase().as_str() {
                            "up" => "ArrowUp".to_string(),
                            "down" => "ArrowDown".to_string(),
                            "left" => "ArrowLeft".to_string(),
                            "right" => "ArrowRight".to_string(),
                            _ => word.clone(),
                        }
                    };
                    out.push_str(&token);
                    word.clear();
                }
                out.push(ch);
            }
        }

        if !word.is_empty() {
            let token = if word.len() == 1 {
                word.to_ascii_uppercase()
            } else {
                match word.to_lowercase().as_str() {
                    "up" => "ArrowUp".to_string(),
                    "down" => "ArrowDown".to_string(),
                    "left" => "ArrowLeft".to_string(),
                    "right" => "ArrowRight".to_string(),
                    _ => word.clone(),
                }
            };
            out.push_str(&token);
        }

        out
    }

    fn expand_modifier_combinations(key: &str, combinations: &HashMap<String, String>) -> String {
        if let Some(plus_pos) = key.find(" + ") {
            let potential_combo = &key[..plus_pos];
            if let Some(combo_value) = combinations.get(potential_combo) {
                let rest = &key[plus_pos + 3..];
                return format!("{} + {}", combo_value, rest);
            }
        }
        key.to_string()
    }

    /// no need to pull in a dep for just this
    fn levenshtein(a: &str, b: &str) -> usize {
        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();
        let mut d = vec![vec![0usize; b_chars.len() + 1]; a_chars.len() + 1];
        for i in 0..=a_chars.len() {
            d[i][0] = i;
        }
        for j in 0..=b_chars.len() {
            d[0][j] = j;
        }
        for i in 1..=a_chars.len() {
            for j in 1..=b_chars.len() {
                let cost = if a_chars[i - 1] == b_chars[j - 1] {
                    0
                } else {
                    1
                };
                d[i][j] = std::cmp::min(
                    std::cmp::min(d[i - 1][j] + 1, d[i][j - 1] + 1),
                    d[i - 1][j - 1] + cost,
                );
            }
        }
        d[a_chars.len()][b_chars.len()]
    }

    // Extracts an "unknown variant `...`" token from serde error string when present.
    // Additionally, if serde's error message contains an "expected" list (backtick-delimited),
    // embed those expected tokens alongside the unknown token using the separator "||".
    // The resulting returned string may therefore be:
    //   - "unknown_token" (no expected candidates found)
    //   - "unknown_token||cand1,cand2,..." (candidates appended)
    fn extract_unknown_variant(err: &str) -> Option<String> {
        let needle = "unknown variant `";
        if let Some(start) = err.find(needle) {
            let rest = &err[start + needle.len()..];
            if let Some(end) = rest.find('`') {
                let unknown = rest[..end].to_string();

                // Collect all backtick-enclosed tokens in the error message and
                // treat them as candidate variants (excluding the unknown itself).
                let mut variants: Vec<String> = Vec::new();
                let mut i = 0usize;
                while let Some(open) = err[i..].find('`') {
                    let open_abs = i + open + 1;
                    if let Some(close_off) = err[open_abs..].find('`') {
                        let close_abs = open_abs + close_off;
                        let token = &err[open_abs..close_abs];
                        if token != unknown {
                            variants.push(token.to_string());
                        }
                        i = close_abs + 1;
                    } else {
                        break;
                    }
                }

                if !variants.is_empty() {
                    // dedupe while preserving order
                    let mut seen = std::collections::HashSet::new();
                    let mut deduped = Vec::new();
                    for v in variants {
                        if seen.insert(v.clone()) {
                            deduped.push(v);
                        }
                    }
                    return Some(format!("{}||{}", unknown, deduped.join(",")));
                }

                return Some(unknown);
            }
        }

        if let Some(unknown_pos) = err.find("unknown") {
            if let Some(backtick_pos) = err[unknown_pos..].find('`') {
                let rest = &err[unknown_pos + backtick_pos + 1..];
                if let Some(end) = rest.find('`') {
                    return Some(rest[..end].to_string());
                }
            }
        }
        None
    }

    // Provide suggestion by comparing the unknown token to a list of known commands.
    // If the `unknown` string was produced by `extract_unknown_variant` and contains
    // an embedded serde candidate list (format: "token||cand1,cand2"), prefer those
    // candidates when computing the best suggestion. Otherwise fall back to the
    // conservative builtin list.
    //
    // Returns the best candidate if its distance is within a reasonable threshold.
    fn suggest_similar_command(unknown: &str) -> Option<(String, Option<String>)> {
        // Detect if `unknown` was augmented with serde-provided expected variants.
        let (unknown_token, serde_candidates): (String, Option<Vec<String>>) =
            if let Some(idx) = unknown.find("||") {
                let (u, rest) = unknown.split_at(idx);
                let rest = &rest[2..];
                let candidates: Vec<String> = rest
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                (u.to_lowercase(), Some(candidates))
            } else {
                (unknown.to_lowercase(), None)
            };

        // Choose candidate set: prefer serde-provided ones when available.
        let mut best: Option<(String, usize)> = None;

        if let Some(cands) = serde_candidates {
            for cand in cands.iter() {
                let cand_norm = cand.to_lowercase();
                let dist = Self::levenshtein(&unknown_token, &cand_norm);
                if best.is_none() || dist < best.as_ref().unwrap().1 {
                    best = Some((cand.clone(), dist));
                }
            }
        } else {
            // Use dynamically generated builtin candidates.
            let builtin_candidates = crate::actor::wm_controller::WmCommand::builtin_candidates();
            for cand in builtin_candidates.iter() {
                let dist = Self::levenshtein(&unknown_token, &cand.to_lowercase());
                if best.is_none() || dist < best.as_ref().unwrap().1 {
                    best = Some((cand.to_string(), dist));
                }
            }
        }

        if let Some((best_cand, dist)) = best {
            // Heuristic threshold: allow suggestions if distance is <= half the length (or <=3).
            let threshold = std::cmp::max(3usize, best_cand.len() / 2);
            if dist <= threshold {
                // If the best candidate is in deprecated map, return the non-deprecated suggestion.
                let mut replacement = None;
                for &(dep, repl) in DEPRECATED_MAP.iter() {
                    if dep == best_cand {
                        replacement = Some(repl.to_string());
                        break;
                    }
                }
                return Some((best_cand.to_string(), replacement));
            }
        }

        // Also check if the unknown token itself matched a deprecated name exactly
        for &(dep, repl) in DEPRECATED_MAP.iter() {
            if dep == unknown_token {
                return Some((repl.to_string(), None)); // recommend replacement
            }
        }

        None
    }

    fn parse(buf: &str) -> anyhow::Result<Config> {
        // Attempt to deserialize. If it fails, and the error indicates an unknown enum
        // variant, attempt to provide a helpful suggestion.
        match parse_config_file(buf) {
            Ok(c) => {
                let mut keys = Vec::new();
                let mut key_specs = Vec::new();
                for (key, cmd) in c.keys {
                    let expanded_key =
                        Self::expand_modifier_combinations(&key, &c.modifier_combinations);
                    let normalized_key = Self::normalize_hotkey_string(&expanded_key);
                    let Ok(hotkey) = Hotkey::from_str(&normalized_key) else {
                        bail!("Could not parse hotkey: {key}");
                    };
                    keys.push((hotkey, cmd.clone()));
                    key_specs.push((normalized_key, cmd));
                }
                Ok(Config {
                    settings: c.settings,
                    keys,
                    key_specs,
                    virtual_workspaces: c.virtual_workspaces,
                })
            }
            Err(e) => {
                let msg = e.to_string();
                if let Some(unknown_token) = Self::extract_unknown_variant(&msg) {
                    if let Some((suggestion, deprecated_replacement)) =
                        Self::suggest_similar_command(&unknown_token)
                    {
                        if let Some(repl) = deprecated_replacement {
                            bail!(
                                "{msg}\nDid you mean `{}`? Note: `{}` is deprecated; use `{}` instead.",
                                suggestion,
                                suggestion,
                                repl
                            );
                        } else {
                            bail!("{msg}\nDid you mean `{}`?", suggestion);
                        }
                    } else {
                        bail!("{msg}");
                    }
                } else {
                    bail!("{msg}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::reactor;
    use crate::layout_engine::{LayoutCommand, ResizeOrientation};

    /// Guards the embedded `rift.default.toml`, which `Config::default()` parses
    /// with `.unwrap()` — so a bad default config is a startup panic, not a
    /// diagnostic. `cargo check` cannot see it: every failure mode here is a serde
    /// runtime error (a new `deny_unknown_fields` struct, a `#[serde(flatten)]`
    /// that changes the deserialization path, a command variant that lost its
    /// dual-form parse or its `#[serde(default)]`).
    ///
    /// Panics in this test binary abort instead of unwinding, so without a named
    /// test like this one a broken default config kills the whole run with no
    /// attributable failure. FORK.md §8 stability item 1.
    #[test]
    fn default_config_parses() {
        let config = Config::parse(include_str!("../../rift.default.toml"))
            .expect("the embedded rift.default.toml must parse");

        // An empty `keys` map would still "parse", so assert the bindings survived.
        assert!(!config.keys.is_empty(), "default config binds no keys");

        // The bare `move_focus = "left"` form specifically: this is what panics at
        // startup if MoveFocusArgs loses its `Repr::Bare` arm (it lives in
        // rift-protocol now, one crate away from this file).
        let move_focus_binds = config
            .keys
            .iter()
            .filter(|(_, cmd)| {
                matches!(
                    cmd,
                    crate::actor::wm_controller::WmCommand::ReactorCommand(
                        reactor::Command::Layout(LayoutCommand::MoveFocus(_))
                    )
                )
            })
            .count();
        assert_eq!(move_focus_binds, 4, "expected the four bare move_focus binds");
    }

    /// Guards the `just test` allowlist against silent coverage loss.
    ///
    /// `just test` runs a *curated* filter list, so a new `#[cfg(test)] mod` that
    /// nobody adds a filter for simply never runs — and reads as covered. That has
    /// now happened three times: `ui::stack_line` and `ui::hints_bar` were absent for
    /// months while `FORK.md` §2.E claimed their geometry tests ran, and
    /// `ui::menu_bar` arrived with upstream's `abdbcc8` carrying four tests that
    /// would have gone the same way.
    ///
    /// The invariant is *classification*, not inclusion: many modules are excluded on
    /// purpose because they need a real GUI session (SkyLight / window-server tests
    /// abort with an objc weak-reference error in a headless shell — see `just
    /// test-all`). So every test module must be either allowlisted in the `test:`
    /// recipe or named in `GUI_OR_DEFERRED` below. A module in neither fails here,
    /// which is the loud failure the recipe cannot produce by itself.
    ///
    /// Lives in this module because the guard is only useful if it runs, and
    /// `common::config` is itself allowlisted.
    #[test]
    fn just_test_allowlist_classifies_every_test_module() {
        /// Test modules deliberately outside `just test`. Removing an entry here
        /// without adding a `test:` filter re-opens the silent-coverage hole.
        ///
        /// This list started out much longer, populated by assuming anything under
        /// `actor::`/`model::`/`ui::` needed a GUI. Measured module by module on
        /// 2026-08-11, that was wrong for 13 of them — 148 tests that pass perfectly
        /// well headless were being skipped. **Only `sys` genuinely aborts** (objc
        /// weak-reference error from the SkyLight/window-server tests). Before adding
        /// an entry here, run it: `cargo test --lib -- <module> --test-threads=1`.
        const GUI_OR_DEFERRED: &[&str] = &[
            // Not reachable from `cargo test --lib` at all: `src/bin/rift-cli.rs` is a
            // separate bin target, so `--lib` never compiles it. `just test-all` does
            // not cover it either; only a bare `cargo test` would.
            "bin::rift-cli",
            // Genuinely needs a real GUI session / live window server: aborts headless
            // rather than failing. This is the one the fast recipe must skip.
            "sys",
            // Test-support and no-#[test] modules: a `#[cfg(test)]` module that
            // declares helpers or fixtures only, so no filter would match anything.
            "actor::app",
            "actor::mission_control",
            "actor::reactor::replay",
            "actor::reactor::SpaceEventHandler",
            "actor::reactor::testing",
            "actor::wm_controller",
            "common::collections",
            "common::log",
        ];

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let recipe = include_str!("../../justfile")
            .lines()
            .find(|line| line.contains("cargo test --lib --") && line.contains("--skip"))
            .expect("justfile must still have a `test:` recipe running `cargo test --lib`");

        // Filters sit between `--lib --` and the first `--skip`.
        let filters: Vec<&str> = recipe
            .split_once("--lib --")
            .expect("recipe shape changed")
            .1
            .split("--skip")
            .next()
            .unwrap()
            .split_whitespace()
            .collect();
        assert!(!filters.is_empty(), "the `test:` recipe names no filters");

        // `a::b` covers `a::b::tests` but must not let `ui::menu` cover `ui::menu_bar`.
        let covers = |prefix: &str, module: &str| {
            module == prefix
                || module.strip_prefix(prefix).is_some_and(|rest| rest.starts_with("::"))
        };

        let mut unclassified = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("src/ must be readable") {
                let path = entry.expect("readable dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|ext| ext != "rs") {
                    continue;
                }
                let body = std::fs::read_to_string(&path).expect("source must be readable");

                // `src/a/b.rs` -> `a::b`; `src/a/mod.rs` -> `a`; `src/lib.rs` -> ``.
                let rel = path.strip_prefix(&root).expect("under src/").with_extension("");
                let mut owner: Vec<String> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                if owner.last().is_some_and(|last| last == "mod" || last == "lib") {
                    owner.pop();
                }

                // Each `#[cfg(test)]` guarding a `mod NAME` is one test module.
                for (idx, line) in body.lines().enumerate() {
                    if line.trim_start() != "#[cfg(test)]" {
                        continue;
                    }
                    let Some(name) = body
                        .lines()
                        .skip(idx + 1)
                        .find(|next| !next.trim_start().starts_with("#["))
                        .and_then(|decl| decl.trim_start().strip_prefix("mod "))
                        .and_then(|rest| rest.split([' ', ';', '{']).next())
                    else {
                        continue; // `#[cfg(test)] use ...` and friends
                    };
                    let module = owner
                        .iter()
                        .map(String::as_str)
                        .chain(std::iter::once(name))
                        .collect::<Vec<_>>()
                        .join("::");
                    let classified =
                        filters.iter().chain(GUI_OR_DEFERRED).any(|prefix| covers(prefix, &module));
                    if !classified {
                        unclassified.push(module);
                    }
                }
            }
        }

        unclassified.sort();
        assert!(
            unclassified.is_empty(),
            "these test modules are in neither the `just test` allowlist nor \
             GUI_OR_DEFERRED, so they silently never run: {unclassified:#?}",
        );
    }

    /// The allowlist guard above covers test *modules*. Nothing covered the
    /// `--skip` names until 2026-08-28, when a sync found
    /// `--skip ax_invalidation_after_quarantine_release_preserves_live_layout_state`
    /// naming a test upstream had renamed months earlier. A `--skip` that matches
    /// nothing is not an error to libtest — it silently filters zero tests, so the
    /// skip looks honoured while the test it was meant to suppress either runs
    /// (and passes, hiding that the skip is stale) or no longer exists at all.
    ///
    /// Same failure shape as the allowlist hole: a curated list that fails *silent*.
    /// Upstream renames tests freely, so this rots on their schedule, not ours.
    #[test]
    fn just_test_skips_all_name_a_real_test() {
        let recipe = include_str!("../../justfile")
            .lines()
            .find(|line| line.contains("cargo test --lib --") && line.contains("--skip"))
            .expect("justfile must still have a `test:` recipe running `cargo test --lib`");

        let skips: Vec<&str> = recipe
            .split("--skip")
            .skip(1)
            .filter_map(|rest| rest.split_whitespace().next())
            .collect();
        assert!(!skips.is_empty(), "recipe shape changed: no --skip names parsed");

        // Every `fn NAME` under src/. `--skip` is a substring match against the
        // full test path, so a prefix of a test's name is a legitimate skip
        // (`topology_change_clears_stale_pending_hide_target` is one).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut fn_names: Vec<String> = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("src/ must be readable") {
                let path = entry.expect("readable dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|ext| ext != "rs") {
                    continue;
                }
                let body = std::fs::read_to_string(&path).expect("source must be readable");
                for line in body.lines() {
                    let Some(rest) = line.trim_start().strip_prefix("fn ") else {
                        continue;
                    };
                    if let Some(name) = rest.split(['(', '<', ' ']).next() {
                        fn_names.push(name.to_owned());
                    }
                }
            }
        }

        let dead: Vec<&str> = skips
            .iter()
            .copied()
            .filter(|skip| !fn_names.iter().any(|name| name.contains(skip)))
            .collect();
        assert!(
            dead.is_empty(),
            "these `just test` --skip names match no test in src/, so they filter \
             nothing and the suppression they claim is imaginary: {dead:#?}\n\
             Either the test was renamed upstream (re-point the skip and re-verify \
             with `just upstream-test <TEST>`) or it is gone (delete the skip).",
        );
    }

    #[test]
    fn layout_insertion_point_supports_global_default_and_per_mode_override() {
        let settings: LayoutSettings = toml::from_str(
            r#"
                window_insertion_point = "end_of_tree"

                [traditional]
                window_insertion_point = "next_to_selection"
                equalize_nodes = true

                [scrolling]
                animate = false
            "#,
        )
        .unwrap();

        assert_eq!(
            settings.window_insertion_point_for(LayoutMode::Traditional),
            WindowInsertionPoint::NextToSelection
        );
        assert_eq!(
            settings.window_insertion_point_for(LayoutMode::Bsp),
            WindowInsertionPoint::EndOfTree
        );
        assert!(settings.traditional.equalize_nodes);
        assert_eq!(settings.scrolling.animate, Some(false));
    }

    #[test]
    fn virtual_workspace_prevent_wrapping_defaults_to_false_and_accepts_suggested_alias() {
        let defaults: VirtualWorkspaceSettings = toml::from_str("").unwrap();
        assert!(!defaults.prevent_wrapping);

        let settings: VirtualWorkspaceSettings =
            toml::from_str("prevent_wrapping_around = true").unwrap();
        assert!(settings.prevent_wrapping);
    }

    #[test]
    fn app_rules_parse_placement_size_and_focus() {
        let settings: VirtualWorkspaceSettings = toml::from_str(
            r#"
                app_rules = [{
                    app_id = "com.example.Tool",
                    floating = true,
                    position = { x = 0.4, y = 0.7 },
                    size = { w = 640, h = 480 },
                    focus = true
                }]
            "#,
        )
        .unwrap();

        let rule = &settings.app_rules[0];
        assert_eq!(rule.position, Some(AppRulePosition { x: 0.4, y: 0.7 }));
        assert_eq!(rule.size, Some(AppRuleSize { w: Some(640.0), h: Some(480.0) }));
        assert!(rule.focus);
        assert!(settings.validate().is_empty());

        let height_only: VirtualWorkspaceSettings = toml::from_str(
            r#"
                app_rules = [{
                    app_id = "com.example.Panel",
                    size = { h = 320 }
                }]
            "#,
        )
        .unwrap();
        assert_eq!(
            height_only.app_rules[0].size,
            Some(AppRuleSize { w: None, h: Some(320.0) })
        );
        assert!(height_only.validate().is_empty());
    }

    #[test]
    fn app_rule_geometry_validation_rejects_invalid_values() {
        let mut settings = VirtualWorkspaceSettings::default();
        settings.app_rules.push(AppWorkspaceRule {
            app_id: Some("com.example.Tool".into()),
            workspace: None,
            floating: false,
            position: Some(AppRulePosition { x: -0.1, y: 1.1 }),
            size: Some(AppRuleSize {
                w: Some(0.0),
                h: Some(f64::NAN),
            }),
            focus: false,
            manage: Some(true),
            app_name: None,
            title_regex: None,
            title_substring: None,
            ax_role: None,
            ax_subrole: None,
        });

        let issues = settings.validate();
        assert!(issues.iter().any(|issue| issue.contains("between 0 and 1")));
        assert!(issues.iter().any(|issue| issue.contains("only applies")));
        assert!(issues.iter().any(|issue| issue.contains("finite positive")));
    }

    #[test]
    fn app_rule_validation_reports_invalid_regex_and_ignored_effects() {
        let mut settings = VirtualWorkspaceSettings::default();
        settings.app_rules.push(AppWorkspaceRule {
            app_id: Some("com.example.Tool".into()),
            workspace: Some(WorkspaceSelector::Index(1)),
            floating: true,
            focus: true,
            manage: Some(false),
            title_regex: Some("[".into()),
            ..Default::default()
        });

        let issues = settings.validate();
        assert!(issues.iter().any(|issue| issue.contains("invalid title_regex")));
        assert!(issues.iter().any(|issue| issue.contains("effects are ignored")));
    }

    #[test]
    fn resize_command_config_supports_legacy_and_oriented_forms() {
        #[derive(Deserialize)]
        struct TestConfig {
            keys: HashMap<String, WmCommand>,
        }

        let mut document: toml::Value = toml::from_str(
            r#"
            [keys]
            legacy = "resize_window_grow"
            vertical = { resize_window_shrink = "vertical" }
            smart = { resize_window_grow = "smart" }
            "#,
        )
        .unwrap();
        assert!(migrate_legacy_resize_bindings(&mut document));
        let config: TestConfig = document.try_into().unwrap();

        assert_eq!(
            config.keys["legacy"],
            WmCommand::ReactorCommand(reactor::Command::Layout(LayoutCommand::ResizeWindowGrow(
                ResizeOrientation::Horizontal
            )))
        );
        assert_eq!(
            config.keys["vertical"],
            WmCommand::ReactorCommand(reactor::Command::Layout(LayoutCommand::ResizeWindowShrink(
                ResizeOrientation::Vertical
            )))
        );
        assert_eq!(
            config.keys["smart"],
            WmCommand::ReactorCommand(reactor::Command::Layout(LayoutCommand::ResizeWindowGrow(
                ResizeOrientation::Smart
            )))
        );
    }

    #[test]
    fn menu_bar_layout_folder_defaults_and_expands_home() {
        let settings: MenuBarSettings = toml::from_str("").unwrap();

        assert_eq!(settings.layout_folder, PathBuf::from("~/.config/rift/layouts"));
        assert_eq!(
            settings.resolved_layout_folder(),
            dirs::home_dir().unwrap().join(".config/rift/layouts")
        );
    }

    #[test]
    fn menu_bar_layout_folder_preserves_absolute_paths() {
        let settings: MenuBarSettings =
            toml::from_str("layout_folder = \"/tmp/rift-layouts\"").unwrap();

        assert_eq!(
            settings.resolved_layout_folder(),
            PathBuf::from("/tmp/rift-layouts")
        );
    }

    #[test]
    fn test_normalize_hotkey_string() {
        assert_eq!(
            Config::normalize_hotkey_string("Alt + Shift + Down"),
            "Alt + Shift + ArrowDown"
        );
        assert_eq!(Config::normalize_hotkey_string("Ctrl + Up"), "Ctrl + ArrowUp");
        assert_eq!(
            Config::normalize_hotkey_string("Shift + Left"),
            "Shift + ArrowLeft"
        );
        assert_eq!(
            Config::normalize_hotkey_string("Meta + Right"),
            "Meta + ArrowRight"
        );
    }

    #[test]
    fn test_modifier_combinations_in_config() {
        let toml = r#"
            [settings]
            animate = false

            [modifier_combinations]
            comb1 = "Alt + Shift"
            leader = "Ctrl + Alt"

            [keys]
            "comb1 + C" = "toggle_space_activated"
            "leader + Tab" = "next_workspace"
            "Alt + H" = { move_focus = "left" }
        "#;

        let cfg = Config::parse(toml).unwrap();
        // We expect keys to be parsed into hotkeys
        assert!(!cfg.keys.is_empty());
    }

    #[test]
    fn serde_round_trip_preserves_key_specs() {
        let cfg = Config::default();
        assert!(!cfg.key_specs.is_empty());

        let json = serde_json::to_string(&cfg).unwrap();
        let round_tripped: Config = serde_json::from_str(&json).unwrap();

        assert_eq!(round_tripped.key_specs, cfg.key_specs);
    }

    #[test]
    fn serde_without_key_specs_reconstructs_from_keys() {
        let cfg = Config::default();
        let mut json = serde_json::to_value(&cfg).unwrap();
        json.as_object_mut().unwrap().remove("key_specs");

        let round_tripped: Config = serde_json::from_value(json).unwrap();

        assert_eq!(round_tripped.key_specs.len(), round_tripped.keys.len());
        assert!(!round_tripped.key_specs.is_empty());
    }

    #[test]
    fn test_levenshtein_suggests() {
        let err =
            "unknown variant `toggle_stak`, expected one of `toggle_stack`, `toggle_orientation`";
        let token = Config::extract_unknown_variant(err).unwrap();
        assert_eq!(token, "toggle_stak||toggle_stack,toggle_orientation");
        let suggestion = Config::suggest_similar_command(&token);
        assert!(suggestion.is_some());
        let (s, _maybe_dep) = suggestion.unwrap();
        assert_eq!(s, "toggle_stack");
    }
}
