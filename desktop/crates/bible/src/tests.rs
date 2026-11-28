use crate::reference::END;
use crate::*;

fn span(book: u8, from: (u16, u16), to: (u16, u16)) -> Span {
    Span { book, from, to }
}

#[test]
fn references_read_the_usual_ways_of_writing_them() {
    let john = 42;
    assert_eq!(parse_reference("John 3:16").unwrap(), [span(john, (3, 16), (3, 16))]);
    assert_eq!(parse_reference("jn 3:16-18, 20").unwrap(), [span(john, (3, 16), (3, 18)), span(john, (3, 20), (3, 20))]);
    assert_eq!(parse_reference("Juan 3.16–4:2").unwrap(), [span(john, (3, 16), (4, 2))]);
    assert_eq!(parse_reference("Ps 23").unwrap(), [span(18, (23, 1), (23, END))]);
    assert_eq!(parse_reference("Mga Awit 23-24").unwrap(), [span(18, (23, 1), (24, END))]);
    assert_eq!(parse_reference("1 Cor 13:4-7; Rom 8:28").unwrap(), [span(45, (13, 4), (13, 7)), span(44, (8, 28), (8, 28))]);
    assert_eq!(parse_reference("1cor 13:4").unwrap(), [span(45, (13, 4), (13, 4))]);
    assert_eq!(parse_reference("I Corinthians 13:4").unwrap(), [span(45, (13, 4), (13, 4))]);
    assert_eq!(parse_reference("1 Mga Taga-Corinto 13:4").unwrap(), [span(45, (13, 4), (13, 4))]);
    assert_eq!(parse_reference("Jude 3").unwrap(), [span(64, (1, 3), (1, 3))]);
    assert_eq!(parse_reference("Phil 4:13").unwrap(), [span(49, (4, 13), (4, 13))]);
    assert_eq!(parse_reference("Pahayag 21:4").unwrap()[0].book, 65);
    assert_eq!(parse_reference("Kawikaan 3:5-6").unwrap()[0].book, 19);
}

#[test]
fn bad_references_say_what_is_wrong() {
    assert!(parse_reference("").is_err());
    assert!(parse_reference("Hezekiah 1:1").unwrap_err().to_string().contains("No book"));
    assert!(parse_reference("Ju 1:1").unwrap_err().to_string().contains("could be"));
    assert!(parse_reference("John 30:1").unwrap_err().to_string().contains("21 chapters"));
    assert!(parse_reference("John 3:18-16").is_err());
}

#[test]
fn references_are_written_back_in_the_bible_language() {
    let spans = parse_reference("jn 3:16-18, 20; 4:1").unwrap();
    assert_eq!(format_reference(&spans, "en"), "John 3:16–18, 20; 4:1");
    assert_eq!(format_reference(&spans, "tl"), "Juan 3:16–18, 20; 4:1");
    assert_eq!(format_reference(&parse_reference("ps 23").unwrap(), "en"), "Psalms 23");
    assert_eq!(format_reference(&parse_reference("jude 3-4").unwrap(), "en"), "Jude 3–4");
}

#[test]
fn bundled_bibles_give_one_slide_per_verse() {
    let bibles = Bibles::default();
    let ids: Vec<String> = bibles.list().into_iter().map(|b| b.id).collect();
    assert_eq!(ids, ["kjv", "adb1905"]);
    let p = passage(&bibles, "John 3:16-17", "kjv", Some("adb1905"));
    assert_eq!(p.warning, None);
    assert_eq!(p.slides.len(), 2);
    assert!(p.slides[0].text.starts_with("For God so loved the world"), "{}", p.slides[0].text);
    assert!(p.slides[0].parallel.contains("Dios"), "{}", p.slides[0].parallel);
    assert_eq!(p.slides[0].caption, "John 3:16 · KJV · ADB1905");
    assert_eq!(p.title, "John 3:16–17 · KJV · ADB1905");
    let psalm = passage(&bibles, "Awit 23", "adb1905", None);
    assert_eq!(psalm.slides.len(), 6);
    assert!(psalm.slides[0].caption.starts_with("Mga Awit 23:1"));
}

#[test]
fn a_missing_translation_falls_back_and_says_so() {
    let p = passage(&Bibles::default(), "John 11:35", "niv", None);
    assert_eq!(p.slides.len(), 1);
    assert!(p.warning.unwrap().contains("NIV is not on this computer"));
}

#[test]
fn whole_bibles_count_their_verses() {
    let bibles = Bibles::default();
    assert_eq!(bibles.get("kjv").unwrap().len(), 31102);
    assert_eq!(bibles.get("adb1905").unwrap().len(), 31102);
    let found = bibles.get("kjv").unwrap().search("jesus wept", 5).into_iter().map(|v| (v.book, v.chapter, v.verse)).collect::<Vec<_>>();
    assert_eq!(found[0], (42, 11, 35));
    assert_eq!(found.len(), 3);
}

#[test]
fn zefania_and_usfm_files_import() {
    let dir = std::env::temp_dir().join(format!("bible-import-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let verses: String = (1..=120).map(|v| format!("<VERS vnumber=\"{v}\">Verse <gr str=\"1\">{v}</gr> text<NOTE>skip</NOTE></VERS>")).collect();
    let zefania = format!("<?xml version=\"1.0\"?><XMLBIBLE biblename=\"Test Bible\"><INFORMATION><identifier>TST</identifier><language>ENG</language></INFORMATION><BIBLEBOOK bnumber=\"43\"><CHAPTER cnumber=\"1\">{verses}</CHAPTER></BIBLEBOOK></XMLBIBLE>");
    std::fs::write(dir.join("test.xml"), zefania).unwrap();
    let bible = import_file(&dir.join("test.xml")).unwrap();
    assert_eq!((bible.info.name.as_str(), bible.info.abbreviation.as_str(), bible.info.language.as_str()), ("Test Bible", "TST", "en"));
    assert_eq!(bible.chapter(42, 1)[0].text, "Verse 1 text");

    let usfm: String = (1..=120).map(|v| format!("\\v {v} In \\w the|strong=\"G1\"\\w* beginning\\f + \\ft note\\f*.\n")).collect();
    std::fs::write(dir.join("43JHN.usfm"), format!("\\id JHN\n\\h John\n\\c 1\n\\p\n{usfm}")).unwrap();
    let bible = import_file(&dir.join("43JHN.usfm")).unwrap();
    assert_eq!(bible.chapter(42, 1)[0].text, "In the beginning.");

    let store = Bibles::new(Some(dir.join("bibles")));
    let info = store.import(&dir.join("test.xml")).unwrap();
    assert_eq!(info.id, "tst");
    assert!(store.list().iter().any(|b| b.id == "tst" && !b.bundled));
    assert_eq!(passage(&store, "John 1:2", "tst", None).slides[0].text, "Verse 2 text");
    store.remove("tst").unwrap();
    assert!(!store.list().iter().any(|b| b.id == "tst"));
    std::fs::remove_dir_all(&dir).unwrap();
}
