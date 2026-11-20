//! The Proton options that the game settings offer as switches. Each one is a variable that Proton reads when a game
//! starts. The settings keep only the ids, and the variables are written here, so the interface never types them.
//! Only Linux has Proton, so the list is empty on Windows and the interface hides the switches there.

/// Each option: its id (what the settings keep, and what the interface translates), and the variable it sets.
const OPTIONS: &[(&str, &str, &str)] = &[
    ("wined3d", "PROTON_USE_WINED3D", "1"),
    ("proton_log", "PROTON_LOG", "1"),
    ("no_esync", "PROTON_NO_ESYNC", "1"),
    ("no_fsync", "PROTON_NO_FSYNC", "1"),
    ("fsr", "WINE_FULLSCREEN_FSR", "1"),
    ("fsr4", "PROTON_FSR4_UPGRADE", "1"),
    ("nvapi", "PROTON_ENABLE_NVAPI", "1"),
    ("dlss", "PROTON_DLSS_UPGRADE", "1"),
    ("dxr", "VKD3D_CONFIG", "dxr"),
    ("antilag", "ENABLE_LAYER_MESA_ANTI_LAG", "1"),
    ("low_latency", "LOW_LATENCY_LAYER", "1"),
    ("low_latency_reflex", "LOW_LATENCY_LAYER_REFLEX", "1"),
    ("dxvk_lowlatency", "PROTON_DXVK_LOWLATENCY", "1"),
    ("fsr_indicator", "PROTON_FSR4_INDICATOR", "1"),
    ("dlss_indicator", "PROTON_DLSS_INDICATOR", "1"),
    ("mangohud", "MANGOHUD", "1"),
    ("shader_cache", "PROTON_LOCAL_SHADER_CACHE", "1"),
];

/// Pairs that must not be on at once: both enable a Vulkan latency layer. The first id in the pair wins in catalog
/// order, so a hand-edited settings file cannot start the game with both.
const EXCLUSIVE: &[(&str, &str)] = &[("antilag", "low_latency")];

/// The ids of the options this computer offers: all of them on Linux, none on Windows.
pub fn available() -> Vec<String> {
    if cfg!(windows) {
        return Vec::new();
    }

    OPTIONS.iter().map(|(id, _, _)| (*id).to_string()).collect()
}

/// The chosen options, in catalog order and once each. An id this build does not know is dropped.
pub fn known(chosen: &[String]) -> Vec<String> {
    chosen_options(chosen)
        .map(|(id, _, _)| (*id).to_string())
        .collect()
}

/// The list after one switch: `option` goes on or off. Switching an option on clears the options it conflicts with, so
/// the newest choice wins; switching it off only removes it. The answer is in catalog order, once each.
pub fn switched(chosen: &[String], option: &str, on: bool) -> Vec<String> {
    let mut next: Vec<String> = chosen
        .iter()
        .filter(|id| id.as_str() != option)
        .cloned()
        .collect();
    if on {
        next.retain(|id| !conflicts(id, option));
        next.push(option.to_string());
    }

    known(&next)
}

/// Whether two options may not be on at once.
fn conflicts(first: &str, second: &str) -> bool {
    EXCLUSIVE
        .iter()
        .any(|(a, b)| (first == *a && second == *b) || (first == *b && second == *a))
}

/// The variables of the chosen options, in catalog order.
pub fn variables(chosen: &[String]) -> Vec<(String, String)> {
    chosen_options(chosen)
        .map(|(_, name, value)| ((*name).to_string(), (*value).to_string()))
        .collect()
}

fn chosen_options(
    chosen: &[String],
) -> impl Iterator<Item = &'static (&'static str, &'static str, &'static str)> {
    let mut kept: Vec<&'static (&'static str, &'static str, &'static str)> = Vec::new();

    for option in OPTIONS {
        if !chosen.iter().any(|picked| picked == option.0) {
            continue;
        }

        let conflicts = EXCLUSIVE.iter().any(|(a, b)| {
            (option.0 == *a && kept.iter().any(|kept| kept.0 == *b))
                || (option.0 == *b && kept.iter().any(|kept| kept.0 == *a))
        });

        if !conflicts {
            kept.push(option);
        }
    }

    kept.into_iter()
}

