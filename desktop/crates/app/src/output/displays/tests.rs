use super::*;

fn display(name: &str, x: i32) -> OutputRoute { OutputRoute { name: Some(name.into()), x, y: 0, width: 1920, height: 1080, fullscreen: true, size: None, aspect: None, device: None } }
#[test]
fn follows_a_windows_display_renumbered_after_replugging() {
    let mut projector = display(r"\\.\DISPLAY2", 1920);
    projector.device = Some("projector-path".into());
    let mut renumbered = display(r"\\.\DISPLAY3", 1920);
    renumbered.device = projector.device.clone();
    assert_eq!(matching_route(&projector, &[display(r"\\.\DISPLAY2", 1920), renumbered]), Some(1));
}
#[test]
fn a_disconnected_device_does_not_use_another_monitors_recycled_number() {
    let mut projector = display(r"\\.\DISPLAY2", 1920);
    projector.device = Some("projector-path".into());
    let mut replacement = display(r"\\.\DISPLAY2", 1920);
    replacement.device = Some("operator-monitor-path".into());
    assert_eq!(matching_route(&projector, &[replacement]), None);
    assert_eq!(matching_route(&projector, &[display(r"\\.\DISPLAY2", 1920)]), Some(0));
}
#[test]
fn restores_named_display_after_enumeration_or_position_changes() {
    let audience = display("HDMI-A-1", 1920);
    assert_eq!(matching_route(&audience, &[audience.clone(), display("eDP-1", 0)]), Some(0));
    assert_eq!(matching_route(&audience, &[display("eDP-1", 0), display("HDMI-A-1", 0)]), Some(1));
}
#[test]
fn missing_or_ambiguous_display_does_not_redirect_output() {
    let audience = display("HDMI-A-1", 1920);
    assert_eq!(matching_route(&audience, &[display("eDP-1", 0)]), None);
    assert_eq!(matching_route(&audience, &[display("HDMI-A-1", 0), display("HDMI-A-1", 3840)]), None);
}
#[test]
fn saved_routes_preserve_separate_fullscreen_choices() {
    let audience = display("HDMI-A-1", 1920);
    let mut confidence = display("DP-1", -1920);
    confidence.fullscreen = false;
    let routes = std::collections::BTreeMap::from([("projection", audience), ("confidence", confidence)]);
    let loaded: std::collections::BTreeMap<String, OutputRoute> = serde_json::from_slice(&serde_json::to_vec(&routes).unwrap()).unwrap();
    assert!(loaded["projection"].fullscreen);
    assert!(!loaded["confidence"].fullscreen);
    assert_eq!(loaded["confidence"].name.as_deref(), Some("DP-1"));
}
#[test]
fn routes_saved_by_older_versions_still_load() {
    let route: OutputRoute = serde_json::from_str(r#"{"name":"HDMI-A-1","x":0,"y":0,"width":1920,"height":1080,"fullscreen":false,"enabled":false}"#).unwrap();
    assert!(!route.fullscreen);
    assert_eq!((route.size, route.aspect), (None, None));
}
#[test]
fn default_displays_avoid_the_operator_screen() {
    assert_eq!((default_monitor("projection", Some(0), 3), default_monitor("confidence", Some(0), 3)), (1, 2));
    assert_eq!((default_monitor("projection", Some(1), 2), default_monitor("confidence", Some(1), 2)), (0, 1));
    assert_eq!((default_monitor("projection", Some(0), 1), default_monitor("confidence", None, 1)), (0, 0));
}
