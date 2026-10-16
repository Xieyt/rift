use std::path::PathBuf;

use serde::Deserialize;

use super::parse::migrate_legacy_resize_bindings;
use super::*;
use crate::actor::reactor;
use crate::actor::wm_controller::WmCommand;
use crate::common::collections::HashMap;
use crate::layout_engine::{LayoutCommand, ResizeOrientation};

#[test]
fn scrolling_display_widths_merge_and_validate() {
    let settings: ScrollingLayoutSettings = toml::from_str(
        r#"
            column_width_ratio = 0.7
            min_column_width_ratio = 0.3
            max_column_width_ratio = 0.9
            [per_display."display-a"]
            column_width_ratio = 0.5
            "#,
    )
    .unwrap();
    assert_eq!(settings.preset_column_widths, vec![1.0 / 3.0, 0.5, 2.0 / 3.0]);
    assert_eq!(settings.widths_for_display(Some("display-a")), (0.5, 0.3, 0.9));
    assert_eq!(settings.widths_for_display(Some("other")), (0.7, 0.3, 0.9));
    assert_eq!(settings.widths_for_display(None), (0.7, 0.3, 0.9));
    assert!(settings.validate().is_empty());
    assert!(settings.effective_for_display(Some("display-a")).per_display.is_empty());

    let mut invalid = settings;
    invalid.per_display.get_mut("display-a").unwrap().max_column_width_ratio = Some(0.4);
    assert!(
        invalid
            .validate()
            .iter()
            .any(|issue| issue.contains("per_display[display-a].column_width_ratio"))
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
fn horizontal_mouse_warp_config_parsing() {
    let missing: Settings = toml::from_str("").unwrap();
    assert_eq!(missing.horizontal_mouse_warp, None);
    for (value, expected) in [
        ("top-to-bottom", HorizontalMouseWarp::TopToBottom),
        ("bottom-to-top", HorizontalMouseWarp::BottomToTop),
    ] {
        let settings: Settings =
            toml::from_str(&format!("horizontal_mouse_warp = \"{value}\" ")).unwrap();
        assert_eq!(settings.horizontal_mouse_warp, Some(expected));
    }
    assert!(toml::from_str::<Settings>("horizontal_mouse_warp = \"sideways\"").is_err());
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
fn menu_bar_binding_mode_label_defaults_on_and_can_be_disabled() {
    let settings: MenuBarSettings = toml::from_str("").unwrap();
    assert!(settings.show_binding_mode);
    assert!(MenuBarSettings::default().show_binding_mode);
    let settings: MenuBarSettings = toml::from_str("show_binding_mode = false").unwrap();
    assert!(!settings.show_binding_mode);
    let roundtrip: MenuBarSettings = toml::from_str(&toml::to_string(&settings).unwrap()).unwrap();
    assert!(!roundtrip.show_binding_mode);
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
fn mouse_settings_defaults_and_variants_parse() {
    let defaults: DragDropSettings = toml::from_str("").unwrap();
    assert_eq!(defaults, DragDropSettings::default());

    let settings: DragDropSettings = toml::from_str(
        r#"
                enabled = false
                modifier = "ctrl"
                action1 = "none"
                action2 = "move"
                drop_action = "stack"
                drop_zone_fraction = 0.45
                preview = false
            "#,
    )
    .unwrap();
    assert_eq!(settings.modifier, MouseModifier::Ctrl);
    assert_eq!(settings.action1, MouseAction::None);
    assert_eq!(settings.action2, MouseAction::Move);
    assert_eq!(settings.drop_action, MouseDropAction::Stack);

    for (alias, expected) in [
        ("command", MouseModifier::Cmd),
        ("option", MouseModifier::Alt),
        ("control", MouseModifier::Ctrl),
    ] {
        let parsed: DragDropSettings = toml::from_str(&format!("modifier = \"{alias}\"")).unwrap();
        assert_eq!(parsed.modifier, expected);
    }
}

#[test]
fn legacy_window_snapping_is_removed_before_deserialization() {
    let cfg = Config::parse(
        r#"
                [settings.window_snapping]
                drag_swap_fraction = 0.3
                [keys]
            "#,
    )
    .unwrap();
    assert_eq!(cfg.settings.drag_drop, DragDropSettings::default());
}

#[test]
fn drag_drop_settings_take_precedence_over_legacy_window_snapping() {
    let cfg = Config::parse(
        r#"
                [settings.window_snapping]
                drag_swap_fraction = 0.3
                [settings.drag_drop]
                modifier = "alt"
                drop_action = "stack"
                [keys]
            "#,
    )
    .unwrap();
    assert_eq!(cfg.settings.drag_drop.modifier, MouseModifier::Alt);
    assert_eq!(cfg.settings.drag_drop.drop_action, MouseDropAction::Stack);
}

#[test]
fn invalid_drop_zone_fraction_has_clear_validation_error() {
    let mut cfg = Config::default();
    cfg.settings.drag_drop.drop_zone_fraction = 0.09;
    assert!(
        cfg.validate()
            .iter()
            .any(|issue| issue.contains("drag_drop.drop_zone_fraction"))
    );
    cfg.settings.drag_drop.drop_zone_fraction = 0.46;
    assert!(
        cfg.validate()
            .iter()
            .any(|issue| issue.contains("drag_drop.drop_zone_fraction"))
    );
}

#[test]
fn serde_round_trip_preserves_binding_mode_specs() {
    let cfg = Config::default();
    assert!(!cfg.binding_mode_specs[0].1.is_empty());

    let json = serde_json::to_string(&cfg).unwrap();
    let round_tripped: Config = serde_json::from_str(&json).unwrap();

    assert_eq!(round_tripped.binding_mode_specs, cfg.binding_mode_specs);
}

#[test]
fn keys_only_config_still_has_only_the_default_binding_set() {
    let config = Config::parse(
        r#"
                [settings]
                [keys]
                "A" = "reload_config"
            "#,
    )
    .unwrap();
    assert_eq!(config.binding_mode_specs.len(), 1);
    assert_eq!(config.binding_mode_specs[0].0, "default");
    assert!(config.binding_mode_specs[0].1.iter().any(|(spec, _)| spec == "A"));
}

#[test]
fn custom_binding_modes_parse_and_expand_modifier_combinations() {
    let config = Config::parse(
        r#"
                [settings]
                [modifier_combinations]
                nav = "Alt + Shift"
                [keys]
                "Alt + R" = { binding_mode = "resize" }
                [binding_modes.resize]
                "nav + N" = { binding_mode = "default" }
            "#,
    )
    .unwrap();
    assert_eq!(config.binding_mode_specs[0].0, "default");
    assert_eq!(config.binding_mode_specs[1].0, "resize");
    assert!(config.binding_mode_specs[1]
            .1
            .iter()
            .any(|(spec, command)| spec == "Alt + Shift + N"
                && matches!(command, WmCommand::Wm(crate::actor::wm_controller::WmCmd::BindingMode(target)) if target == "default")));
}

#[test]
fn binding_mode_config_rejects_bad_targets_reserved_default_and_bad_hotkeys() {
    let missing = r#"
            [settings]
            [keys]
            "Alt + R" = { binding_mode = "does-not-exist" }
            [binding_modes.resize]
            "Escape" = { binding_mode = "default" }
        "#;
    assert!(Config::parse(&missing).unwrap_err().to_string().contains("does-not-exist"));

    let reserved = r#"
            [settings]
            [keys]
            [binding_modes.default]
        "#;
    assert!(Config::parse(&reserved).unwrap_err().to_string().contains("reserved"));

    let malformed = r#"
            [settings]
            [keys]
            "NotARealHotkey" = { binding_mode = "default" }
        "#;
    assert!(Config::parse(&malformed).unwrap_err().to_string().contains("hotkey"));
}

#[test]
fn config_validation_rejects_duplicate_binding_mode_names() {
    let mut json = serde_json::to_value(Config::default()).unwrap();
    json["binding_mode_specs"] = serde_json::json!([["default", []], ["default", []]]);
    let config: Config = serde_json::from_value(json).unwrap();
    assert!(config.validate().iter().any(|issue| issue.contains("unique")));
}

#[test]
fn default_document_is_the_canonical_template() {
    let document =
        ConfigDocument::from_source(ConfigDocument::default().source().unwrap()).unwrap();
    assert_eq!(document.to_string(), include_str!("../../../rift.default.toml"));
}

#[test]
fn scalar_edit_preserves_all_other_source_bytes_and_rolls_back_invalid_edits() {
    let original = include_str!("../../../rift.default.toml")
        .replace(
            "animation_duration = 0.3",
            "animation_duration = 0.3 # my timing",
        )
        .replace("\n[keys]\n", "\n# custom key documentation\n[keys]\n");
    let mut document = ConfigDocument::parse(&original).unwrap();
    let runtime = document.update(|source| source.settings.animation_duration = 0.2).unwrap();
    assert_eq!(runtime.settings.animation_duration, 0.2);
    assert_eq!(
        document.to_string(),
        original.replace("animation_duration = 0.3", "animation_duration = 0.2")
    );
    let saved = document.to_string();
    assert!(document.update(|source| source.settings.animation_duration = -1.0).is_err());
    assert_eq!(document.to_string(), saved);
}

#[test]
fn typed_collections_keep_source_aliases_and_inline_commands() {
    let mut document = ConfigDocument::default();
    document
        .update(|source| {
            source.virtual_workspaces.workspace_names = vec!["coding".into(), "mail".into()];
            source.settings.layout.gaps.per_display.insert("display-a".into(), GapOverride {
                inner: Some(InnerGaps { horizontal: 8.0, vertical: 8.0 }),
                outer: None,
            });
            source.virtual_workspaces.app_rules.push(AppWorkspaceRule {
                app_id: Some("com.example.Tool".into()),
                floating: true,
                position: Some(AppRulePosition { x: 0.4, y: 0.7 }),
                ..Default::default()
            });
            source.virtual_workspaces.workspace_rules.push(WorkspaceLayoutRule {
                workspace: WorkspaceSelector::Index(1),
                layout: LayoutMode::Scrolling,
            });
            source.keys.insert("comb1 + Q".into(), source.keys["Alt + H"].clone());
            source.binding_modes.insert(
                "nav".into(),
                std::collections::BTreeMap::from([("H".into(), source.keys["Alt + H"].clone())]),
            );
        })
        .unwrap();
    let text = document.to_string();
    assert!(text.contains("workspace_names = [\n\t\"coding\",\n\t\"mail\"\n]"));
    assert!(
        text.contains("\"comb1 + Q\" = { move_focus = \"left\" }"),
        "{text}"
    );
    assert!(text.contains("\"Alt + H\" = { move_focus = \"left\" }"));
    assert!(text.find("[keys]").unwrap() < text.find("[binding_modes.nav]").unwrap());
    assert!(
        text.find("[settings.layout.gaps.per_display.display-a.inner]").unwrap()
            < text.find("[settings.ui.mission_control]").unwrap()
    );
    let runtime = Config::parse(&text).unwrap();
    assert_eq!(runtime.virtual_workspaces.app_rules.len(), 1);
    assert_eq!(runtime.virtual_workspaces.workspace_rules.len(), 1);
    assert!(runtime.binding_mode_specs[0].1.iter().any(|(key, _)| key == "Alt + Shift + Q"));
    document
        .update(|source| {
            source.keys.remove("comb1 + Q");
            source.binding_modes.remove("nav");
            source.virtual_workspaces.app_rules.clear();
        })
        .unwrap();
    assert!(!document.to_string().contains("\"comb1 + Q\" ="));
}

#[test]
fn document_save_preserves_symlink_and_reloadable_source() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("nested/config.toml");
    let mut document = ConfigDocument::default();
    document.save(&target).unwrap();
    let link = dir.path().join("config.toml");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    document.update(|source| source.settings.animate = true).unwrap();
    document.save(&link).unwrap();
    assert!(link.is_symlink());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), document.to_string());
    assert!(Config::read(&link).unwrap().settings.animate);
}

#[test]
fn commented_optional_setting_uses_example_anchor_and_resets_to_omission() {
    let mut document = ConfigDocument::default();
    document.update(|source| source.settings.default_disable = false).unwrap();
    let text = document.to_string();
    assert!(text.contains("#default_disable = false\ndefault_disable = false\n"));
    assert!(
        text.find("default_disable = false\n").unwrap()
            < text.find("focus_follows_mouse = true").unwrap()
    );
    document.update(|source| source.settings.default_disable = true).unwrap();
    assert_eq!(document.to_string(), include_str!("../../../rift.default.toml"));
}

#[test]
fn serde_without_binding_mode_specs_reconstructs_from_keys() {
    let cfg = Config::default();
    let mut json = serde_json::to_value(&cfg).unwrap();
    json.as_object_mut().unwrap().remove("binding_mode_specs");

    let round_tripped: Config = serde_json::from_value(json).unwrap();

    assert_eq!(
        round_tripped.binding_mode_specs[0].1.len(),
        round_tripped.keys.len()
    );
    assert!(!round_tripped.binding_mode_specs[0].1.is_empty());
}

#[test]
fn workspace_resize_cleans_removed_assignments_and_preserves_valid_rules() {
    let mut settings = VirtualWorkspaceSettings::default();
    settings.workspace_names = vec!["one".into(), "two".into(), "three".into()];
    settings.default_workspace = 2;
    settings.workspace_rules = vec![
        WorkspaceLayoutRule {
            workspace: WorkspaceSelector::Name("three".into()),
            layout: LayoutMode::Stack,
        },
        WorkspaceLayoutRule {
            workspace: WorkspaceSelector::Index(0),
            layout: LayoutMode::Bsp,
        },
    ];
    settings.app_rules = vec![AppWorkspaceRule {
        workspace: Some(WorkspaceSelector::Index(2)),
        ..Default::default()
    }];
    settings.resize(2);
    assert_eq!(settings.workspace_names, ["one", "two"]);
    assert_eq!(settings.default_workspace, 1);
    assert_eq!(settings.workspace_rules.len(), 1);
    assert_eq!(
        settings.workspace_rules[0].workspace,
        WorkspaceSelector::Index(0)
    );
    assert_eq!(settings.app_rules[0].workspace, None);
    settings.resize(MAX_WORKSPACES + 1);
    assert_eq!(settings.workspace_names, ["one", "two"]);
    assert!(!settings.validate().is_empty());
}

#[test]
fn keymap_rename_and_delete_update_both_command_encodings_and_self_references() {
    use rift_protocol::ReactorCommand;

    use crate::actor::wm_controller::WmCmd;
    let mut source = ConfigSource {
        settings: Config::default().settings,
        keys: Default::default(),
        binding_modes: Default::default(),
        virtual_workspaces: Default::default(),
        modifier_combinations: Default::default(),
    };
    source.create_keymap("edit".into()).unwrap();
    source.keys.insert("A".into(), WmCommand::Wm(WmCmd::BindingMode("edit".into())));
    source.keymap_mut("edit").unwrap().insert(
        "B".into(),
        WmCommand::ReactorCommand(reactor::Command::Reactor(ReactorCommand::BindingMode(
            "edit".into(),
        ))),
    );
    source.rename_keymap("edit", "renamed".into()).unwrap();
    assert!(source.keymap("edit").is_none());
    assert!(
        matches!(source.keys.get("A"), Some(WmCommand::Wm(WmCmd::BindingMode(name))) if name == "renamed")
    );
    assert!(
        matches!(source.keymap("renamed").unwrap().get("B"), Some(WmCommand::ReactorCommand(reactor::Command::Reactor(ReactorCommand::BindingMode(name)))) if name == "renamed")
    );
    source.keys.insert("C".into(), source.keymap("renamed").unwrap()["B"].clone());
    source.delete_keymap("renamed").unwrap();
    assert!(source.keymap("renamed").is_none());
    assert!(
        matches!(source.keys.get("A"), Some(WmCommand::Wm(WmCmd::BindingMode(name))) if name == "default")
    );
    assert!(
        matches!(source.keys.get("C"), Some(WmCommand::ReactorCommand(reactor::Command::Reactor(ReactorCommand::BindingMode(name)))) if name == "default")
    );
}

// ---- fork-only guards, relocated from src/common/config.rs's test module
// when b391d56 split config.rs and moved tests here (UPSTREAM-SYNC.md §12.6
// HZ-12: upstream's side of that hunk was empty, so @theirs would have
// deleted every one of these).

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
    let config = Config::parse(include_str!("../../../rift.default.toml"))
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
                crate::actor::wm_controller::WmCommand::ReactorCommand(reactor::Command::Layout(
                    LayoutCommand::MoveFocus(_)
                ))
            )
        })
        .count();
    assert_eq!(move_focus_binds, 4, "expected the four bare move_focus binds");
}