/// A frame rate cap, written for both Direct3D translation layers. `None` leaves both variables unset so a launch
/// setting can still choose its own limit.
pub fn fps_limit_variables(limit: Option<u32>) -> Vec<(String, String)> {
    match limit {
        Some(frames) if frames > 0 => vec![
            ("DXVK_FRAME_RATE".to_string(), frames.to_string()),
            ("VKD3D_FRAME_RATE".to_string(), frames.to_string()),
        ],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_string()).collect()
    }

    #[cfg(not(windows))]
    #[test]
    fn linux_offers_every_catalogued_option() {
        // The catalogue can grow without breaking this test; what it guards is that Linux filters none of it out.
        let catalogue: Vec<String> = OPTIONS.iter().map(|(id, _, _)| (*id).to_string()).collect();
        assert_eq!(available(), catalogue);
    }

    #[cfg(windows)]
    #[test]
    fn windows_offers_no_proton_option() {
        assert!(available().is_empty());
    }

    #[test]
    fn the_settings_keep_known_ids_once_in_catalog_order() {
        assert_eq!(
            known(&ids(&["no_fsync", "unknown", "wined3d", "wined3d"])),
            ids(&["wined3d", "no_fsync"])
        );
        assert!(known(&ids(&["unknown"])).is_empty());
    }

    #[test]
    fn each_chosen_option_sets_its_variable() {
        assert_eq!(
            variables(&ids(&["proton_log", "wined3d"])),
            vec![
                ("PROTON_USE_WINED3D".to_string(), "1".to_string()),
                ("PROTON_LOG".to_string(), "1".to_string()),
            ]
        );
        assert!(variables(&[]).is_empty());
    }

    #[test]
    fn a_switch_can_set_a_value_other_than_one() {
        assert_eq!(
            variables(&ids(&["dxr"])),
            vec![("VKD3D_CONFIG".to_string(), "dxr".to_string())]
        );
    }

    #[test]
    fn conflicting_latency_layers_are_dropped_from_saved_options_and_variables() {
        assert_eq!(known(&ids(&["low_latency", "antilag"])), ids(&["antilag"]));
        assert_eq!(
            variables(&ids(&["low_latency", "antilag"])),
            vec![("ENABLE_LAYER_MESA_ANTI_LAG".to_string(), "1".to_string())]
        );
    }

    #[test]
    fn a_switch_keeps_the_other_chosen_options_in_catalog_order() {
        let chosen = ids(&["shader_cache", "wined3d"]);
        assert_eq!(
            switched(&chosen, "proton_log", true),
            ids(&["wined3d", "proton_log", "shader_cache"])
        );
        assert_eq!(switched(&chosen, "wined3d", false), ids(&["shader_cache"]));
        assert_eq!(
            switched(&chosen, "wined3d", true),
            ids(&["wined3d", "shader_cache"])
        );
    }

    #[test]
    fn switching_a_latency_layer_on_clears_the_conflicting_one() {
        assert_eq!(
            switched(&ids(&["low_latency"]), "antilag", true),
            ids(&["antilag"])
        );
        assert_eq!(
            switched(&ids(&["antilag"]), "low_latency", true),
            ids(&["low_latency"])
        );
        assert_eq!(
            switched(&ids(&["low_latency"]), "antilag", false),
            ids(&["low_latency"])
        );
    }

    #[test]
    fn a_switch_drops_an_unknown_id_and_an_unknown_option_is_not_added() {
        assert_eq!(
            switched(&ids(&["retired"]), "proton_log", true),
            ids(&["proton_log"])
        );
        assert!(switched(&[], "unknown", true).is_empty());
    }

    #[test]
    fn a_frame_rate_cap_is_written_for_both_translation_layers() {
        assert_eq!(
            fps_limit_variables(Some(60)),
            vec![
                ("DXVK_FRAME_RATE".to_string(), "60".to_string()),
                ("VKD3D_FRAME_RATE".to_string(), "60".to_string()),
            ]
        );
        assert!(fps_limit_variables(None).is_empty());
        assert!(fps_limit_variables(Some(0)).is_empty());
    }
}
