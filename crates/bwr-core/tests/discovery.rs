use bwr_core::discovery::*;
use bwr_core::Station;

fn st(name: &str, tags: &str) -> Station {
    Station {
        uuid: format!("u-{name}-{tags}"),
        name: name.into(),
        stream_url: format!("http://example.com/{}", name.replace(' ', "")),
        tags: tags.into(),
        last_check_ok: 1,
        ..Default::default()
    }
}

#[test]
fn talk_filter_hides_talk_by_tags() {
    let cfg = MoodConfig::default();
    for (name, tags) in [
        ("BBC World Service", "news,talk"),
        ("Talk 1", "talk radio,politics"),
        ("Sport Live", "sports,football"),
        ("Grace FM", "christian,religion"),
        ("Laugh Factory", "comedy"),
        ("Pod Pod", "podcast"),
        ("Story Hour", "spoken word,audiobooks"),
    ] {
        assert!(cfg.is_talk(&st(name, tags)), "{name} [{tags}] should be talk");
    }
}

#[test]
fn talk_filter_uses_name_heuristics() {
    let cfg = MoodConfig::default();
    assert!(cfg.is_talk(&st("Radio News 24", "")));
    assert!(cfg.is_talk(&st("talkSPORT 2", "uk")));
    assert!(cfg.is_talk(&st("Radio Maria", "")));
    assert!(cfg.is_talk(&st("LBC London", "")));
    assert!(cfg.is_talk(&st("France Info", "")));
    assert!(cfg.is_talk(&st("Radio Nachrichten", "")));
}

#[test]
fn talk_filter_keeps_music() {
    let cfg = MoodConfig::default();
    for (name, tags) in [
        ("Sunshine Live", "dance,electronic,house"),
        ("Hits Radio", "pop,hits,news"),          // music outweighs one talk tag
        ("Trance Energy", "trance,uplifting"),
        ("Newstalgia 80s", "80s,pop"),           // "newstalgia" is not the word "news"
        ("Smooth Chill", "chillout,lounge"),
        ("Unknown FM", ""),
        ("Club News Mix", "dance,club,house"),   // name word, but clearly music
    ] {
        assert!(!cfg.is_talk(&st(name, tags)), "{name} [{tags}] should be music");
    }
}

#[test]
fn moods_cover_expected_tags() {
    let cfg = MoodConfig::default();
    let ids: Vec<_> = cfg.moods.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["dance", "trance", "chill", "easy", "80s", "90s"]);
    let trance = cfg.mood("trance").unwrap();
    for t in ["trance", "progressive", "uplifting", "psytrance"] {
        assert!(trance.tags.iter().any(|x| x == t), "trance missing {t}");
    }
    assert!(!trance.accepts(&st("Prog Metal", "progressive metal,rock")));
    assert!(trance.accepts(&st("Uplift", "progressive,uplifting trance")));
    // round-trips through JSON (the editable moods.json)
    let json = serde_json::to_string(&cfg).unwrap();
    assert_eq!(serde_json::from_str::<MoodConfig>(&json).unwrap(), cfg);
    // partial file falls back to defaults for missing keys
    let partial: MoodConfig = serde_json::from_str(r#"{"moods":[{"id":"x","label":"X","tags":["x"]}]}"#).unwrap();
    assert_eq!(partial.moods.len(), 1);
    assert!(!partial.talk_tags.is_empty());
}

#[test]
fn ranking_puts_working_streams_first_then_popularity_and_bitrate() {
    let mut broken = st("Broken Mega", "dance");
    broken.votes = 100_000;
    broken.last_check_ok = 0;
    let mut popular = st("Popular", "dance");
    popular.votes = 5000;
    popular.click_count = 900;
    popular.bitrate = 128;
    let mut small = st("Small", "dance");
    small.votes = 3;
    small.bitrate = 320;
    let mut small_lo = st("Small Lo", "dance");
    small_lo.votes = 3;
    small_lo.bitrate = 64;
    let out = rank_and_dedupe(vec![small_lo.clone(), broken.clone(), small.clone(), popular.clone()]);
    let names: Vec<_> = out.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Popular", "Small", "Small Lo", "Broken Mega"]);
}