/// Locates the `test:` recipe line in the justfile. Shared by both
/// module-direction guards below and by `just_test_skips_all_name_a_real_test`.
fn test_recipe() -> &'static str {
    include_str!("../../../justfile")
        .lines()
        .find(|line| line.contains("cargo test --lib --") && line.contains("--skip"))
        .expect("justfile must still have a `test:` recipe running `cargo test --lib`")
}

/// Locates the `test-crates` recipe line. Unlike `test:`, it runs each
/// workspace-member crate's whole `--lib` suite unfiltered
/// (`cargo test -p <crate> --lib`), so it has `-p` package names instead of
/// module filters.
fn test_crates_recipe() -> &'static str {
    include_str!("../../../justfile")
        .lines()
        .find(|line| line.contains("cargo test -p") && line.contains("--lib"))
        .expect("justfile must still have a `test-crates` recipe running `cargo test -p ... --lib`")
}

/// The `-p <crate>` package names `test-crates` passes to cargo. Since that
/// recipe runs each named crate's full suite unfiltered, a package name here
/// classifies every test module under that crate the same way a `test:`
/// filter classifies a `src/` module.
fn test_crates_recipe_packages() -> Vec<&'static str> {
    let tokens: Vec<&str> = test_crates_recipe().split_whitespace().collect();
    tokens.windows(2).filter(|pair| pair[0] == "-p").map(|pair| pair[1]).collect()
}

