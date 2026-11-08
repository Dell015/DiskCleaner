//! Comparing names safely: exact equality after normalization, never plain substrings.

fn is_noise(token: &str) -> bool {
    matches!(
        token,
        "inc"
            | "ltd"
            | "llc"
            | "corp"
            | "corporation"
            | "co"
            | "the"
            | "version"
            | "x64"
            | "x86"
            | "bit"
            | "user"
            | "machine"
            | "gmbh"
            | "limited"
            | "ver"
    ) || token.chars().all(|c| c.is_ascii_digit())
}

/// Lower-case words of `text`, without punctuation, company suffixes, versions and bitness.
pub fn tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '+' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .filter(|t| !is_noise(t))
        .map(str::to_owned)
        .collect()
}

/// The normalized form: tokens joined by a space.
pub fn normalize(text: &str) -> String {
    tokens(text).join(" ")
}

/// Names shorter than this (normalized) are too ambiguous to match on.
pub const MIN_LEN: usize = 4;

/// Do two names refer to the same thing? True for equal normalized names, or when the shorter
/// has at least two words and is the start of the longer ("visual studio" / "visual studio code").
pub fn same_name(a: &str, b: &str) -> bool {
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.is_empty() || tb.is_empty() {
        return false;
    }
    let (short, long) = if ta.len() <= tb.len() {
        (&ta, &tb)
    } else {
        (&tb, &ta)
    };
    let joined = short.join(" ");
    if joined.chars().count() < MIN_LEN {
        return false;
    }
    if short == long {
        return true;
    }
    short.len() >= 2 && long.starts_with(short)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_bitness_and_suffixes_are_dropped() {
        assert_eq!(normalize("Git version 2.47.0"), "git");
        assert_eq!(normalize("Python 3.12.1 (64-bit)"), "python");
        assert_eq!(normalize("Contoso, Inc."), "contoso");
        assert_eq!(
            normalize("Microsoft Visual C++ 2015 Redistributable (x64)"),
            "microsoft visual c++ redistributable"
        );
    }

    #[test]
    fn equal_names_match_regardless_of_case_and_punctuation() {
        assert!(same_name("Mozilla Firefox", "mozilla-firefox"));
        assert!(same_name("Notepad++", "NOTEPAD++ (64-bit)"));
        assert!(
            !same_name("Git", "Git"),
            "three letters is too short to trust by name"
        );
    }

    #[test]
    fn false_friends_do_not_match() {
        assert!(!same_name("Go", "Go"), "too short to trust");
        assert!(!same_name("Java", "JavaScript Tools"));
        assert!(!same_name("Adobe", "Adobe Reader"));
        assert!(!same_name("Steam", "Steamworks Shared"));
        assert!(!same_name("", "x"));
    }

    #[test]
    fn multi_word_prefix_is_allowed_but_single_words_are_not() {
        assert!(same_name("Visual Studio", "Visual Studio Code"));
        assert!(!same_name("Visual", "Visual Studio Code"));
    }
}
