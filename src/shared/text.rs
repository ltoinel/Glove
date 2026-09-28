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

/// Relevance tier of an exact match: the name equals the query.
pub const RANK_EXACT: usize = 0;
/// Relevance tier of a prefix match: the name starts with the query.
pub const RANK_PREFIX: usize = 1;
/// Relevance tier of a word-prefix match: one word of the name starts with the query.
pub const RANK_WORD_PREFIX: usize = 2;
/// Relevance tier of a substring match: the query appears anywhere in the name.
pub const RANK_SUBSTRING: usize = 3;

/// A relevance tier and the test deciding whether a name falls into it.
type RankTier = (usize, fn(&str, &str) -> bool);

/// Tiers from best to worst, so the first that matches is the rank.
const RANK_TIERS: [RankTier; 4] = [
    (RANK_EXACT, |name, query| name == query),
    (RANK_PREFIX, |name, query| name.starts_with(query)),
    (RANK_WORD_PREFIX, |name, query| {
        name.split_whitespace().any(|word| word.starts_with(query))
    }),
    (RANK_SUBSTRING, |name, query| name.contains(query)),
];

/// Rank how well an already-normalized `name` matches a normalized `query`.
///
/// Returns the best tier that applies (lower is better), or `None` when none
/// does or when the best tier is worse than `worst_useful`. The cut-off lets
/// callers whose result buffer is already full of good hits skip the costlier
/// word-split and substring checks.
pub fn match_rank(name: &str, query: &str, worst_useful: usize) -> Option<usize> {
    RANK_TIERS
        .iter()
        .take_while(|(rank, _)| *rank <= worst_useful)
        .find(|(_, matches)| matches(name, query))
        .map(|(rank, _)| *rank)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_rank_orders_tiers() {
        assert_eq!(match_rank("gare", "gare", usize::MAX), Some(RANK_EXACT));
        assert_eq!(
            match_rank("gare de lyon", "gare", usize::MAX),
            Some(RANK_PREFIX)
        );
        assert_eq!(
            match_rank("paris gare", "gare", usize::MAX),
            Some(RANK_WORD_PREFIX)
        );
        assert_eq!(
            match_rank("lagare", "gare", usize::MAX),
            Some(RANK_SUBSTRING)
        );
        assert_eq!(match_rank("station", "gare", usize::MAX), None);
    }

    #[test]
    fn match_rank_honours_cut_off() {
        assert_eq!(match_rank("lagare", "gare", RANK_WORD_PREFIX), None);
        assert_eq!(match_rank("gare", "gare", RANK_EXACT), Some(RANK_EXACT));
    }

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
