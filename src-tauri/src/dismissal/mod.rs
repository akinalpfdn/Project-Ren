//! Dismissal phrase detection.
//!
//! When the user explicitly tells Ren to stop ("go to sleep", "bye Ren",
//! "tamam yeter", ...), the conversation loop must end immediately and Ren
//! returns to the Sleeping state. This module exposes a single pure function
//! so the caller can pass either the raw STT transcript or the LLM response.

use crate::config::defaults::DISMISSAL_PHRASES;

/// Returns `true` when the input text contains a dismissal phrase.
///
/// Matching rules:
/// - Case-insensitive (`to_lowercase` on both sides).
/// - Substring match — the user can wrap the phrase in pleasantries
///   ("ok ren, go to sleep please") and still be heard.
/// - Empty / whitespace-only input is never a dismissal.
pub fn is_dismissal(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_lowercase();
    DISMISSAL_PHRASES
        .iter()
        .any(|phrase| lower.contains(&phrase.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_english_phrase_anywhere() {
        assert!(is_dismissal("ok ren go to sleep please"));
        assert!(is_dismissal("Goodbye Ren"));
    }

    #[test]
    fn matches_turkish_phrase_anywhere() {
        assert!(is_dismissal("tamam yeter teşekkürler"));
        assert!(is_dismissal("İyi geceler"));
    }

    #[test]
    fn ignores_unrelated_speech() {
        assert!(!is_dismissal("what's the weather today"));
        assert!(!is_dismissal("play some music"));
    }

    #[test]
    fn ignores_empty_input() {
        assert!(!is_dismissal(""));
        assert!(!is_dismissal("   \n\t"));
    }
}
