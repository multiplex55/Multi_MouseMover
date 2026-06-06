use crate::key_chord::{KeyChord, ModifierRequirements, ModifierSideRequirement};
use crate::keyboard::{preserved_shortcut_risk_for_chord, VirtualKey};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ConfigAuditSeverity {
    Info,
    Warning,
    Deprecated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigAuditWarning {
    pub path: String,
    pub severity: ConfigAuditSeverity,
    pub message: String,
    pub suggestion: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ConfigAuditReport {
    pub warnings: Vec<ConfigAuditWarning>,
    pub keybind_report: KeybindAuditReport,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct KeybindAuditReport {
    pub issues: Vec<KeybindIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeybindIssueKind {
    DuplicateExactChord,
    OverlappingGenericAndSideSpecificModifier,
    PlainBindingShadowedByModifierRelaxation,
    ReservedWindowsShortcut,
    SystemBindingAlsoUsedAsActionBinding,
    BindingUsesOwnedModifier,
    BindingLikelyAltGrAmbiguous,
    BindingNotShownInHelpDueToLimit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeybindIssue {
    pub kind: KeybindIssueKind,
    pub severity: ConfigAuditSeverity,
    pub chord_display: String,
    pub action_id: String,
    pub action_name: String,
    pub message: String,
    pub suggestion: String,
    pub winning_chord_display: Option<String>,
    pub winning_action_id: Option<String>,
    pub winning_action_name: Option<String>,
}

impl KeybindIssue {
    fn warning(
        kind: KeybindIssueKind,
        chord_display: impl Into<String>,
        action_id: impl Into<String>,
        message: impl Into<String>,
        suggestion: impl Into<String>,
    ) -> Self {
        let action_id = action_id.into();
        Self {
            kind,
            severity: ConfigAuditSeverity::Warning,
            chord_display: chord_display.into(),
            action_name: action_id.clone(),
            action_id,
            message: message.into(),
            suggestion: suggestion.into(),
            winning_chord_display: None,
            winning_action_id: None,
            winning_action_name: None,
        }
    }

    fn with_severity(mut self, severity: ConfigAuditSeverity) -> Self {
        self.severity = severity;
        self
    }

    fn with_winner(
        mut self,
        chord_display: impl Into<String>,
        action_id: impl Into<String>,
    ) -> Self {
        let action_id = action_id.into();
        self.winning_chord_display = Some(chord_display.into());
        self.winning_action_name = Some(action_id.clone());
        self.winning_action_id = Some(action_id);
        self
    }
}

pub fn audit_config_toml(raw_toml: &str) -> ConfigAuditReport {
    let value = match raw_toml.parse::<toml::Value>() {
        Ok(value) => value,
        Err(err) => {
            return ConfigAuditReport {
                warnings: vec![ConfigAuditWarning {
                    path: "<toml>".to_string(),
                    severity: ConfigAuditSeverity::Warning,
                    message: format!("Config TOML could not be parsed: {err}"),
                    suggestion: "Fix the TOML syntax before changing config paths.".to_string(),
                }],
                keybind_report: KeybindAuditReport::default(),
            };
        }
    };

    let mut paths = Vec::new();
    flatten_paths(&value, "", &mut paths);

    let path_set: HashSet<&str> = paths.iter().map(String::as_str).collect();
    let mut report = ConfigAuditReport::default();

    for path in &paths {
        audit_explicit_path(path, &path_set, &mut report);
    }
    audit_key_binding_aliases(&value, &mut report);
    audit_keybind_report(&value, &mut report);

    for path in &paths {
        if is_explicitly_audited_path(path, &path_set) || is_known_active_path(path) {
            continue;
        }

        report.warnings.push(ConfigAuditWarning {
            path: path.to_string(),
            severity: ConfigAuditSeverity::Warning,
            message: format!(
                "Unknown config path '{path}' is not used by the current config schema."
            ),
            suggestion: "Remove it or move it to a documented config path.".to_string(),
        });
    }

    report
}

#[derive(Debug, Clone)]
struct ParsedKeyBinding {
    index: usize,
    chord_source: String,
    action: String,
    chord: KeyChord,
}

fn audit_keybind_report(value: &toml::Value, report: &mut ConfigAuditReport) {
    let parsed = parsed_key_bindings(value);
    audit_keybind_duplicates(&parsed, report);
    audit_keybind_overlaps(&parsed, report);
    audit_shift_relaxation_shadowing(&parsed, report);
    audit_reserved_windows_shortcuts(&parsed, report);
    audit_system_binding_reuse(value, &parsed, report);
    audit_owned_modifier_bindings(&parsed, report);
    audit_altgr_ambiguity(&parsed, report);
    audit_help_limit(value, &parsed, report);
}

fn parsed_key_bindings(value: &toml::Value) -> Vec<ParsedKeyBinding> {
    let Some(bindings) = value.get("key_bindings").and_then(toml::Value::as_array) else {
        return Vec::new();
    };

    bindings
        .iter()
        .enumerate()
        .filter_map(|(index, binding)| {
            let items = binding.as_array()?;
            let chord_source = items.first()?.as_str()?;
            let action = items.get(1)?.as_str()?;
            let chord = KeyChord::parse(chord_source).ok()?;
            Some(ParsedKeyBinding {
                index,
                chord_source: chord_source.to_string(),
                action: action.to_string(),
                chord,
            })
        })
        .collect()
}

fn audit_keybind_duplicates(parsed: &[ParsedKeyBinding], report: &mut ConfigAuditReport) {
    let mut chord_indexes: HashMap<KeyChord, Vec<&ParsedKeyBinding>> = HashMap::new();
    for binding in parsed {
        chord_indexes
            .entry(binding.chord)
            .or_default()
            .push(binding);
    }

    let mut duplicates: Vec<Vec<&ParsedKeyBinding>> = chord_indexes
        .into_values()
        .filter(|bindings| bindings.len() > 1)
        .collect();
    duplicates.sort_by(|left, right| left[0].chord_source.cmp(&right[0].chord_source));

    for bindings in duplicates {
        let winner = bindings.last().expect("duplicate group has a winner");
        let binding_refs = bindings
            .iter()
            .map(|binding| format!("key_bindings[{}]", binding.index))
            .collect::<Vec<_>>()
            .join(", ");
        let chord_display = bindings[0].chord.display_label();
        let message = format!(
            "Duplicate key chord '{chord_display}' appears in multiple bindings ({binding_refs}); later entry wins."
        );
        let suggestion = "Use unique chords in key_bindings to avoid unintentional overrides.";
        report.warnings.push(ConfigAuditWarning {
            path: "key_bindings".to_string(),
            severity: ConfigAuditSeverity::Warning,
            message: message.clone(),
            suggestion: suggestion.to_string(),
        });
        report.keybind_report.issues.push(
            KeybindIssue::warning(
                KeybindIssueKind::DuplicateExactChord,
                chord_display,
                bindings[0].action.clone(),
                message,
                suggestion,
            )
            .with_winner(winner.chord.display_label(), winner.action.clone()),
        );
    }
}

fn audit_keybind_overlaps(parsed: &[ParsedKeyBinding], report: &mut ConfigAuditReport) {
    for i in 0..parsed.len() {
        for j in (i + 1)..parsed.len() {
            let left = &parsed[i];
            let right = &parsed[j];
            if left.chord.key != right.chord.key || left.chord == right.chord {
                continue;
            }
            if !chords_overlap_by_generic_and_side_specific_modifier(left.chord, right.chord) {
                continue;
            }
            let winner = winning_binding_for_overlap(left, right);
            let loser = if winner.index == left.index {
                right
            } else {
                left
            };
            let message = format!(
                "Key chord overlap: '{}' ({}) overlaps '{}' ({}); both may match a single event, and the more specific chord wins ('{}') due to resolver specificity.",
                left.chord.display_label(),
                left.action,
                right.chord.display_label(),
                right.action,
                winner.chord.display_label()
            );
            let suggestion = format!(
                "Review key_bindings[{}] and key_bindings[{}]: avoid mixing generic and side-specific modifiers for the same key unless the winner is intentional.",
                left.index, right.index
            );
            report.warnings.push(ConfigAuditWarning {
                path: "key_bindings".to_string(),
                severity: ConfigAuditSeverity::Warning,
                message: message.clone(),
                suggestion: suggestion.clone(),
            });
            report.keybind_report.issues.push(
                KeybindIssue::warning(
                    KeybindIssueKind::OverlappingGenericAndSideSpecificModifier,
                    loser.chord.display_label(),
                    loser.action.clone(),
                    message,
                    suggestion,
                )
                .with_winner(winner.chord.display_label(), winner.action.clone()),
            );
        }
    }
}

fn audit_shift_relaxation_shadowing(parsed: &[ParsedKeyBinding], report: &mut ConfigAuditReport) {
    for plain in parsed
        .iter()
        .filter(|binding| is_plain_chord(binding.chord))
    {
        for shifted in parsed.iter().filter(|binding| {
            binding.chord.key == plain.chord.key
                && binding.chord.modifiers.shift != ModifierSideRequirement::NotRequired
        }) {
            let message = format!(
                "Plain binding '{}' can also match shifted events when input.shift_can_modify_plain_movement is enabled, but '{}' wins for Shift+{}.",
                plain.chord.display_label(),
                shifted.chord.display_label(),
                shifted.chord.display_label().rsplit('+').next().unwrap_or("key")
            );
            let suggestion = "Use separate non-overlapping keys or disable input.shift_can_modify_plain_movement if shifted movement should never share plain bindings.";
            report.keybind_report.issues.push(
                KeybindIssue::warning(
                    KeybindIssueKind::PlainBindingShadowedByModifierRelaxation,
                    plain.chord.display_label(),
                    plain.action.clone(),
                    message.clone(),
                    suggestion,
                )
                .with_winner(shifted.chord.display_label(), shifted.action.clone()),
            );
            report.warnings.push(ConfigAuditWarning {
                path: "key_bindings".to_string(),
                severity: ConfigAuditSeverity::Warning,
                message,
                suggestion: suggestion.to_string(),
            });
        }
    }
}

fn audit_reserved_windows_shortcuts(parsed: &[ParsedKeyBinding], report: &mut ConfigAuditReport) {
    for binding in parsed {
        if let Some(risk) = preserved_shortcut_risk_for_chord(&binding.chord) {
            let message = format!(
                "Binding '{}' for '{}' may conflict with a preserved Windows/app shortcut: {risk}.",
                binding.chord.display_label(),
                binding.action
            );
            let suggestion = "Choose a non-reserved chord, or make this a deliberate system binding if it must override preserved shortcuts.";
            push_keybind_issue(
                report,
                binding,
                KeybindIssueKind::ReservedWindowsShortcut,
                message,
                suggestion,
            );
        }
    }
}

fn audit_system_binding_reuse(
    value: &toml::Value,
    parsed: &[ParsedKeyBinding],
    report: &mut ConfigAuditReport,
) {
    let system_bindings = system_bindings_from_value(value);
    for (system_name, system_chord) in system_bindings {
        for binding in parsed
            .iter()
            .filter(|binding| binding.chord == system_chord)
        {
            let message = format!(
                "Action binding '{}' for '{}' reuses system binding '{}'. System routing wins before action dispatch.",
                binding.chord.display_label(), binding.action, system_name
            );
            let suggestion = format!(
                "Move either key_bindings[{}] or system_bindings.{system_name} to a unique chord.",
                binding.index
            );
            report.warnings.push(ConfigAuditWarning {
                path: "key_bindings".to_string(),
                severity: ConfigAuditSeverity::Warning,
                message: message.clone(),
                suggestion: suggestion.clone(),
            });
            report.keybind_report.issues.push(
                KeybindIssue::warning(
                    KeybindIssueKind::SystemBindingAlsoUsedAsActionBinding,
                    binding.chord.display_label(),
                    binding.action.clone(),
                    message,
                    suggestion,
                )
                .with_winner(binding.chord.display_label(), system_name.to_string()),
            );
        }
    }
}

fn audit_owned_modifier_bindings(parsed: &[ParsedKeyBinding], report: &mut ConfigAuditReport) {
    for binding in parsed
        .iter()
        .filter(|binding| is_modifier_key(binding.chord.key))
    {
        let message = format!(
            "Binding '{}' uses a modifier key as the trigger; owned-modifier swallowing can make this surprising while the binding is active.",
            binding.chord.display_label()
        );
        let suggestion = "Prefer a non-modifier trigger key, or verify input.swallow_owned_modifiers behavior matches your workflow.";
        push_keybind_issue(
            report,
            binding,
            KeybindIssueKind::BindingUsesOwnedModifier,
            message,
            suggestion,
        );
    }
}

fn audit_altgr_ambiguity(parsed: &[ParsedKeyBinding], report: &mut ConfigAuditReport) {
    let ambiguous: Vec<&ParsedKeyBinding> = parsed
        .iter()
        .filter(|binding| {
            binding.chord.modifiers.alt == ModifierSideRequirement::Right
                || (binding.chord.modifiers.alt != ModifierSideRequirement::NotRequired
                    && binding.chord.modifiers.ctrl != ModifierSideRequirement::NotRequired)
        })
        .collect();
    if ambiguous.is_empty() {
        return;
    }

    for binding in ambiguous {
        let message = format!(
            "AltGr advisory: '{}' may be ambiguous on keyboard layouts that report AltGr as RightAlt+Ctrl.",
            binding.chord.display_label()
        );
        let suggestion = "Prefer explicit side requirements that do not compete with AltGr text input, or avoid AltGr-adjacent chords for actions used while typing.";
        report.warnings.push(ConfigAuditWarning {
            path: "key_bindings".to_string(),
            severity: ConfigAuditSeverity::Info,
            message: message.clone(),
            suggestion: suggestion.to_string(),
        });
        report.keybind_report.issues.push(
            KeybindIssue::warning(
                KeybindIssueKind::BindingLikelyAltGrAmbiguous,
                binding.chord.display_label(),
                binding.action.clone(),
                message,
                suggestion,
            )
            .with_severity(ConfigAuditSeverity::Info),
        );
    }
}

fn audit_help_limit(
    value: &toml::Value,
    parsed: &[ParsedKeyBinding],
    report: &mut ConfigAuditReport,
) {
    let max_bindings = value
        .get("tooltip_overlay")
        .and_then(|tooltip| tooltip.get("help_max_bindings"))
        .and_then(toml::Value::as_integer)
        .unwrap_or(40);
    if max_bindings <= 0 || parsed.len() as i64 <= max_bindings {
        return;
    }

    let hidden_count = parsed.len() as i64 - max_bindings;
    let message = format!(
        "Help overlay will show at most {max_bindings} bindings, hiding {hidden_count} of {} configured bindings.",
        parsed.len()
    );
    let suggestion = "Raise tooltip_overlay.help_max_bindings or split bindings across a paged/help workflow when available.";
    report.warnings.push(ConfigAuditWarning {
        path: "tooltip_overlay.help_max_bindings".to_string(),
        severity: ConfigAuditSeverity::Info,
        message: message.clone(),
        suggestion: suggestion.to_string(),
    });
    report.keybind_report.issues.push(
        KeybindIssue::warning(
            KeybindIssueKind::BindingNotShownInHelpDueToLimit,
            "<help overlay>",
            "show_help",
            message,
            suggestion,
        )
        .with_severity(ConfigAuditSeverity::Info),
    );
}

fn push_keybind_issue(
    report: &mut ConfigAuditReport,
    binding: &ParsedKeyBinding,
    kind: KeybindIssueKind,
    message: String,
    suggestion: &str,
) {
    report.warnings.push(ConfigAuditWarning {
        path: "key_bindings".to_string(),
        severity: ConfigAuditSeverity::Warning,
        message: message.clone(),
        suggestion: suggestion.to_string(),
    });
    report.keybind_report.issues.push(KeybindIssue::warning(
        kind,
        binding.chord.display_label(),
        binding.action.clone(),
        message,
        suggestion,
    ));
}

fn system_bindings_from_value(value: &toml::Value) -> Vec<(&'static str, KeyChord)> {
    let system = value.get("system_bindings");
    [
        (
            "toggle_active",
            system
                .and_then(|s| s.get("toggle_active"))
                .and_then(toml::Value::as_str)
                .unwrap_or("Ctrl+Q"),
        ),
        (
            "exit",
            system
                .and_then(|s| s.get("exit"))
                .and_then(toml::Value::as_str)
                .unwrap_or("Ctrl+Escape"),
        ),
        (
            "panic_reset",
            system
                .and_then(|s| s.get("panic_reset"))
                .and_then(toml::Value::as_str)
                .unwrap_or("RightAlt+Escape"),
        ),
    ]
    .into_iter()
    .filter_map(|(name, chord)| KeyChord::parse(chord).ok().map(|chord| (name, chord)))
    .collect()
}

fn chords_overlap_by_generic_and_side_specific_modifier(left: KeyChord, right: KeyChord) -> bool {
    modifier_requirements_overlap(left.modifiers.ctrl, right.modifiers.ctrl)
        && modifier_requirements_overlap(left.modifiers.alt, right.modifiers.alt)
        && modifier_requirements_overlap(left.modifiers.shift, right.modifiers.shift)
        && modifier_requirements_overlap(left.modifiers.win, right.modifiers.win)
        && [
            (left.modifiers.ctrl, right.modifiers.ctrl),
            (left.modifiers.alt, right.modifiers.alt),
            (left.modifiers.shift, right.modifiers.shift),
            (left.modifiers.win, right.modifiers.win),
        ]
        .iter()
        .any(|(left, right)| {
            matches!(
                (left, right),
                (
                    ModifierSideRequirement::Any,
                    ModifierSideRequirement::Left | ModifierSideRequirement::Right
                ) | (
                    ModifierSideRequirement::Left | ModifierSideRequirement::Right,
                    ModifierSideRequirement::Any
                )
            )
        })
}

fn modifier_requirements_overlap(
    left: ModifierSideRequirement,
    right: ModifierSideRequirement,
) -> bool {
    matches!(
        (left, right),
        (
            ModifierSideRequirement::NotRequired,
            ModifierSideRequirement::NotRequired
        ) | (ModifierSideRequirement::Any, ModifierSideRequirement::Any)
            | (
                ModifierSideRequirement::Any,
                ModifierSideRequirement::Left | ModifierSideRequirement::Right
            )
            | (
                ModifierSideRequirement::Left | ModifierSideRequirement::Right,
                ModifierSideRequirement::Any
            )
            | (ModifierSideRequirement::Left, ModifierSideRequirement::Left)
            | (
                ModifierSideRequirement::Right,
                ModifierSideRequirement::Right
            )
    )
}

fn winning_binding_for_overlap<'a>(
    left: &'a ParsedKeyBinding,
    right: &'a ParsedKeyBinding,
) -> &'a ParsedKeyBinding {
    match left.chord.specificity().cmp(&right.chord.specificity()) {
        std::cmp::Ordering::Greater => left,
        std::cmp::Ordering::Less => right,
        std::cmp::Ordering::Equal if left.index <= right.index => left,
        std::cmp::Ordering::Equal => right,
    }
}

fn is_plain_chord(chord: KeyChord) -> bool {
    chord.modifiers == ModifierRequirements::default()
}

fn is_modifier_key(key: VirtualKey) -> bool {
    matches!(
        key,
        VirtualKey::Alt
            | VirtualKey::LeftAlt
            | VirtualKey::RightAlt
            | VirtualKey::Ctrl
            | VirtualKey::LeftCtrl
            | VirtualKey::RightCtrl
            | VirtualKey::Shift
            | VirtualKey::LeftShift
            | VirtualKey::RightShift
            | VirtualKey::LeftWin
            | VirtualKey::RightWin
    )
}

fn audit_key_binding_aliases(value: &toml::Value, report: &mut ConfigAuditReport) {
    let Some(bindings) = value.get("key_bindings").and_then(toml::Value::as_array) else {
        return;
    };
    for (index, binding) in bindings.iter().enumerate() {
        let Some(items) = binding.as_array() else {
            continue;
        };
        let Some(action) = items.get(1).and_then(toml::Value::as_str) else {
            continue;
        };
        if action.eq_ignore_ascii_case("hints") {
            report.warnings.push(replacement_warning(
                &format!("key_bindings[{index}][1]"),
                ConfigAuditSeverity::Info,
                "\"hints\" maps to the help overlay action.",
                "Use \"ui_hint_mode\" for UI hint mode bindings.",
            ));
        }
    }
}

fn flatten_paths(value: &toml::Value, prefix: &str, paths: &mut Vec<String>) {
    match value {
        toml::Value::Table(table) => {
            if table.is_empty() && !prefix.is_empty() {
                paths.push(prefix.to_string());
                return;
            }

            for (key, value) in table {
                let path = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_paths(value, &path, paths);
            }
        }
        _ if !prefix.is_empty() => paths.push(prefix.to_string()),
        _ => {}
    }
}

fn audit_explicit_path(path: &str, path_set: &HashSet<&str>, report: &mut ConfigAuditReport) {
    match path {
        "system_bindings.polling_rate" => report.warnings.push(replacement_warning(
            path,
            ConfigAuditSeverity::Warning,
            "polling_rate is not read under [system_bindings].",
            "Move system_bindings.polling_rate to top-level polling_rate.",
        )),
        "system_bindings.grid_size"
        | "system_bindings.grid_size.width"
        | "system_bindings.grid_size.height" => {
            report.warnings.push(replacement_warning(
                path,
                ConfigAuditSeverity::Warning,
                "grid_size is not read under [system_bindings].",
                "Move system_bindings.grid_size to top-level grid_size, or prefer [jump.coarse].width and [jump.coarse].height.",
            ));
        }
        "grid_mode.starting_speed" => report.warnings.push(replacement_warning(
            path,
            ConfigAuditSeverity::Warning,
            "starting_speed is not read under [grid_mode].",
            "Move grid_mode.starting_speed to [mouse_speed].default_speed.",
        )),
        "grid_mode.acceleration" => report.warnings.push(replacement_warning(
            path,
            ConfigAuditSeverity::Warning,
            "acceleration is not read under [grid_mode].",
            "Move grid_mode.acceleration to top-level acceleration.",
        )),
        "grid_mode.acceleration_rate" => report.warnings.push(replacement_warning(
            path,
            ConfigAuditSeverity::Warning,
            "acceleration_rate is not read under [grid_mode].",
            "Move grid_mode.acceleration_rate to top-level acceleration_rate.",
        )),
        "grid_mode.top_speed" => report.warnings.push(replacement_warning(
            path,
            ConfigAuditSeverity::Warning,
            "top_speed is not read under [grid_mode].",
            "Move grid_mode.top_speed to top-level top_speed.",
        )),
        "jump.move_cursor_after_each_stage" => report.warnings.push(replacement_warning(
            path,
            ConfigAuditSeverity::Deprecated,
            "jump.move_cursor_after_each_stage is a deprecated alias.",
            "Replace jump.move_cursor_after_each_stage with jump.cursor_between_stages.",
        )),
        _ if path.ends_with(".preview_margin_percent") || path == "preview_margin_percent" => {
            report.warnings.push(replacement_warning(
                path,
                ConfigAuditSeverity::Deprecated,
                "preview_margin_percent is a deprecated alias.",
                &format!(
                    "Replace {path} with {}.",
                    path.strip_suffix("preview_margin_percent")
                        .map(|prefix| format!("{prefix}visual_context_margin_percent"))
                        .unwrap_or_else(|| "visual_context_margin_percent".to_string())
                ),
            ));
        }
        "grid_size.width" | "grid_size.height"
            if path_set.contains("jump.coarse.width")
                || path_set.contains("jump.coarse.height") =>
        {
            report.warnings.push(replacement_warning(
                path,
                ConfigAuditSeverity::Deprecated,
                "top-level grid_size is a legacy coarse-grid fallback and is ignored when [jump.coarse] is present.",
                "Remove grid_size and use jump.coarse.width and jump.coarse.height.",
            ));
        }
        _ => {}
    }
}

fn replacement_warning(
    path: &str,
    severity: ConfigAuditSeverity,
    message: &str,
    suggestion: &str,
) -> ConfigAuditWarning {
    ConfigAuditWarning {
        path: path.to_string(),
        severity,
        message: message.to_string(),
        suggestion: suggestion.to_string(),
    }
}

fn is_explicitly_audited_path(path: &str, path_set: &HashSet<&str>) -> bool {
    matches!(
        path,
        "system_bindings.polling_rate"
            | "system_bindings.grid_size"
            | "system_bindings.grid_size.width"
            | "system_bindings.grid_size.height"
            | "grid_mode.starting_speed"
            | "grid_mode.acceleration"
            | "grid_mode.acceleration_rate"
            | "grid_mode.top_speed"
            | "jump.move_cursor_after_each_stage"
    ) || path.ends_with(".preview_margin_percent")
        || path == "preview_margin_percent"
        || (matches!(path, "grid_size.width" | "grid_size.height")
            && (path_set.contains("jump.coarse.width") || path_set.contains("jump.coarse.height")))
}

fn is_known_active_path(path: &str) -> bool {
    const KNOWN_PATHS: &[&str] = &[
        "key_bindings",
        "polling_rate",
        "grid_size.width",
        "grid_size.height",
        "starting_speed",
        "acceleration",
        "acceleration_rate",
        "top_speed",
        "system_bindings.toggle_active",
        "system_bindings.exit",
        "system_bindings.exit_ignore_extra_modifiers",
        "system_bindings.panic_reset",
        "system_bindings.panic_reset_ignore_extra_modifiers",
        "system_bindings.panic_reset_sets_idle",
        "input.swallow_owned_modifiers",
        "input.modifier_reconcile_on_tick",
        "input.modifier_reconcile_interval_ms",
        "input.stuck_key_timeout_ms",
        "input.debug_input",
        "input.shift_can_modify_plain_movement",
        "input.right_alt_suppresses_synthetic_ctrl",
        "grid_mode.enabled",
        "grid_mode.start_region",
        "grid_mode.width_percent",
        "grid_mode.height_percent",
        "grid_mode.center_on_cursor",
        "grid_mode.min_width_px",
        "grid_mode.min_height_px",
        "grid_mode.move_cursor_each_step",
        "grid_mode.line_visible",
        "grid_mode.show_direction_labels",
        "mouse_speed.default_speed",
        "mouse_speed.min_speed",
        "mouse_speed.max_speed",
        "mouse_speed.speed_step",
        "mouse_speed.flash_indicator_ms",
        "slow_mouse.strategy",
        "slow_mouse.fixed_speed",
        "slow_mouse.multiplier",
        "slow_mouse.subtract_speed",
        "slow_mouse.min_speed",
        "slow_mouse.max_speed",
        "slow_mouse.acceleration",
        "slow_mouse.acceleration_rate",
        "surgical_mode.enabled",
        "surgical_mode.speed_px",
        "surgical_mode.zoom_enabled",
        "surgical_mode.zoom_scale",
        "surgical_mode.zoom_size_px",
        "surgical_mode.overlay_offset_x",
        "surgical_mode.overlay_offset_y",
        "surgical_mode.refresh_interval_ms",
        "surgical_mode.center_crosshair",
        "movement_profiles.*.default_speed",
        "movement_profiles.*.min_speed",
        "movement_profiles.*.max_speed",
        "movement_profiles.*.speed_step",
        "movement_profiles.*.flash_indicator_ms",
        "wheel.default_speed",
        "wheel.min_speed",
        "wheel.max_speed",
        "wheel.speed_step",
        "wheel.tick_interval",
        "wheel.speed_indicator_ms",
        "wheel.vertical_multiplier",
        "wheel.horizontal_multiplier",
        "wheel_profiles.*.default_speed",
        "wheel_profiles.*.min_speed",
        "wheel_profiles.*.max_speed",
        "wheel_profiles.*.speed_step",
        "wheel_profiles.*.tick_interval",
        "wheel_profiles.*.speed_indicator_ms",
        "wheel_profiles.*.vertical_multiplier",
        "wheel_profiles.*.horizontal_multiplier",
        "scroll_mode.enabled",
        "scroll_mode.modifier_action",
        "scroll_mode.exit_on_click",
        "scroll_mode.exclusive_with_jump_mode",
        "scroll_mode.exclusive_with_grid_mode",
        "scroll_mode.exclusive_with_ui_hint_mode",
        "edge_jump.offset_px",
        "edge_jump.use_work_area",
        "window_jump.enabled",
        "window_jump.use_extended_frame_bounds",
        "window_jump.edge_offset_px",
        "window_jump.titlebar_y_offset_px",
        "window_jump.clamp_to_window",
        "final_adjust.enabled",
        "final_adjust.small_step_px",
        "final_adjust.large_step_px",
        "final_adjust.modifier_key",
        "final_adjust.confirm_key",
        "final_adjust.cancel_key",
        "final_adjust.back_key",
        "final_adjust.show_hint",
        "step_move.enabled",
        "step_move.small_step_px",
        "step_move.normal_step_px",
        "step_move.large_step_px",
        "step_move.clamp_mode",
        "step_move.show_tooltip",
        "status_overlay.visibility",
        "status_overlay.mode",
        "status_overlay.positioning",
        "status_overlay.flash_duration_ms",
        "status_overlay.fields.active",
        "status_overlay.fields.drag",
        "status_overlay.fields.slow",
        "status_overlay.fields.surgical",
        "status_overlay.fields.jump",
        "status_overlay.fields.mouse_speed",
        "status_overlay.fields.wheel_speed",
        "status_overlay.fields.flash",
        "status_overlay.fields.final_adjust",
        "status_overlay.fields.bookmark_mode",
        "tooltip_overlay.enabled",
        "tooltip_overlay.show_temporary_tooltips",
        "tooltip_overlay.show_help",
        "tooltip_overlay.positioning",
        "tooltip_overlay.offset_x",
        "tooltip_overlay.offset_y",
        "tooltip_overlay.duration_ms",
        "tooltip_overlay.help_positioning",
        "tooltip_overlay.help_width",
        "tooltip_overlay.help_max_bindings",
        "tooltip_overlay.mode_cards.enabled",
        "tooltip_overlay.mode_cards.show_bookmark_mode",
        "tooltip_overlay.mode_cards.show_jump_mode",
        "tooltip_overlay.mode_cards.show_grid_mode",
        "tooltip_overlay.mode_cards.show_ui_hint_mode",
        "tooltip_overlay.mode_cards.show_final_adjust",
        "tooltip_overlay.mode_cards.duration_ms",
        "tooltip_overlay.help.interactive",
        "tooltip_overlay.help.page_size",
        "tooltip_overlay.help.show_unbound_actions",
        "tooltip_overlay.help.show_conflicts",
        "tooltip_overlay.help.show_mode_specific_sections",
        "tooltip_overlay.events.mouse",
        "tooltip_overlay.events.wheel",
        "tooltip_overlay.events.profile",
        "tooltip_overlay.events.drag",
        "tooltip_overlay.events.reload",
        "tooltip_overlay.events.panic",
        "tooltip_overlay.events.surgical",
        "tooltip_overlay.events.ui_hints_query_start",
        "tooltip_overlay.events.ui_hints_query_fail",
        "tooltip_overlay.events.ui_hints_query_empty",
        "tooltip_overlay.events.ui_hints_query_capped_count",
        "tooltip_overlay.events.bookmarks",
        "ui_hints.enabled",
        "ui_hints.debug",
        "ui_hints.debug_fake_targets",
        "ui_hints.selection_keys",
        "ui_hints.label_length",
        "ui_hints.overflow_behavior",
        "ui_hints.max_hints",
        "ui_hints.min_hint_spacing_px",
        "ui_hints.include_thread_windows",
        "ui_hints.include_owned_popups",
        "ui_hints.query_strategy",
        "ui_hints.min_targets_before_fallback",
        "ui_hints.query_timeout_ms",
        "ui_hints.target_point",
        "ui_hints.after_select",
        "ui_hints.left_click_modifier.ctrl",
        "ui_hints.left_click_modifier.alt",
        "ui_hints.left_click_modifier.right_alt",
        "ui_hints.left_click_modifier.shift",
        "ui_hints.left_click_modifier.win",
        "ui_hints.right_click_modifier.ctrl",
        "ui_hints.right_click_modifier.alt",
        "ui_hints.right_click_modifier.right_alt",
        "ui_hints.right_click_modifier.shift",
        "ui_hints.right_click_modifier.win",
        "ui_hints.middle_click_modifier.ctrl",
        "ui_hints.middle_click_modifier.alt",
        "ui_hints.middle_click_modifier.right_alt",
        "ui_hints.middle_click_modifier.shift",
        "ui_hints.middle_click_modifier.win",
        "ui_hints.browse_modifier.ctrl",
        "ui_hints.browse_modifier.alt",
        "ui_hints.browse_modifier.right_alt",
        "ui_hints.browse_modifier.shift",
        "ui_hints.browse_modifier.win",
        "ui_hints.repeat_click_window_ms",
        "ui_hints.overlay.font_scale",
        "ui_hints.overlay.offset_x",
        "ui_hints.overlay.offset_y",
        "ui_hints.overlay.dim_non_matching",
        "ui_hints.overlay.show_background",
        "ui_hints.overlay.show_border",
        "bookmarks.enabled",
        "bookmarks.file",
        "bookmarks.slot_count",
        "bookmarks.show_tooltips",
        "bookmarks.prompt_for_name_on_set",
        "bookmarks.prompt_for_name_on_save",
        "bookmarks.allow_name_updates",
        "bookmarks.name_prompt_timeout_ms",
        "bookmarks.max_name_length",
        "bookmarks.desktop_behavior",
        "bookmarks.desktop_switch_wait_ms",
        "bookmarks.require_desktop_switch_success",
        "bookmarks.cancel_key",
        "bookmarks.clear_modifier_key",
        "bookmarks.coordinate_policy",
        "bookmarks.list_show_coordinates",
        "bookmarks.list_include_empty_slots",
        "bookmarks.list_empty_slot_label",
        "bookmarks.list_unnamed_label",
        "bookmarks.list_tooltip_duration_ms",
        "bookmark_markers.enabled",
        "bookmark_markers.show_with_help",
        "bookmark_markers.show_with_show_bookmarks",
        "bookmark_markers.show_with_bookmark_mode",
        "bookmark_markers.filter_current_virtual_desktop",
        "bookmark_markers.hide_offscreen",
        "bookmark_markers.position_source",
        "bookmark_markers.shape",
        "bookmark_markers.size_px",
        "bookmark_markers.opacity",
        "bookmark_markers.fill_color",
        "bookmark_markers.text_color",
        "bookmark_markers.border_color",
        "bookmark_markers.border_width_px",
        "bookmark_markers.font_scale",
        "bookmark_markers.offset_x",
        "bookmark_markers.offset_y",
        "bookmark_markers.center_on_bookmark",
        "bookmark_markers.slot_styles.*.fill_color",
        "bookmark_markers.slot_styles.*.text_color",
        "bookmark_markers.slot_styles.*.border_color",
        "bookmark_markers.slot_styles.*.opacity",
        "jump.mode",
        "jump.cursor_between_stages",
        "jump.start_region",
        "jump.preview_edge_behavior",
        "jump.hints.selection_keys",
        "jump.visuals.selected_region_outline",
        "jump.visuals.preview_outline",
        "jump.visuals.active_grid_outline",
        "jump.visuals.cell_centers",
        "jump.visuals.final_crosshair",
        "jump.coarse.enabled",
        "jump.coarse.width",
        "jump.coarse.height",
        "jump.coarse.aim_point",
        "jump.coarse.aim_offset_x_px",
        "jump.coarse.aim_offset_y_px",
        "jump.coarse.target_region_mode",
        "jump.coarse.target_margin_percent",
        "jump.coarse.visual_context_margin_percent",
        "jump.coarse.zoom_scale",
        "jump.coarse.preview_edge_behavior",
        "jump.coarse.labels.font_scale",
        "jump.coarse.labels.center_marker",
        "jump.coarse.labels.separators",
        "jump.coarse.labels.hide_threshold_px",
        "jump.fine.enabled",
        "jump.fine.width",
        "jump.fine.height",
        "jump.fine.aim_point",
        "jump.fine.aim_offset_x_px",
        "jump.fine.aim_offset_y_px",
        "jump.fine.target_region_mode",
        "jump.fine.target_margin_percent",
        "jump.fine.visual_context_margin_percent",
        "jump.fine.zoom_scale",
        "jump.fine.preview_edge_behavior",
        "jump.fine.labels.font_scale",
        "jump.fine.labels.center_marker",
        "jump.fine.labels.separators",
        "jump.fine.labels.hide_threshold_px",
        "jump.precise.enabled",
        "jump.precise.width",
        "jump.precise.height",
        "jump.precise.aim_point",
        "jump.precise.aim_offset_x_px",
        "jump.precise.aim_offset_y_px",
        "jump.precise.target_region_mode",
        "jump.precise.target_margin_percent",
        "jump.precise.visual_context_margin_percent",
        "jump.precise.zoom_scale",
        "jump.precise.preview_edge_behavior",
        "jump.precise.labels.font_scale",
        "jump.precise.labels.center_marker",
        "jump.precise.labels.separators",
        "jump.precise.labels.hide_threshold_px",
        "jump.profiles.*.mode",
        "jump.profiles.*.cursor_between_stages",
        "jump.profiles.*.start_region",
        "jump.profiles.*.preview_edge_behavior",
        "jump.profiles.*.visuals.selected_region_outline",
        "jump.profiles.*.visuals.preview_outline",
        "jump.profiles.*.visuals.active_grid_outline",
        "jump.profiles.*.visuals.cell_centers",
        "jump.profiles.*.visuals.final_crosshair",
        "jump.profiles.*.coarse.enabled",
        "jump.profiles.*.coarse.width",
        "jump.profiles.*.coarse.height",
        "jump.profiles.*.coarse.aim_point",
        "jump.profiles.*.coarse.aim_offset_x_px",
        "jump.profiles.*.coarse.aim_offset_y_px",
        "jump.profiles.*.coarse.target_region_mode",
        "jump.profiles.*.coarse.target_margin_percent",
        "jump.profiles.*.coarse.visual_context_margin_percent",
        "jump.profiles.*.coarse.zoom_scale",
        "jump.profiles.*.coarse.preview_edge_behavior",
        "jump.profiles.*.coarse.labels.font_scale",
        "jump.profiles.*.coarse.labels.center_marker",
        "jump.profiles.*.coarse.labels.separators",
        "jump.profiles.*.coarse.labels.hide_threshold_px",
        "jump.profiles.*.fine.enabled",
        "jump.profiles.*.fine.width",
        "jump.profiles.*.fine.height",
        "jump.profiles.*.fine.aim_point",
        "jump.profiles.*.fine.aim_offset_x_px",
        "jump.profiles.*.fine.aim_offset_y_px",
        "jump.profiles.*.fine.target_region_mode",
        "jump.profiles.*.fine.target_margin_percent",
        "jump.profiles.*.fine.visual_context_margin_percent",
        "jump.profiles.*.fine.zoom_scale",
        "jump.profiles.*.fine.preview_edge_behavior",
        "jump.profiles.*.fine.labels.font_scale",
        "jump.profiles.*.fine.labels.center_marker",
        "jump.profiles.*.fine.labels.separators",
        "jump.profiles.*.fine.labels.hide_threshold_px",
        "jump.profiles.*.precise.enabled",
        "jump.profiles.*.precise.width",
        "jump.profiles.*.precise.height",
        "jump.profiles.*.precise.aim_point",
        "jump.profiles.*.precise.aim_offset_x_px",
        "jump.profiles.*.precise.aim_offset_y_px",
        "jump.profiles.*.precise.target_region_mode",
        "jump.profiles.*.precise.target_margin_percent",
        "jump.profiles.*.precise.visual_context_margin_percent",
        "jump.profiles.*.precise.zoom_scale",
        "jump.profiles.*.precise.preview_edge_behavior",
        "jump.profiles.*.precise.labels.font_scale",
        "jump.profiles.*.precise.labels.center_marker",
        "jump.profiles.*.precise.labels.separators",
        "jump.profiles.*.precise.labels.hide_threshold_px",
    ];

    KNOWN_PATHS
        .iter()
        .any(|pattern| path_matches(pattern, path))
}

fn path_matches(pattern: &str, path: &str) -> bool {
    let pattern_segments: Vec<&str> = pattern.split('.').collect();
    let path_segments: Vec<&str> = path.split('.').collect();

    pattern_segments.len() == path_segments.len()
        && pattern_segments
            .iter()
            .zip(path_segments)
            .all(|(pattern, path)| *pattern == "*" || *pattern == path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warning_for<'a>(report: &'a ConfigAuditReport, path: &str) -> &'a ConfigAuditWarning {
        report
            .warnings
            .iter()
            .find(|warning| warning.path == path)
            .unwrap_or_else(|| panic!("expected warning for {path}, got {:?}", report.warnings))
    }

    fn issue_for<'a>(report: &'a ConfigAuditReport, kind: KeybindIssueKind) -> &'a KeybindIssue {
        report
            .keybind_report
            .issues
            .iter()
            .find(|issue| issue.kind == kind)
            .unwrap_or_else(|| {
                panic!(
                    "expected keybind issue {:?}, got {:?}",
                    kind, report.keybind_report.issues
                )
            })
    }

    #[test]
    fn audit_warns_unknown_bookmark_markers_keys() {
        let report = audit_config_toml(
            r##"
            [bookmark_markers]
            enabled = true
            unknown = true

            [bookmark_markers.slot_styles."1"]
            fill_color = "#FFD400"
            unknown = "ignored"
            "##,
        );

        warning_for(&report, "bookmark_markers.unknown");
        warning_for(&report, "bookmark_markers.slot_styles.1.unknown");
    }

    #[test]
    fn audit_accepts_documented_bookmark_markers_keys() {
        let report = audit_config_toml(
            r##"
            [bookmark_markers]
            enabled = true
            show_with_help = true
            show_with_show_bookmarks = true
            show_with_bookmark_mode = true
            filter_current_virtual_desktop = true
            hide_offscreen = true
            position_source = "saved_coordinate"
            shape = "square"
            size_px = 32
            opacity = 0.82
            fill_color = "#FFD400"
            text_color = "#000000"
            border_color = "#000000"
            border_width_px = 2
            font_scale = 1.0
            offset_x = 0
            offset_y = 0
            center_on_bookmark = true

            [bookmark_markers.slot_styles."1"]
            fill_color = "#FFD400"
            text_color = "#000000"
            border_color = "#000000"
            opacity = 0.82
            "##,
        );

        assert!(
            report
                .warnings
                .iter()
                .all(|warning| !warning.path.starts_with("bookmark_markers")),
            "unexpected bookmark_markers warnings: {:?}",
            report.warnings
        );
    }

    #[test]
    fn audit_warns_when_help_will_hide_bindings() {
        let report = audit_config_toml(
            r#"
            key_bindings = [
                ["A", "move_up"],
                ["B", "move_down"],
                ["C", "move_left"],
            ]

            [tooltip_overlay]
            help_max_bindings = 2
            "#,
        );

        let issue = issue_for(&report, KeybindIssueKind::BindingNotShownInHelpDueToLimit);
        assert_eq!(issue.severity, ConfigAuditSeverity::Info);
        assert!(issue.message.contains("hiding 1"));
        assert!(issue
            .suggestion
            .contains("tooltip_overlay.help_max_bindings"));
    }

    #[test]
    fn audit_warns_generic_and_specific_chords_even_with_different_actions() {
        let report = audit_config_toml(
            r#"
            key_bindings = [
                ["Alt+E", "step_move_up"],
                ["RightAlt+E", "move_to_top_edge"],
            ]
            "#,
        );

        let issue = issue_for(
            &report,
            KeybindIssueKind::OverlappingGenericAndSideSpecificModifier,
        );
        assert!(issue.message.contains("Alt+E"));
        assert!(issue.message.contains("RightAlt+E"));
        assert_eq!(issue.winning_chord_display.as_deref(), Some("RightAlt+E"));
        assert_eq!(issue.winning_action_id.as_deref(), Some("move_to_top_edge"));
    }

    #[test]
    fn audit_warns_system_binding_reused_in_key_bindings() {
        let report = audit_config_toml(
            r#"
            key_bindings = [
                ["Ctrl+Escape", "disable"],
            ]
            "#,
        );

        let issue = issue_for(
            &report,
            KeybindIssueKind::SystemBindingAlsoUsedAsActionBinding,
        );
        assert!(issue.message.contains("system binding 'exit'"));
        assert_eq!(issue.winning_action_id.as_deref(), Some("exit"));
    }

    #[test]
    fn audit_reports_winning_binding_for_overlap() {
        let report = audit_config_toml(
            r#"
            key_bindings = [
                ["Alt+E", "step_move_up"],
                ["RightAlt+E", "move_to_top_edge"],
            ]
            "#,
        );

        let issue = issue_for(
            &report,
            KeybindIssueKind::OverlappingGenericAndSideSpecificModifier,
        );
        assert_eq!(issue.chord_display, "Alt+E");
        assert_eq!(issue.winning_chord_display.as_deref(), Some("RightAlt+E"));
        assert_eq!(
            issue.winning_action_name.as_deref(),
            Some("move_to_top_edge")
        );
    }

    #[test]
    fn audit_warns_altgr_ambiguous_bindings() {
        let report = audit_config_toml(
            r#"
            key_bindings = [
                ["RightAlt+E", "move_to_top_edge"],
            ]
            "#,
        );

        let issue = issue_for(&report, KeybindIssueKind::BindingLikelyAltGrAmbiguous);
        assert_eq!(issue.severity, ConfigAuditSeverity::Info);
        assert!(issue.message.contains("AltGr advisory"));
    }

    #[test]
    fn audit_warns_duplicate_exact_chords() {
        let report = audit_config_toml(
            r#"
            key_bindings = [
                ["I", "wheel_left"],
                ["I", "wheel_speed_down"],
            ]
            "#,
        );

        let issue = issue_for(&report, KeybindIssueKind::DuplicateExactChord);
        assert_eq!(issue.chord_display, "I");
        assert_eq!(issue.winning_action_id.as_deref(), Some("wheel_speed_down"));
        assert!(issue.message.contains("later entry wins"));
    }

    #[test]
    fn audit_warns_on_system_bindings_polling_rate_mis_scope() {
        let report = audit_config_toml(
            r#"
            [system_bindings]
            polling_rate = 16
            "#,
        );

        let warning = warning_for(&report, "system_bindings.polling_rate");
        assert_eq!(warning.severity, ConfigAuditSeverity::Warning);
        assert!(warning.suggestion.contains("top-level polling_rate"));
    }

    #[test]
    fn config_audit_warns_on_alt_generic_side_overlap() {
        let report = audit_config_toml(
            r#"
            key_bindings = [
              ["Alt+E", "move_up"],
              ["RightAlt+E", "move_up"],
            ]
            "#,
        );
        let warning = warning_for(&report, "key_bindings");
        assert_eq!(warning.severity, ConfigAuditSeverity::Warning);
        assert!(warning.message.contains("Alt+E"));
        assert!(warning.message.contains("RightAlt+E"));
        assert!(warning.message.contains("more specific chord wins"));
    }

    #[test]
    fn config_audit_warns_on_shift_generic_side_overlap() {
        let report = audit_config_toml(
            r#"
            key_bindings = [
              ["Shift+E", "move_up"],
              ["LeftShift+E", "move_up"],
            ]
            "#,
        );
        let warning = warning_for(&report, "key_bindings");
        assert_eq!(warning.severity, ConfigAuditSeverity::Warning);
        assert!(warning.message.contains("Shift+E"));
        assert!(warning.message.contains("LeftShift+E"));
    }

    #[test]
    fn config_audit_emits_altgr_advisory_for_relevant_chords() {
        let report = audit_config_toml(
            r#"
            key_bindings = [
              ["RightAlt+E", "move_up"],
              ["Ctrl+Alt+E", "move_up"],
            ]
            "#,
        );
        let advisory = report
            .warnings
            .iter()
            .find(|warning| {
                warning.severity == ConfigAuditSeverity::Info
                    && warning.message.contains("AltGr advisory")
            })
            .expect("expected AltGr advisory warning");
        assert_eq!(advisory.path, "key_bindings");
    }

    #[test]
    fn config_audit_has_no_false_positive_for_non_overlapping_chords() {
        let report = audit_config_toml(
            r#"
            key_bindings = [
              ["Alt+E", "move_up"],
              ["RightAlt+Q", "move_up"],
            ]
            "#,
        );
        assert!(!report
            .warnings
            .iter()
            .any(|warning| warning.message.contains("overlaps")));
    }

    #[test]
    fn audit_warns_on_grid_mode_acceleration_mis_scope() {
        let report = audit_config_toml(
            r#"
            [grid_mode]
            acceleration = 4
            "#,
        );

        let warning = warning_for(&report, "grid_mode.acceleration");
        assert_eq!(warning.severity, ConfigAuditSeverity::Warning);
        assert!(warning.suggestion.contains("top-level acceleration"));
    }

    #[test]
    fn audit_warns_on_preview_margin_percent_alias() {
        let report = audit_config_toml(
            r#"
            [jump.coarse]
            preview_margin_percent = 5
            "#,
        );

        let warning = warning_for(&report, "jump.coarse.preview_margin_percent");
        assert_eq!(warning.severity, ConfigAuditSeverity::Deprecated);
        assert!(warning
            .suggestion
            .contains("jump.coarse.visual_context_margin_percent"));
    }

    #[test]
    fn audit_warns_on_move_cursor_after_each_stage_alias() {
        let report = audit_config_toml(
            r#"
            [jump]
            move_cursor_after_each_stage = true
            "#,
        );

        let warning = warning_for(&report, "jump.move_cursor_after_each_stage");
        assert_eq!(warning.severity, ConfigAuditSeverity::Deprecated);
        assert!(warning.suggestion.contains("jump.cursor_between_stages"));
    }

    #[test]
    fn audit_accepts_step_move_paths() {
        let report = audit_config_toml(
            r#"
            [step_move]
            enabled = true
            small_step_px = 10
            normal_step_px = 80
            large_step_px = 200
            clamp_mode = "virtual_screen"
            show_tooltip = false
            "#,
        );
        assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    }

    #[test]
    fn audit_warns_on_grid_size_when_jump_coarse_present() {
        let report = audit_config_toml(
            r#"
            grid_size = { width = 10, height = 10 }

            [jump.coarse]
            width = 8
            height = 8
            "#,
        );

        let warning = warning_for(&report, "grid_size.width");
        assert_eq!(warning.severity, ConfigAuditSeverity::Deprecated);
        assert!(warning
            .message
            .contains("ignored when [jump.coarse] is present"));
        assert!(warning.suggestion.contains("jump.coarse.width"));
    }

    #[test]
    fn audit_clean_for_default_config_no_unexpected_unknowns() {
        let report = audit_config_toml(include_str!("../config.toml"));

        assert!(
            !report
                .warnings
                .iter()
                .any(|warning| warning.message.contains("Unknown config path")),
            "unexpected unknown-path warnings: {:?}",
            report.warnings
        );
    }

    #[test]
    fn audit_warns_on_duplicate_key_chords() {
        let report = audit_config_toml(
            r#"
            key_bindings = [
                ["I", "wheel_left"],
                ["I", "wheel_speed_down"],
                ["O", "wheel_right"],
            ]
            "#,
        );

        let duplicate_warning = report
            .warnings
            .iter()
            .find(|warning| {
                warning.path == "key_bindings" && warning.message.contains("Duplicate key chord")
            })
            .expect("expected duplicate key chord warning");
        assert!(duplicate_warning.message.contains("later entry wins"));
        assert!(duplicate_warning.message.contains("key_bindings[0]"));
        assert!(duplicate_warning.message.contains("key_bindings[1]"));
    }

    #[test]
    fn audit_does_not_warn_for_unique_key_chords() {
        let report = audit_config_toml(
            r#"
            key_bindings = [
                ["I", "wheel_left"],
                ["B", "wheel_speed_down"],
                ["O", "wheel_right"],
            ]
            "#,
        );

        assert!(
            !report
                .warnings
                .iter()
                .any(|warning| warning.path == "key_bindings"
                    && warning.message.contains("Duplicate key chord")),
            "unexpected duplicate warning: {:?}",
            report.warnings
        );
    }

    #[test]
    fn audit_duplicate_key_chord_warning_text_is_stable_for_regression_fixture() {
        let report = audit_config_toml(include_str!(
            "../tests/fixtures/config_duplicate_wheel_chord.toml"
        ));

        let warning = report
            .warnings
            .iter()
            .find(|warning| warning.path == "key_bindings")
            .expect("expected duplicate warning for regression fixture");
        assert_eq!(
            warning.message,
            "Duplicate key chord 'I' appears in multiple bindings (key_bindings[0], key_bindings[1]); later entry wins."
        );
    }

    #[test]
    fn default_config_has_no_mis_scoped_legacy_fields() {
        let report = audit_config_toml(include_str!("../config.toml"));
        let mis_scoped_paths = [
            "system_bindings.polling_rate",
            "system_bindings.grid_size",
            "system_bindings.grid_size.width",
            "system_bindings.grid_size.height",
            "grid_mode.starting_speed",
            "grid_mode.acceleration",
            "grid_mode.acceleration_rate",
            "grid_mode.top_speed",
            "jump.move_cursor_after_each_stage",
        ];

        assert!(
            report
                .warnings
                .iter()
                .all(|warning| !mis_scoped_paths.contains(&warning.path.as_str())
                    && !warning.path.ends_with(".preview_margin_percent")),
            "mis-scoped legacy warnings in default config: {:?}",
            report.warnings
        );
    }

    #[test]
    fn docs_reference_paths_exist_in_config_schema_smoke() {
        let docs = include_str!("../docs/config.md");
        let rows = config_reference_rows(docs);
        let required_paths = [
            "polling_rate",
            "grid_size.width",
            "mouse_speed.default_speed",
            "jump.coarse.width",
            "jump.fine.width",
            "jump.precise.enabled",
            "jump.coarse.zoom_scale",
            "jump.hints.selection_keys",
        ];

        assert!(!rows.is_empty(), "expected config reference rows");

        for path in required_paths {
            assert!(
                rows.iter()
                    .any(|(documented_path, _)| documented_path == path),
                "docs/config.md does not document required path {path}"
            );
        }

        for (path, status) in rows {
            assert!(
                matches!(
                    status.as_str(),
                    "Active" | "Legacy" | "Deprecated alias" | "Preview-related"
                ),
                "unexpected documented status {status:?} for {path}"
            );
            assert!(
                is_known_active_path(&path) || is_documented_legacy_or_group_path(&path),
                "docs/config.md documents {path}, but config_audit does not recognize it"
            );
        }
    }

    fn config_reference_rows(docs: &str) -> Vec<(String, String)> {
        let mut in_config_paths = false;
        let mut rows = Vec::new();

        for line in docs.lines() {
            match line {
                "## Config Paths" => {
                    in_config_paths = true;
                    continue;
                }
                "## Critical Examples" => break,
                _ => {}
            }

            if !in_config_paths || !line.starts_with("| `") {
                continue;
            }

            let columns: Vec<&str> = line.split('|').map(str::trim).collect();
            if columns.len() < 7 {
                continue;
            }

            rows.push((
                columns[1].trim_matches('`').to_string(),
                columns[5].to_string(),
            ));
        }

        rows
    }

    fn is_documented_legacy_or_group_path(path: &str) -> bool {
        matches!(
            path,
            "jump.move_cursor_after_each_stage"
                | "jump.<stage>.preview_margin_percent"
                | "system_bindings.polling_rate"
                | "system_bindings.grid_size"
                | "grid_mode.starting_speed"
                | "grid_mode.acceleration"
                | "grid_mode.acceleration_rate"
                | "grid_mode.top_speed"
                | "jump.profiles.*.visuals.*"
                | "jump.profiles.*.coarse.*"
                | "jump.profiles.*.fine.*"
                | "jump.profiles.*.precise.*"
                | "status_overlay.fields.*"
                | "tooltip_overlay.events.*"
        )
    }

    #[test]
    fn audit_treats_slow_mouse_paths_as_known_and_active() {
        let report = audit_config_toml(
            r#"
            [slow_mouse]
            strategy = "fixed"
            fixed_speed = 1
            multiplier = 0.75
            subtract_speed = 1
            min_speed = 1
            max_speed = 5
            acceleration = 1
            acceleration_rate = 1
            "#,
        );

        for path in [
            "slow_mouse.strategy",
            "slow_mouse.fixed_speed",
            "slow_mouse.multiplier",
            "slow_mouse.subtract_speed",
            "slow_mouse.min_speed",
            "slow_mouse.max_speed",
            "slow_mouse.acceleration",
            "slow_mouse.acceleration_rate",
        ] {
            assert!(
                !report.warnings.iter().any(|warning| warning.path == path),
                "expected {path} to be known/active, warnings: {:?}",
                report.warnings
            );
        }
    }

    #[test]
    fn audit_keeps_slow_mouse_typo_path_unknown() {
        let report = audit_config_toml(
            r#"
            [slow_mouse]
            multiplyer = 0.75
            "#,
        );

        let warning = warning_for(&report, "slow_mouse.multiplyer");
        assert_eq!(warning.severity, ConfigAuditSeverity::Warning);
        assert!(warning.message.contains("Unknown config path"));
    }

    #[test]
    fn audit_accepts_ui_hints_query_keys() {
        let report = audit_config_toml(
            r#"
            [ui_hints]
            query_strategy = "children_then_descendants"
            min_targets_before_fallback = 12
            query_timeout_ms = 900
            "#,
        );
        assert!(
            !report
                .warnings
                .iter()
                .any(|w| w.path.starts_with("ui_hints.query_")
                    || w.path == "ui_hints.min_targets_before_fallback"),
            "unexpected warnings: {:?}",
            report.warnings
        );
    }
    #[test]
    fn audit_parses_invalid_toml_gracefully() {
        let report = audit_config_toml("[jump\nmode = \"precision\"");

        let warning = warning_for(&report, "<toml>");
        assert_eq!(warning.severity, ConfigAuditSeverity::Warning);
        assert!(warning.message.contains("could not be parsed"));
    }
    #[test]
    fn audit_flags_legacy_position_history_paths_as_unknown() {
        let report = audit_config_toml(
            r#"
            [position_history]
            enabled = true
            max_positions = 16
            selection_keys = "ASDF"
            label_length = 2
            show_numbers = false
        "#,
        );

        let legacy_paths = [
            "position_history.enabled",
            "position_history.max_positions",
            "position_history.selection_keys",
            "position_history.label_length",
            "position_history.show_numbers",
        ];

        for path in legacy_paths {
            let warning = warning_for(&report, path);
            assert_eq!(warning.severity, ConfigAuditSeverity::Warning);
            assert!(warning.message.contains("Unknown config path"));
        }
    }
}
