//! Post-processing of transcripts before they are pasted.

/// Hesitation sounds that never carry meaning in dictated text. Words people
/// use on purpose ("ah", "hmm", "like") are deliberately not included.
const FILLERS: [&str; 6] = ["um", "umm", "uh", "uhh", "uhm", "erm"];

/// Removes filler words, keeping sentence punctuation and capitalization
/// intact: "Um, so I think uh we should." -> "So I think we should."
pub fn remove_fillers(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    // True when the next kept word starts a sentence.
    let mut sentence_start = true;
    let mut capitalize_next = false;

    for token in text.split_whitespace() {
        let core = token.trim_matches(|c: char| !c.is_alphanumeric());
        let trailing = &token[token.trim_end_matches(|c: char| !c.is_alphanumeric()).len()..];

        if FILLERS.iter().any(|f| core.eq_ignore_ascii_case(f)) {
            // Keep sentence-ending punctuation the filler carried ("...said uh.").
            if let Some(end) = trailing.chars().find(|c| matches!(c, '.' | '?' | '!')) {
                if let Some(prev) = out.last_mut() {
                    let trimmed = prev.trim_end_matches([',', ';', ':']).len();
                    prev.truncate(trimmed);
                    if !prev.ends_with(['.', '?', '!']) {
                        prev.push(end);
                    }
                }
                sentence_start = true;
            }
            // A capitalized filler that started a sentence hands the capital on.
            if sentence_start {
                capitalize_next = true;
            }
            continue;
        }

        let mut word = token.to_string();
        if capitalize_next {
            word = capitalize(&word);
            capitalize_next = false;
        }
        sentence_start = word.ends_with(['.', '?', '!']);
        out.push(word);
    }
    out.join(" ")
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::remove_fillers as clean;

    #[test]
    fn removes_fillers_mid_sentence() {
        assert_eq!(clean("I plan to make this uh voice app"), "I plan to make this voice app");
        assert_eq!(clean("we should, um, ship it."), "we should, ship it.");
    }

    #[test]
    fn capitalizes_after_leading_filler() {
        assert_eq!(clean("Um, so I think we should."), "So I think we should.");
        assert_eq!(clean("Done. Uh, next one."), "Done. Next one.");
    }

    #[test]
    fn keeps_sentence_end_carried_by_filler() {
        assert_eq!(clean("That is what I said, uh."), "That is what I said.");
        assert_eq!(clean("Is it ready uh?"), "Is it ready?");
    }

    #[test]
    fn only_fillers_becomes_empty() {
        assert_eq!(clean("Um."), "");
        assert_eq!(clean("Uh, um..."), "");
    }

    #[test]
    fn leaves_real_words_alone() {
        // Substrings and words people use on purpose stay.
        for s in ["Umbrella and hummus.", "Hmm, I like it.", "Ah, I see.", "The UH-60 helicopter."] {
            assert_eq!(clean(s), s);
        }
    }
}
