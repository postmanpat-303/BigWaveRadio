//! Mood labels in the UI language. (The old left-rail mood chips were
//! replaced by the Browse tiles and the Now Playing dial.)

/// A mood's label in the UI language. The built-in moods are translated
/// while their label is still the default; labels edited in moods.json
/// (or moods the user added) are shown exactly as written.
pub fn mood_label(mood: &bwr_core::Mood) -> String {
    crate::rtl::visual_string(mood_label_raw(mood))
}

/// `mood_label` in logical order, for passing into `t!(…, mood = …)`.
pub fn mood_label_raw(mood: &bwr_core::Mood) -> String {
    let translated = match (mood.id.as_str(), mood.label.as_str()) {
        ("dance", "Dance") => t_raw!("mood.dance"),
        ("trance", "Trance") => t_raw!("mood.trance"),
        ("chill", "Chill") => t_raw!("mood.chill"),
        ("easy", "Easy") => t_raw!("mood.easy"),
        ("80s", "80s") => t_raw!("mood.80s"),
        ("90s", "90s") => t_raw!("mood.90s"),
        _ => return mood.label.clone(),
    };
    translated.to_string()
}
