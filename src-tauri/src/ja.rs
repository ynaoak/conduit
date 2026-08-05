//! Japanese text helpers for romaji-insensitive fuzzy matching.
//!
//! Lets "memo" hit "メモ帳" and "denngenn" hit "でんげん…": app names are
//! normalized katakana→hiragana, and ASCII queries are converted
//! romaji→hiragana so both meet in kana space. Kanji readings are out of
//! scope (no dictionary).

/// Katakana -> hiragana, ASCII lowercased. Other chars pass through.
pub fn normalize_kana(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'ァ'..='ヶ' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            _ => c.to_ascii_lowercase(),
        })
        .collect()
}

/// True if the string contains any kana (after katakana normalization
/// this means a kana search key is worth building)
pub fn contains_kana(s: &str) -> bool {
    s.chars()
        .any(|c| matches!(c, 'ぁ'..='ゖ' | 'ァ'..='ヶ' | 'ー'))
}

/// Convert an ASCII romaji string to hiragana (longest-match, IME-style).
/// Unconvertible chars pass through unchanged.
pub fn romaji_to_hiragana(input: &str) -> String {
    let chars: Vec<char> = input.to_ascii_lowercase().chars().collect();
    let mut out = String::new();
    let mut i = 0;

    while i < chars.len() {
        // Sokuon: doubled consonant (except "nn" which is ん)
        if i + 1 < chars.len()
            && chars[i] == chars[i + 1]
            && chars[i] != 'n'
            && is_consonant(chars[i])
        {
            out.push('っ');
            i += 1;
            continue;
        }

        // Longest match first: 3 -> 2 -> 1 chars
        let mut matched = false;
        for len in (1..=3usize).rev() {
            if i + len <= chars.len() {
                let chunk: String = chars[i..i + len].iter().collect();
                if let Some(kana) = lookup(&chunk) {
                    out.push_str(kana);
                    i += len;
                    matched = true;
                    break;
                }
            }
        }
        if matched {
            continue;
        }

        // "n" before a consonant or at the end is ん
        if chars[i] == 'n' {
            out.push('ん');
            i += 1;
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Kana variants of an ASCII query, for matching against kana-normalized
/// names. Empty when the query is not ASCII or nothing converts.
pub fn query_variants(query: &str) -> Vec<String> {
    let mut variants = Vec::new();
    if !query.is_ascii() {
        return variants;
    }
    let kana = romaji_to_hiragana(query);
    if kana == query {
        return variants;
    }
    // Trailing unconverted letters are an incomplete syllable ("mem" -> めm);
    // also offer the stripped form so incremental typing keeps matching
    let stripped: String = kana
        .trim_end_matches(|c: char| c.is_ascii_alphabetic())
        .to_string();
    if !stripped.is_empty() && stripped != kana {
        variants.push(stripped);
    }
    variants.insert(0, kana);
    variants
}

fn is_consonant(c: char) -> bool {
    c.is_ascii_alphabetic() && !matches!(c, 'a' | 'i' | 'u' | 'e' | 'o')
}

fn lookup(chunk: &str) -> Option<&'static str> {
    Some(match chunk {
        // triples
        "kya" => "きゃ", "kyu" => "きゅ", "kyo" => "きょ",
        "sha" => "しゃ", "shu" => "しゅ", "sho" => "しょ", "shi" => "し",
        "sya" => "しゃ", "syu" => "しゅ", "syo" => "しょ",
        "cha" => "ちゃ", "chu" => "ちゅ", "cho" => "ちょ", "chi" => "ち", "che" => "ちぇ",
        "tya" => "ちゃ", "tyu" => "ちゅ", "tyo" => "ちょ",
        "tsu" => "つ", "tsa" => "つぁ", "tso" => "つぉ",
        "nya" => "にゃ", "nyu" => "にゅ", "nyo" => "にょ",
        "hya" => "ひゃ", "hyu" => "ひゅ", "hyo" => "ひょ",
        "mya" => "みゃ", "myu" => "みゅ", "myo" => "みょ",
        "rya" => "りゃ", "ryu" => "りゅ", "ryo" => "りょ",
        "gya" => "ぎゃ", "gyu" => "ぎゅ", "gyo" => "ぎょ",
        "ja" => "じゃ", "ju" => "じゅ", "jo" => "じょ", "ji" => "じ", "je" => "じぇ",
        "jya" => "じゃ", "jyu" => "じゅ", "jyo" => "じょ",
        "zya" => "じゃ", "zyu" => "じゅ", "zyo" => "じょ",
        "bya" => "びゃ", "byu" => "びゅ", "byo" => "びょ",
        "pya" => "ぴゃ", "pyu" => "ぴゅ", "pyo" => "ぴょ",
        "dya" => "ぢゃ", "dyu" => "ぢゅ", "dyo" => "ぢょ",
        "fa" => "ふぁ", "fi" => "ふぃ", "fe" => "ふぇ", "fo" => "ふぉ",
        "va" => "ゔぁ", "vi" => "ゔぃ", "ve" => "ゔぇ", "vo" => "ゔぉ",
        "wi" => "うぃ", "we" => "うぇ",
        "dhi" => "でぃ", "dhu" => "でゅ", "thi" => "てぃ", "thu" => "てゅ",
        "nn" => "ん",
        // basics
        "a" => "あ", "i" => "い", "u" => "う", "e" => "え", "o" => "お",
        "ka" => "か", "ki" => "き", "ku" => "く", "ke" => "け", "ko" => "こ",
        "sa" => "さ", "si" => "し", "su" => "す", "se" => "せ", "so" => "そ",
        "ta" => "た", "ti" => "ち", "tu" => "つ", "te" => "て", "to" => "と",
        "na" => "な", "ni" => "に", "nu" => "ぬ", "ne" => "ね", "no" => "の",
        "ha" => "は", "hi" => "ひ", "hu" => "ふ", "fu" => "ふ", "he" => "へ", "ho" => "ほ",
        "ma" => "ま", "mi" => "み", "mu" => "む", "me" => "め", "mo" => "も",
        "ya" => "や", "yu" => "ゆ", "yo" => "よ",
        "ra" => "ら", "ri" => "り", "ru" => "る", "re" => "れ", "ro" => "ろ",
        "wa" => "わ", "wo" => "を",
        "ga" => "が", "gi" => "ぎ", "gu" => "ぐ", "ge" => "げ", "go" => "ご",
        "za" => "ざ", "zi" => "じ", "zu" => "ず", "ze" => "ぜ", "zo" => "ぞ",
        "da" => "だ", "di" => "ぢ", "du" => "づ", "de" => "で", "do" => "ど",
        "ba" => "ば", "bi" => "び", "bu" => "ぶ", "be" => "べ", "bo" => "ぼ",
        "pa" => "ぱ", "pi" => "ぴ", "pu" => "ぷ", "pe" => "ぺ", "po" => "ぽ",
        "vu" => "ゔ",
        "la" => "ぁ", "li" => "ぃ", "lu" => "ぅ", "le" => "ぇ", "lo" => "ぉ",
        "-" => "ー",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_basic_romaji() {
        assert_eq!(romaji_to_hiragana("memo"), "めも");
        assert_eq!(romaji_to_hiragana("peinto"), "ぺいんと");
        assert_eq!(romaji_to_hiragana("dentaku"), "でんたく");
    }

    #[test]
    fn handles_nn_and_n_before_consonant() {
        assert_eq!(romaji_to_hiragana("denngenn"), "でんげん");
        assert_eq!(romaji_to_hiragana("dengen"), "でんげん");
        assert_eq!(romaji_to_hiragana("kondate"), "こんだて");
    }

    #[test]
    fn handles_sokuon_and_digraphs() {
        assert_eq!(romaji_to_hiragana("kitte"), "きって");
        assert_eq!(romaji_to_hiragana("shashin"), "しゃしん");
        assert_eq!(romaji_to_hiragana("jouhou"), "じょうほう");
    }

    #[test]
    fn passes_through_unconvertible() {
        assert_eq!(romaji_to_hiragana("chrome"), "chろめ");
    }

    #[test]
    fn normalizes_katakana() {
        assert_eq!(normalize_kana("メモ帳"), "めも帳");
        assert_eq!(normalize_kana("ペイント"), "ぺいんと");
        assert_eq!(normalize_kana("Edge"), "edge");
    }

    #[test]
    fn query_variants_strip_incomplete_syllable() {
        assert_eq!(query_variants("memo"), vec!["めも".to_string()]);
        assert_eq!(
            query_variants("mem"),
            vec!["めm".to_string(), "め".to_string()]
        );
        assert!(query_variants("めも").is_empty());
    }
}
