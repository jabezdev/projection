use std::path::PathBuf;

use palto_obs::text::{text, Kind, Options};
use protocol::FeedState;

fn fixture(name: &str) -> FeedState {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../desktop/fixtures/feed").join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn every_shared_fixture_reads() {
    for name in ["song", "song_tag", "text", "black", "nothing", "newer"] {
        fixture(name);
    }
}

#[test]
fn lyrics_follow_the_options() {
    let song = fixture("song");
    let options = Options::default();
    assert_eq!(text(Kind::Lyrics, Some(&song), &options), "Amazing grace, how sweet the sound\nThat saved a wretch like me");
    assert_eq!(text(Kind::Lyrics, Some(&song), &Options { max_lines: 1, uppercase: true, ..options.clone() }), "AMAZING GRACE, HOW SWEET THE SOUND");
    assert_eq!(text(Kind::Lyrics, Some(&song), &Options { join_lines: true, ..options.clone() }), "Amazing grace, how sweet the sound That saved a wretch like me");
    assert_eq!(text(Kind::Lyrics, Some(&fixture("text")), &options), "Welcome home");
    assert_eq!(text(Kind::Lyrics, Some(&fixture("text")), &Options { songs_only: true, ..options.clone() }), "");
    assert_eq!(text(Kind::Lyrics, None, &options), "");
}

#[test]
fn clear_black_and_logo_blank_unless_asked_not_to() {
    let black = fixture("black");
    assert_eq!(text(Kind::Lyrics, Some(&black), &Options::default()), "");
    assert_eq!(text(Kind::Lyrics, Some(&black), &Options { blank_when_hidden: false, ..Options::default() }), "I once was lost, but now am found");
    assert_eq!(text(Kind::Lyrics, Some(&fixture("newer")), &Options::default()), "", "an unknown display state counts as hidden");
}

#[test]
fn section_and_song_info_show_only_for_songs() {
    let options = Options { license: "12345".into(), ..Options::default() };
    assert_eq!(text(Kind::Section, Some(&fixture("song")), &options), "Verse 1");
    assert_eq!(text(Kind::Section, Some(&fixture("song_tag")), &options), "Ending");
    assert_eq!(text(Kind::Section, Some(&fixture("text")), &options), "");
    assert_eq!(text(Kind::SongInfo, Some(&fixture("text")), &options), "");
    assert_eq!(text(Kind::SongInfo, Some(&fixture("song")), &options), "Amazing Grace\nJohn Newton\nPublic Domain · CCLI Song #22025 · CCLI License #12345");
}

#[test]
fn empty_parts_of_the_template_drop_out() {
    let song = fixture("song");
    let options = Options { template: "{title} · {music} · {year}\n{publisher}\n© {copyright} {unknown}".into(), ..Options::default() };
    assert_eq!(text(Kind::SongInfo, Some(&song), &options), "Amazing Grace · 1779\n© Public Domain {unknown}");
    let words = Options { template: "Words: {words}".into(), ..Options::default() };
    assert_eq!(text(Kind::SongInfo, Some(&song), &words), "Words: John Newton");
}