/// The `test:` recipe's filters, i.e. everything between `--lib --` and the
/// first `--skip`.
fn recipe_filters(recipe: &str) -> Vec<&str> {
    recipe
        .split_once("--lib --")
        .expect("recipe shape changed")
        .1
        .split("--skip")
        .next()
        .unwrap()
        .split_whitespace()
        .collect()
}

/// `a::b` covers `a::b::tests` but must not let `ui::menu` cover `ui::menu_bar`.
fn covers(prefix: &str, module: &str) -> bool {
    module == prefix || module.strip_prefix(prefix).is_some_and(|rest| rest.starts_with("::"))
}

/// Walks one `src`-shaped directory tree, collecting the fully qualified path
/// of every `#[cfg(test)] mod NAME` it finds. `crate_prefix` prepends a
/// workspace-member crate's directory name for trees under `crates/*/src` so
/// their module paths (and the `-p` names that classify them) line up.
fn walk_test_modules(root: &std::path::Path, crate_prefix: Option<&str>) -> Vec<String> {
    let mut modules = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("source dir must be readable") {
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
            let rel = path.strip_prefix(root).expect("under root").with_extension("");
            let mut owner: Vec<String> =
                rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
            if owner.last().is_some_and(|last| last == "mod" || last == "lib") {
                owner.pop();
            }
            if let Some(prefix) = crate_prefix {
                owner.insert(0, prefix.to_owned());
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
                modules.push(module);
            }
        }
    }
    modules
}

