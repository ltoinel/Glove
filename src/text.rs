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

#[cfg(test)]
mod tests {
    use super::normalize;

    // Basic lowercase
    #[test]
    fn test_lowercase() {
        assert_eq!(normalize("HELLO"), "hello");
        assert_eq!(normalize("Hello World"), "hello world");
        assert_eq!(normalize("ABC"), "abc");
    }

    // French diacritics — each one individually
    #[test]
    fn test_diacritic_e_acute() {
        assert_eq!(normalize("é"), "e");
    }

    #[test]
    fn test_diacritic_e_grave() {
        assert_eq!(normalize("è"), "e");
    }

    #[test]
    fn test_diacritic_e_circumflex() {
        assert_eq!(normalize("ê"), "e");
    }

    #[test]
    fn test_diacritic_e_umlaut() {
        assert_eq!(normalize("ë"), "e");
    }

    #[test]
    fn test_diacritic_a_grave() {
        assert_eq!(normalize("à"), "a");
    }

    #[test]
    fn test_diacritic_a_circumflex() {
        assert_eq!(normalize("â"), "a");
    }

    #[test]
    fn test_diacritic_a_umlaut() {
        assert_eq!(normalize("ä"), "a");
    }

    #[test]
    fn test_diacritic_o_circumflex() {
        assert_eq!(normalize("ô"), "o");
    }

    #[test]
    fn test_diacritic_o_umlaut() {
        assert_eq!(normalize("ö"), "o");
    }

    #[test]
    fn test_diacritic_u_grave() {
        assert_eq!(normalize("ù"), "u");
    }

    #[test]
    fn test_diacritic_u_circumflex() {
        assert_eq!(normalize("û"), "u");
    }

    #[test]
    fn test_diacritic_u_umlaut() {
        assert_eq!(normalize("ü"), "u");
    }

    #[test]
    fn test_diacritic_i_circumflex() {
        assert_eq!(normalize("î"), "i");
    }

    #[test]
    fn test_diacritic_i_umlaut() {
        assert_eq!(normalize("ï"), "i");
    }

    #[test]
    fn test_diacritic_c_cedilla() {
        assert_eq!(normalize("ç"), "c");
    }

    #[test]
    fn test_diacritic_oe_ligature() {
        assert_eq!(normalize("œ"), "oe");
    }

    #[test]
    fn test_diacritic_ae_ligature() {
        assert_eq!(normalize("æ"), "ae");
    }

    // Uppercase diacritics (normalize via to_lowercase first)
    #[test]
    fn test_uppercase_diacritics() {
        assert_eq!(normalize("É"), "e");
        assert_eq!(normalize("È"), "e");
        assert_eq!(normalize("Ê"), "e");
        assert_eq!(normalize("À"), "a");
        assert_eq!(normalize("Â"), "a");
        assert_eq!(normalize("Ô"), "o");
        assert_eq!(normalize("Î"), "i");
        assert_eq!(normalize("Ç"), "c");
        assert_eq!(normalize("Œ"), "oe");
        assert_eq!(normalize("Æ"), "ae");
    }

    // Hyphens replaced by spaces
    #[test]
    fn test_hyphen_replaced_by_space() {
        assert_eq!(normalize("saint-lazare"), "saint lazare");
        assert_eq!(normalize("arc-en-ciel"), "arc en ciel");
    }

    // ASCII apostrophe replaced by space
    #[test]
    fn test_ascii_apostrophe_replaced_by_space() {
        assert_eq!(normalize("l'église"), "l eglise");
        assert_eq!(normalize("aujourd'hui"), "aujourd hui");
    }

    // Unicode RIGHT SINGLE QUOTATION MARK U+2019 replaced by space
    #[test]
    fn test_unicode_right_single_quote_replaced_by_space() {
        assert_eq!(normalize("l\u{2019}église"), "l eglise");
        assert_eq!(normalize("c\u{2019}est"), "c est");
    }

    // Unicode LEFT SINGLE QUOTATION MARK U+2018 replaced by space
    #[test]
    fn test_unicode_left_single_quote_replaced_by_space() {
        assert_eq!(normalize("l\u{2018}église"), "l eglise");
        assert_eq!(normalize("c\u{2018}est"), "c est");
    }

    // Mixed case + diacritics
    #[test]
    fn test_mixed_case_and_diacritics() {
        assert_eq!(normalize("Île-de-France"), "ile de france");
        assert_eq!(normalize("CHÂTELET"), "chatelet");
        assert_eq!(normalize("Gare de l'Est"), "gare de l est");
        assert_eq!(normalize("Saint-Étienne"), "saint etienne");
    }

    // Empty string
    #[test]
    fn test_empty_string() {
        assert_eq!(normalize(""), "");
    }

    // String with no diacritics passes through (only lowercased)
    #[test]
    fn test_no_diacritics_passthrough() {
        assert_eq!(normalize("paris"), "paris");
        assert_eq!(normalize("lyon"), "lyon");
        assert_eq!(normalize("hello world"), "hello world");
    }

    // Multiple diacritics in one word
    #[test]
    fn test_multiple_diacritics_in_one_word() {
        assert_eq!(normalize("préféré"), "prefere");
        assert_eq!(normalize("hétérogène"), "heterogene");
        assert_eq!(normalize("bœuf"), "boeuf");
        assert_eq!(normalize("naïveté"), "naivete");
        assert_eq!(normalize("cœur"), "coeur");
    }
}