#[test]
fn dedupe_drops_near_identical_stations() {
    assert_eq!(dedupe_key("Sunshine Live - 128 kbps MP3"), dedupe_key("sunshine live (320k)"));
    assert_eq!(dedupe_key("Kiss FM HQ"), dedupe_key("KISS FM"));
    assert_ne!(dedupe_key("Radio 1"), dedupe_key("Radio 2"));
    let mut a = st("Sunshine Live 128", "dance");
    a.votes = 10;
    let mut b = st("SUNSHINE LIVE (AAC)", "dance");
    b.votes = 500;
    let mut c = st("Different", "dance");
    c.stream_url = b.stream_url.clone().replace("http://", "https://") + "/?x=1";
    let out = rank_and_dedupe(vec![a, b, c]);
    assert_eq!(out.len(), 1, "{:?}", out.iter().map(|s| &s.name).collect::<Vec<_>>());
    assert_eq!(out[0].votes, 500);
}

#[test]
fn more_like_this_is_weighted_by_ratings() {
    let cfg = MoodConfig::default();
    let mut loved = st("Loved", "trance,uplifting");
    loved.uuid = "loved".into();
    let hated = st("Hated", "news,talk");
    let profile = taste_profile([(5u8, &loved), (1u8, &hated)]);
    assert!(profile["trance"] > 0.0 && profile["news"] < 0.0);

    let seed = st("Seed", "news,trance,electronic,music");
    let tags = seed_tags(&seed, &cfg, &profile, 3);
    assert_eq!(tags[0], "trance", "liked tag first, talk/generic skipped: {tags:?}");
    assert!(!tags.contains(&"news".to_string()) && !tags.contains(&"music".to_string()));

    let a = st("A", "trance,uplifting");
    let b = st("B", "trance,electronic");
    let unrelated = st("C", "jazz");
    let sa = similarity(&seed, &a, &profile, 0);
    let sc = similarity(&seed, &unrelated, &profile, 0);
    assert!(sa > sc);
    // the user's own rating of a candidate moves it up or down
    assert!(similarity(&seed, &b, &profile, 5) > similarity(&seed, &b, &profile, 0));
    assert!(similarity(&seed, &b, &profile, 1) < similarity(&seed, &b, &profile, 0));
}

#[test]
fn surprise_skips_talk_broken_and_disliked() {
    let cfg = MoodConfig::default();
    let mut broken = st("Broken", "dance");
    broken.last_check_ok = 0;
    let talk = st("Talky", "news");
    let disliked = st("Meh", "dance");
    let good = st("Good", "trance");
    let ranked = vec![broken, talk, disliked.clone(), good.clone()];
    for seed in 1..50 {
        let mut rng = Rng::new(seed);
        let pick = pick_surprise(&ranked, &cfg, |u| if u == disliked.uuid { 1 } else { 0 }, &mut rng).unwrap();
        assert_eq!(pick.name, "Good");
    }
    assert!(pick_surprise(&[], &cfg, |_| 0, &mut Rng::new(3)).is_none());
}

#[test]
fn recently_played_is_newest_first_deduped_capped_and_persisted() {
    let dir = std::env::temp_dir().join(format!("bwr-disc-{}", std::process::id()));
    let path = dir.join("discovery.json");
    let mut d = DiscoveryState::load_from(path.clone());
    assert!(!d.show_talk);
    for i in 0..(MAX_RECENT + 5) {
        d.note_played(&st(&format!("S{i}"), ""));
    }
    d.note_played(&st("S3", ""));
    assert_eq!(d.recent.len(), MAX_RECENT);
    assert_eq!(d.recent[0].name, "S3");
    assert_eq!(d.recent.iter().filter(|s| s.name == "S3").count(), 1);
    let again = DiscoveryState::load_from(path);
    assert_eq!(again.recent[0].name, "S3");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn old_station_records_without_lastcheckok_still_load() {
    let json = r#"{"stationuuid":"x","name":"Old","url_resolved":"u","homepage":"","favicon":"","codec":"MP3","bitrate":128,"votes":1,"clickcount":2,"country":"","language":"","tags":""}"#;
    let s: Station = serde_json::from_str(json).unwrap();
    assert_eq!(s.last_check_ok, 1);
}

#[test]
fn settings_json_stays_compatible() {
    // A settings.json as the C# app / earlier versions wrote it must load,
    // and saving must not add discovery keys to it.
    let old = r#"{"Theme":"dark","Accent":"ocean","MinBitrate":128,"RatingFilter":4,"Codec":"","Volume":0.5,"MinimizeToTray":true}"#;
    let s: bwr_core::Settings = serde_json::from_str(old).unwrap();
    assert_eq!(s.min_bitrate, 128);
    let out = serde_json::to_string(&s).unwrap();
    assert!(!out.contains("ShowTalk") && !out.contains("Recent\""));
}
