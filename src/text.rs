//! Shared text normalization utilities.

/// Normalize a string for fuzzy search: lowercase, strip French diacritics,
/// replace hyphens and apostrophes with spaces.
#[allow(clippy::collapsible_str_replace)]
pub fn normalize(s: &str) -> String {
    s.to_lowercase()
        .replace('é', "e")
        .replace('è', "e")
        .replace('ê', "e")
        .replace('ë', "e")
        .replace('à', "a")
        .replace('â', "a")
        .replace('ä', "a")
        .replace('ô', "o")
        .replace('ö', "o")
        .replace('ù', "u")
        .replace('û', "u")
        .replace('ü', "u")
        .replace('î', "i")
        .replace('ï', "i")
        .replace('ç', "c")
        .replace('œ', "oe")
        .replace('æ', "ae")
        .replace(['-', '\''], " ")
        .replace('\u{2019}', " ")
        .replace('\u{2018}', " ")
}