/// Every test module in the workspace: `src/` (the root crate) plus every
/// `crates/*/src` member. `crates/rift-protocol/src/transport.rs` and its
/// siblings only run via `just test-crates`, never through `just test`'s
/// filters, so they get their own module-path prefix (the crate directory
/// name, matching the `-p` name `test-crates` passes to cargo).
fn discover_test_modules() -> Vec<String> {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut modules = walk_test_modules(&manifest_dir.join("src"), None);

    let crates_dir = manifest_dir.join("crates");
    let mut crate_dirs: Vec<std::path::PathBuf> = std::fs::read_dir(&crates_dir)
        .expect("crates/ must be readable")
        .map(|entry| entry.expect("readable crates/ entry").path())
        .filter(|path| path.is_dir())
        .collect();
    crate_dirs.sort();
    for crate_dir in crate_dirs {
        let crate_name = crate_dir
            .file_name()
            .expect("crate dir has a name")
            .to_string_lossy()
            .into_owned();
        let crate_src = crate_dir.join("src");
        if crate_src.is_dir() {
            modules.extend(walk_test_modules(&crate_src, Some(&crate_name)));
        }
    }
    modules
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

    let filters = recipe_filters(test_recipe());
    assert!(!filters.is_empty(), "the `test:` recipe names no filters");

    // `crates/*/src` modules run unfiltered via `test-crates`'s `-p` list, not
    // via `test:`'s filters, so a crate's package name classifies its whole
    // crate the same way a `test:` filter classifies a `src/` module.
    let crate_packages = test_crates_recipe_packages();

    let mut unclassified = Vec::new();
    for module in discover_test_modules() {
        let classified = filters
            .iter()
            .chain(GUI_OR_DEFERRED)
            .chain(crate_packages.iter())
            .any(|prefix| covers(prefix, &module));
        if !classified {
            unclassified.push(module);
        }
    }

    unclassified.sort();
    assert!(
        unclassified.is_empty(),
        "these test modules are in neither the `just test` allowlist nor \
         GUI_OR_DEFERRED, so they silently never run: {unclassified:#?}",
    );
}

