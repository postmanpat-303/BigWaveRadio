//! Mood labels in the UI language. (The old left-rail mood chips were
//! replaced by the Browse tiles and the Now Playing dial.)

/// A mood's label in the UI language. The built-in moods are translated
/// while their label is still the default; labels edited in moods.json
/// (or moods the user added) are shown exactly as written.
pub fn mood_label(mood: &bwr_core::Mood) -> String {
    let translated = match (mood.id.as_str(), mood.label.as_str()) {
        ("dance", "Dance") => t!("mood.dance"),
        ("trance", "Trance") => t!("mood.trance"),
        ("chill", "Chill") => t!("mood.chill"),
        ("easy", "Easy") => t!("mood.easy"),
        ("80s", "80s") => t!("mood.80s"),
        ("90s", "90s") => t!("mood.90s"),
        _ => return mood.label.clone(),
    };
    translated.to_string()
}
