use std::collections::HashSet;

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
        "edge_jump.offset_px",
        "edge_jump.use_work_area",
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
        "status_overlay.fields.jump",
        "status_overlay.fields.mouse_speed",
        "status_overlay.fields.wheel_speed",
        "status_overlay.fields.flash",
        "status_overlay.fields.final_adjust",
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
        "tooltip_overlay.events.mouse",
        "tooltip_overlay.events.wheel",
        "tooltip_overlay.events.profile",
        "tooltip_overlay.events.drag",
        "tooltip_overlay.events.reload",
        "tooltip_overlay.events.panic",
        "tooltip_overlay.events.ui_hints_query_start",
        "tooltip_overlay.events.ui_hints_query_fail",
        "tooltip_overlay.events.ui_hints_query_empty",
        "tooltip_overlay.events.ui_hints_query_capped_count",
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
        "ui_hints.target_point",
        "ui_hints.after_select",
        "ui_hints.overlay.font_scale",
        "ui_hints.overlay.offset_x",
        "ui_hints.overlay.offset_y",
        "ui_hints.overlay.dim_non_matching",
        "ui_hints.overlay.show_background",
        "ui_hints.overlay.show_border",
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
    fn audit_parses_invalid_toml_gracefully() {
        let report = audit_config_toml("[jump\nmode = \"precision\"");

        let warning = warning_for(&report, "<toml>");
        assert_eq!(warning.severity, ConfigAuditSeverity::Warning);
        assert!(warning.message.contains("could not be parsed"));
    }
}