/// Mirror image of the guard above: that one asserts every test module is
/// classified by a `test:` filter, `GUI_OR_DEFERRED`, or a `test-crates`
/// package; this one asserts every `test:` filter still classifies a real
/// module. A filter matching nothing is not an error to libtest — it
/// silently selects zero tests, the same failure shape
/// `just_test_skips_all_name_a_real_test` guards against for `--skip`, just
/// in the opposite direction.
///
/// This is about to matter for real: the upstream sync deletes
/// `src/actor/event_tap.rs` and `src/actor/gesture_tap.rs`, both named
/// directly in the `test:` recipe. Once those modules are gone, the filters
/// would keep matching nothing forever, silently and without complaint,
/// exactly like the dead `--skip` the other guard was written for.
///
/// Shares `discover_test_modules`/`covers`/`recipe_filters`/`test_recipe`
/// with the guard above rather than re-walking `src/` and `crates/*/src` a
/// second time.
#[test]
fn just_test_filters_all_name_a_real_module() {
    let filters = recipe_filters(test_recipe());
    assert!(!filters.is_empty(), "the `test:` recipe names no filters");

    let modules = discover_test_modules();

    let mut dead: Vec<&str> = filters
        .iter()
        .copied()
        .filter(|filter| !modules.iter().any(|module| covers(filter, module)))
        .collect();
    dead.sort();

    assert!(
        dead.is_empty(),
        "these `just test` filters match no test module under src/ or \
         crates/*/src, so they silently select nothing: {dead:#?}\n\
         Either the module was renamed or moved (re-point the filter at its new \
         path) or it is gone (delete the filter).",
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
    let recipe = include_str!("../../../justfile")
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

/// `FORK.md` §6 "Enforced guards" tabulates every TTSR rule in `.omp/rules/`.
/// A rule nobody documented is a rule nobody knows will interrupt them, and a
/// documented rule that no longer exists is a promise the repo does not keep.
///
/// This only checks *existence*, both directions — no test can verify that a
/// rule's argument still matches the policy it cites. That part is on you: the
/// `description` line is the contract, the body is the argument.
#[test]
fn every_ttsr_rule_is_documented_in_fork_md() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let rules_dir = root.join(".omp/rules");

    // Absent inside the nix build sandbox: `nix/package.nix` filters `src` to
    // `commonCargoSources` (*.rs / *.toml / Cargo.lock) plus plists, so `.omp/`
    // is genuinely not there. Skip rather than fail a legitimate build.
    let Ok(entries) = std::fs::read_dir(&rules_dir) else {
        return;
    };

    let doc = std::fs::read_to_string(root.join("FORK.md")).expect("FORK.md must exist");

    let mut on_disk: Vec<String> = Vec::new();
    for entry in entries {
        let path = entry.expect("readable dir entry").path();
        if path.extension().is_none_or(|ext| ext != "md" && ext != "mdc") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        on_disk.push(stem.to_owned());
    }
    assert!(
        !on_disk.is_empty(),
        "{} exists but holds no rules",
        rules_dir.display()
    );

    let undocumented: Vec<&String> =
        on_disk.iter().filter(|name| !doc.contains(name.as_str())).collect();
    assert!(
        undocumented.is_empty(),
        "these TTSR rules exist but FORK.md never mentions them, so nobody knows \
         they can interrupt a turn: {undocumented:#?}\n\
         Add a row to FORK.md §6 \"Enforced guards\".",
    );

    // The other direction: a row naming a rule file that is gone.
    let dangling: Vec<String> = doc
        .lines()
        .filter_map(|line| line.split('`').nth(1))
        .filter(|token| token.starts_with("fork-") && !token.contains(' '))
        .filter(|token| !on_disk.iter().any(|name| name == token))
        .map(str::to_owned)
        .collect();
    assert!(
        dangling.is_empty(),
        "FORK.md documents these rules but no file backs them in .omp/rules/: \
         {dangling:#?}\nEither restore the rule or drop its row.",
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
fn test_levenshtein_suggests() {
    let err = "unknown variant `toggle_stak`, expected one of `toggle_stack`, `toggle_orientation`";
    let token = Config::extract_unknown_variant(err).unwrap();
    assert_eq!(token, "toggle_stak||toggle_stack,toggle_orientation");
    let suggestion = Config::suggest_similar_command(&token);
    assert!(suggestion.is_some());
    let (s, _maybe_dep) = suggestion.unwrap();
    assert_eq!(s, "toggle_stack");
}

#[test]
fn stack_tabs_default_on_and_release_the_reservation_when_disabled() {
    let mut tabs = Config::parse("[keys]\n[settings]").unwrap().settings.ui.stack_line;
    assert!(tabs.enabled && tabs.validate().is_empty());
    assert_eq!(tabs.reserve().total(), 28.0);
    tabs.enabled = false;
    assert_eq!(tabs.reserve().total(), 0.0);
}

/// The stack line section of main's `rift.default.toml`, verbatim.
const MAIN_DEFAULT_STACK_LINE: &str = r#"
[keys]

[settings.ui.stack_line]
# experimental stack line indicator (defaults to off)
enabled = false
# action on hover: "click" or "hover" (defaults to "hover" which is just a visual indicator)
hover = "hover"
# in a horizontal stack where the line will be (top/bottom)
horiz_placement = "top"
# in a vertical stack where the line will be (left/right)
vert_placement = "left"
# how thick the indicator will be (in px)
thickness = 20.0
# spacing between the stack line and window edges (in px)
spacing = 1.0
# RGBA components range from 0.0 to 1.0. Each color can be customized
# selected_color = { r = 0.0, g = 0.5, b = 1.0, a = 1.0 }
# unselected_color = { r = 0.8, g = 0.8, b = 0.8, a = 1.0 }
# border_color = { r = 0.6, g = 0.6, b = 0.6, a = 1.0 }
"#;

#[test]
fn main_stack_line_configs_keep_their_meaning() {
    let uncommented = MAIN_DEFAULT_STACK_LINE
        .replace("# selected", "selected")
        .replace("# unselected", "unselected")
        .replace("# border", "border");
    let thin = uncommented.replace("20.0", "4").replace("false", "true");
    for (text, enabled) in [
        (MAIN_DEFAULT_STACK_LINE.into(), false),
        (uncommented, false),
        (thin, true),
    ] {
        let config = Config::parse(&text).unwrap();
        assert!(config.validate().is_empty(), "{:?}", config.validate());
        let tabs = &config.settings.ui.stack_line;
        assert_eq!(
            tabs.enabled, enabled,
            "an explicit setting beats the new default"
        );
        assert!(!tabs.hover.selects_on_hover(), "\"hover\" only highlights");
        assert_eq!((tabs.horiz_placement, tabs.vert_placement), Default::default());
        // Removed colors are dropped; indicator thicknesses rise to the 20 point minimum.
        assert_eq!((tabs.thickness, tabs.spacing), (20.0, 1.0));
        assert_eq!(tabs.reserve().total(), if enabled { 21.0 } else { 0.0 });
        let mut document = ConfigDocument::parse(&text).unwrap();
        document.update(|source| source.settings.ui.stack_line.spacing = 2.0).unwrap();
        assert_eq!(
            Config::parse(&document.to_string()).unwrap().settings.ui.stack_line.spacing,
            2.0
        );
    }
    assert!(Config::parse(&MAIN_DEFAULT_STACK_LINE.replace("hover =", "hover_color =")).is_err());
    let click: StackLineSettings = toml::from_str(r#"hover = "click""#).unwrap();
    assert!(click.hover.selects_on_hover());
}

#[test]
fn stack_tab_geometry_rejects_unusable_values() {
    for text in [
        "thickness = 19.5",
        "thickness = nan",
        "spacing = -1.0",
        "spacing = inf",
    ] {
        let tabs: StackLineSettings = toml::from_str(text).unwrap();
        assert_eq!(tabs.validate().len(), 1, "{text}");
    }
}

/// §12.6 HZ-2, the second data-losing hazard of the sixth sync.
///
/// Upstream's native tab bar (952fc17) has no colour or font API, so fork
/// feature C's theming keys were deleted from `StackLineSettings`. But
/// `ConfigSource` is `deny_unknown_fields`: every fork user's existing
/// `~/.config/rift/config.toml` still carries those keys, and without a
/// migration rift would fail to parse it at startup and silently fall back to
/// the default config — losing every other setting in the file too.
///
/// This is the pairing that had to land in one commit: the deletion is only
/// safe because `migrate_legacy_stack_line` strips the retired names.
#[test]
fn a_fork_era_stack_line_config_still_parses_after_feature_c_retirement() {
    let config = Config::parse(
        r##"
            [settings.ui.stack_line]
            enabled = true
            thickness = 20.0
            border_width = 0.5
            show_titles = true
            active_color = "#3d7bfaee"
            inactive_color = "#1f1f24ee"
            active_title_color = "#ffffff"
            inactive_title_color = "#b8b8be"
            font_size = 13.0

            [keys]
            "Alt + H" = { move_focus = "left" }
        "##,
    )
    .expect("a fork-era config must still parse after the retired keys are gone");

    assert!(
        config.settings.ui.stack_line.enabled,
        "surviving keys keep their values"
    );
    // Below upstream's floor: the migration raises it rather than rejecting.
    assert_eq!(
        config.settings.ui.stack_line.thickness,
        StackLineSettings::MIN_THICKNESS
    );
    assert!(!config.keys.is_empty(), "the rest of the file survives too");
}

/// §12.6 HZ-10, found on a LIVE host rather than by a fixture: the sixth sync
/// retired the fork's `width_presets` in favour of upstream's
/// `preset_column_widths`, and `ScrollingLayoutSettings` is
/// `deny_unknown_fields` — so without a migration every fork config written
/// before 0.7 is rejected at startup and rift silently runs on defaults.
///
/// The values must be CARRIED, not dropped: the two keys mean the same thing,
/// so a user's tuned presets have to keep working under the new name.
#[test]
fn fork_width_presets_migrate_to_upstream_preset_column_widths() {
    let config = Config::parse(
        r#"
            [settings.layout.scrolling]
            column_width_ratio = 0.7
            width_presets = [0.5, 0.85, 0.93]

            [keys]
        "#,
    )
    .expect("a pre-0.7 fork config must still parse");

    assert_eq!(
        config.settings.layout.scrolling.preset_column_widths,
        vec![0.5, 0.85, 0.93],
        "tuned presets must survive the rename, not reset to the default set"
    );
}
