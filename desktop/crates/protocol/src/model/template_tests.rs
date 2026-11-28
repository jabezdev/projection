use super::*;

fn song_item(id: &str) -> ServiceItem {
    ServiceItem { id: id.into(), title: None, notes: None, background: None, content: ItemContent::Song(SongItem { song_id: "s1".into(), version_id: Some("v1".into()), key: None, arrangement: None }) }
}

fn header(id: &str, title: &str) -> ServiceItem {
    ServiceItem { id: id.into(), title: Some(title.into()), notes: None, background: None, content: ItemContent::Header }
}

fn text(id: &str) -> ServiceItem {
    ServiceItem { id: id.into(), title: Some("Offering".into()), notes: None, background: None, content: ItemContent::Text { steps: vec!["Give".into()] } }
}

#[test]
fn the_sunday_template_has_the_usual_parts_with_two_optional() {
    let sunday = ServiceTemplate::sunday();
    let titles: Vec<_> = sunday.parts.iter().map(|p| p.title.as_str()).collect();
    assert_eq!(titles, ["Welcoming", "Countdown", "Praise & Worship", "Tithes and Offering", "Special Number", "Message", "Special Program", "Closing"]);
    assert_eq!(sunday.default_parts().len(), 6);
    assert_eq!(sunday.id, DEFAULT_TEMPLATE_ID);
}

#[test]
fn a_service_from_a_template_has_a_section_per_chosen_part_and_fresh_ids() {
    let mut template = ServiceTemplate::sunday();
    template.parts[3].items.push(text("fixed"));
    let chosen = vec![template.parts[0].id.clone(), template.parts[3].id.clone()];
    let service = template.instantiate(&chosen, None, 42);
    let titles: Vec<_> = service.items.iter().map(|i| i.title.clone().unwrap_or_default()).collect();
    assert_eq!(titles, ["Welcoming", "Tithes and Offering", "Offering"]);
    assert_eq!(service.title, "Sunday Service");
    assert_eq!(service.scheduled_at_ms, 42);
    assert!(service.items.iter().all(|i| i.id != "fixed"));
    assert_eq!(template.instantiate(&chosen, Some("  Easter "), 0).title, "Easter");
}

#[test]
fn a_template_from_a_service_keeps_sections_and_fixed_items_and_unpins_songs() {
    let service = Service { id: "x".into(), scheduled_at_ms: 0, title: "Youth".into(), leader: None, notes: None, ended_at_ms: None, items: vec![text("t0"), header("h1", "Worship"), song_item("s"), header("h2", "Offering"), text("t1")] };
    let without = ServiceTemplate::from_service("tpl".into(), "Youth".into(), &service, false);
    let shape: Vec<_> = without.parts.iter().map(|p| (p.title.as_str(), p.items.len())).collect();
    assert_eq!(shape, [("Start", 1), ("Worship", 0), ("Offering", 1)]);
    let with = ServiceTemplate::from_service("tpl".into(), "Youth".into(), &service, true);
    let ItemContent::Song(song) = &with.parts[1].items[0].content else { panic!("song") };
    assert_eq!(song.version_id, None);
}
