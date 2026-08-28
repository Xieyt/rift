use std::path::PathBuf;

use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::{
    Direction, DisplaySelector, LayoutMode, ResizeOrientation, RestoreScope, RestoreSource,
    WindowId, WorkspaceSelector,
};

/// Arguments for [`LayoutCommand::MoveFocus`].
///
/// Deserializes from either the bare direction (`move_focus = "left"` — the
/// upstream default-config form, with `activate` defaulting to `false`) or the
/// full table (`move_focus = { direction = "left", activate = true }`). The
/// `activate` override forces app foregrounding even when
/// `[settings.layout] activate_on_focus` is disabled.
///
/// Both forms MUST stay parseable: the bare form is what `rift.default.toml`
/// binds, and that file is parsed by `Config::default()`, so dropping it is a
/// runtime panic at startup that `cargo check` cannot see. The same impl also
/// covers legacy untyped IPC clients sending `{"Reactor":{"move_focus":"left"}}`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MoveFocusArgs {
    pub direction: Direction,
    pub activate: bool,
}

impl<'de> Deserialize<'de> for MoveFocusArgs {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Bare(Direction),
            Full {
                direction: Direction,
                #[serde(default)]
                activate: bool,
            },
        }
        Ok(match Repr::deserialize(deserializer)? {
            Repr::Bare(direction) => MoveFocusArgs { direction, activate: false },
            Repr::Full { direction, activate } => MoveFocusArgs { direction, activate },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FloatingWindowSizePreset {
    Smart,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FloatingWindowSize {
    Dimensions { w: f64, h: f64 },
    Preset(FloatingWindowSizePreset),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToggleWindowFloatingOptions {
    #[serde(default)]
    pub center: bool,
    #[serde(default)]
    pub size: Option<FloatingWindowSize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutCommand {
    NextWindow,
    PrevWindow,
    MoveFocus(MoveFocusArgs),
    /// Focus the Nth top-level column (0-indexed, left->right), matching the
    /// hint-bar letters. Scrolling/niri only; other layouts ignore it.
    FocusColumn(usize),
    Ascend,
    Descend,
    MoveNode(Direction),
    JoinWindow(Direction),
    ConsumeOrExpelWindow(Direction),
    ToggleStack,
    ToggleColumnTabbed,
    CycleColumnWidth,
    ToggleOrientation,
    UnjoinWindows,
    ToggleFocusFloating,
    ToggleWindowFloating,
    ToggleWindowFloatingWithOptions(ToggleWindowFloatingOptions),
    ToggleFullscreen,
    ToggleFullscreenWithinGaps,
    ResizeWindowGrow(ResizeOrientation),
    ResizeWindowShrink(ResizeOrientation),
    ResizeWindowBy {
        amount: f64,
    },
    ScrollStrip {
        delta: f64,
    },
    SnapStrip,
    CenterSelection,
    NextWorkspace(Option<bool>),
    PrevWorkspace(Option<bool>),
    SwitchToWorkspace(usize),
    MoveWindowToWorkspace {
        workspace: WorkspaceSelector,
        // Keep `follow` optional: `rift.default.toml` documents the table form
        // without it, and legacy IPC clients predate the field. Bare
        // `move_window_to_workspace = N` binds resolve via WmCmd, not here.
        #[serde(default)]
        follow: bool,
        window_id: Option<u32>,
    },
    SetWorkspaceLayout {
        workspace: Option<usize>,
        mode: LayoutMode,
    },
    CreateWorkspace,
    SwitchToLastWorkspace,
    SwapWindows(WindowId, WindowId),
    AdjustMasterRatio(f64),
    AdjustMasterCount {
        delta: i32,
    },
    PromoteToMaster,
    SwapMasterStack,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReactorCommand {
    Debug,
    Serialize,
    SaveLayout {
        path: PathBuf,
    },
    SaveAndExit,
    RestoreLayout {
        path: PathBuf,
        scope: RestoreScope,
        #[serde(default)]
        source: RestoreSource,
    },
    SwitchSpace(Direction),
    ToggleSpaceActivated,
    FocusWindow {
        window_id: WindowId,
        window_server_id: Option<u32>,
    },
    ShowMissionControlAll,
    ShowMissionControlCurrent,
    DismissMissionControl,
    MoveMouseToDisplay(DisplaySelector),
    FocusDisplay(DisplaySelector),
    CloseWindow {
        window_server_id: Option<u32>,
    },
    ToggleHintsBar,
    MoveWindowToDisplay {
        selector: DisplaySelector,
        window_id: Option<u32>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricsCommand {
    ShowTiming,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigCommand {
    SetAnimate(bool),
    SetAnimationDuration(f64),
    SetAnimationFps(f64),
    SetAnimationEasing(AnimationEasing),
    SetMouseFollowsFocus(bool),
    SetMouseHidesOnFocus(bool),
    SetFocusFollowsMouse(bool),
    SetStackOffset(f64),
    SetOuterGaps {
        top: f64,
        left: f64,
        bottom: f64,
        right: f64,
    },
    SetInnerGaps {
        horizontal: f64,
        vertical: f64,
    },
    SetWorkspaceNames(Vec<String>),
    Set {
        key: String,
        value: Value,
    },
    GetConfig,
    SaveConfig,
    ReloadConfig,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationEasing {
    #[default]
    EaseInOut,
    Linear,
    EaseInSine,
    EaseOutSine,
    EaseInOutSine,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    EaseInQuart,
    EaseOutQuart,
    EaseInOutQuart,
    EaseInQuint,
    EaseOutQuint,
    EaseInOutQuint,
    EaseInExpo,
    EaseOutExpo,
    EaseInOutExpo,
    EaseInCirc,
    EaseOutCirc,
    EaseInOutCirc,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RiftCommand {
    Layout(LayoutCommand),
    Metrics(MetricsCommand),
    Reactor(ReactorCommand),
    Config(ConfigCommand),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum TypedRiftCommand {
    Layout(LayoutCommand),
    Metrics(MetricsCommand),
    Reactor(ReactorCommand),
    Config(ConfigCommand),
}

#[derive(Deserialize)]
enum LegacyCommand {
    #[serde(alias = "reactor")]
    Reactor(LegacyReactorCommand),
    #[serde(alias = "config")]
    Config(ConfigCommand),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LegacyReactorCommand {
    Layout(LayoutCommand),
    Metrics(MetricsCommand),
    Reactor(ReactorCommand),
}

impl From<TypedRiftCommand> for RiftCommand {
    fn from(command: TypedRiftCommand) -> Self {
        match command {
            TypedRiftCommand::Layout(command) => Self::Layout(command),
            TypedRiftCommand::Metrics(command) => Self::Metrics(command),
            TypedRiftCommand::Reactor(command) => Self::Reactor(command),
            TypedRiftCommand::Config(command) => Self::Config(command),
        }
    }
}

impl<'de> Deserialize<'de> for RiftCommand {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum CommandInput {
            Typed(TypedRiftCommand),
            LegacyJson(String),
        }

        match CommandInput::deserialize(deserializer)? {
            CommandInput::Typed(command) => Ok(command.into()),
            CommandInput::LegacyJson(command) => decode_legacy_command(&command),
        }
    }
}

fn decode_legacy_command<E>(command: &str) -> Result<RiftCommand, E>
where E: DeError {
    match serde_json::from_str::<LegacyCommand>(command)
        .map_err(|error| E::custom(format!("invalid legacy command JSON: {error}")))?
    {
        LegacyCommand::Config(command) => Ok(RiftCommand::Config(command)),
        LegacyCommand::Reactor(LegacyReactorCommand::Layout(command)) => {
            Ok(RiftCommand::Layout(command))
        }
        LegacyCommand::Reactor(LegacyReactorCommand::Metrics(command)) => {
            Ok(RiftCommand::Metrics(command))
        }
        LegacyCommand::Reactor(LegacyReactorCommand::Reactor(command)) => {
            Ok(RiftCommand::Reactor(command))
        }
    }
}
