//! Target language registry. Codes end up in prompts, filenames and
//! `settings.toml`, so only codes listed in [`LANGUAGES`] are accepted.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Language {
    pub code: &'static str,
    /// Turkish UI name.
    pub name: &'static str,
    pub native: &'static str,
}

pub const DEFAULT_TARGET: &str = "en";

/// All of ISO-639-1 plus `pt-BR`, `zh-Hans`, `zh-Hant`. Keep it complete: a
/// missing code gets dropped on load and silently resets the user's settings.
pub const LANGUAGES: &[Language] = &[
    Language { code: "aa", name: "Afarca", native: "Afaraf" },
    Language { code: "ab", name: "Abhazca", native: "аҧсуа бызшәа" },
    Language { code: "ae", name: "Avestaca", native: "avesta" },
    Language { code: "af", name: "Afrikaans", native: "Afrikaans" },
    Language { code: "ak", name: "Akan", native: "Akan" },
    Language { code: "am", name: "Amharca", native: "አማርኛ" },
    Language { code: "an", name: "Aragonca", native: "aragonés" },
    Language { code: "ar", name: "Arapça", native: "العربية" },
    Language { code: "as", name: "Assamca", native: "অসমীয়া" },
    Language { code: "av", name: "Avarca", native: "авар мацӀ" },
    Language { code: "ay", name: "Aymara", native: "aymar aru" },
    Language { code: "az", name: "Azerbaycanca", native: "azərbaycan dili" },
    Language { code: "ba", name: "Başkurtça", native: "башҡорт теле" },
    Language { code: "be", name: "Belarusça", native: "беларуская мова" },
    Language { code: "bg", name: "Bulgarca", native: "български" },
    Language { code: "bh", name: "Biharca", native: "भोजपुरी" },
    Language { code: "bi", name: "Bislama", native: "Bislama" },
    Language { code: "bm", name: "Bambara", native: "bamanankan" },
    Language { code: "bn", name: "Bengalce", native: "বাংলা" },
    Language { code: "bo", name: "Tibetçe", native: "བོད་ཡིག" },
    Language { code: "br", name: "Bretonca", native: "brezhoneg" },
    Language { code: "bs", name: "Bosnaca", native: "bosanski" },
    Language { code: "ca", name: "Katalanca", native: "català" },
    Language { code: "ce", name: "Çeçençe", native: "нохчийн мотт" },
    Language { code: "ch", name: "Chamorroca", native: "Chamoru" },
    Language { code: "co", name: "Korsikaca", native: "corsu" },
    Language { code: "cr", name: "Keece", native: "ᓀᐦᐃᔭᐍᐏᐣ" },
    Language { code: "cs", name: "Çekçe", native: "čeština" },
    Language { code: "cu", name: "Kilise Slavcası", native: "ѩзыкъ словѣньскъ" },
    Language { code: "cv", name: "Çuvaşça", native: "чӑваш чӗлхи" },
    Language { code: "cy", name: "Gallerce", native: "Cymraeg" },
    Language { code: "da", name: "Danca", native: "dansk" },
    Language { code: "de", name: "Almanca", native: "Deutsch" },
    Language { code: "dv", name: "Divehi", native: "ދިވެހި" },
    Language { code: "dz", name: "Dzongkha", native: "རྫོང་ཁ" },
    Language { code: "ee", name: "Eve", native: "Eʋegbe" },
    Language { code: "el", name: "Yunanca", native: "Ελληνικά" },
    Language { code: "en", name: "İngilizce", native: "English" },
    Language { code: "eo", name: "Esperanto", native: "Esperanto" },
    Language { code: "es", name: "İspanyolca", native: "Español" },
    Language { code: "et", name: "Estonca", native: "eesti" },
    Language { code: "eu", name: "Baskça", native: "euskara" },
    Language { code: "fa", name: "Farsça", native: "فارسی" },
    Language { code: "ff", name: "FULah", native: "Fulfulde" },
    Language { code: "fi", name: "Fince", native: "suomi" },
    Language { code: "fj", name: "Fijice", native: "vosa Vakaviti" },
    Language { code: "fo", name: "Faroece", native: "føroyskt" },
    Language { code: "fr", name: "Fransızca", native: "français" },
    Language { code: "fy", name: "Batı Frizce", native: "Frysk" },
    Language { code: "ga", name: "İrlandaca", native: "Gaeilge" },
    Language { code: "gd", name: "İskoçça", native: "Gàidhlig" },
    Language { code: "gl", name: "Galiççe", native: "galego" },
    Language { code: "gn", name: "Guarani", native: "Avañe'ẽ" },
    Language { code: "gu", name: "Gujarati", native: "ગુજરાતી" },
    Language { code: "gv", name: "Manks", native: "Gaelg" },
    Language { code: "ha", name: "Hausaca", native: "هَوُسَه" },
    Language { code: "he", name: "İbranice", native: "עברית" },
    Language { code: "hi", name: "Hintçe", native: "हिन्दी" },
    Language { code: "ho", name: "Hiri Motu", native: "Hiri Motu" },
    Language { code: "hr", name: "Hırvatça", native: "hrvatski" },
    Language { code: "ht", name: "Haiti", native: "Kreyòl ayisyen" },
    Language { code: "hu", name: "Macarca", native: "magyar" },
    Language { code: "hy", name: "Ermenice", native: "Հայերեն" },
    Language { code: "hz", name: "Herero", native: "Otjiherero" },
    Language { code: "ia", name: "Interlingua", native: "Interlingua" },
    Language { code: "id", name: "Endonezce", native: "Bahasa Indonesia" },
    Language { code: "ie", name: "Interlingue", native: "Interlingue" },
    Language { code: "ig", name: "İgbo", native: "Asụsụ Igbo" },
    Language { code: "ii", name: "Nuossu", native: "ꆈꌠ꒿ Nuosuhxop" },
    Language { code: "ik", name: "İnupik", native: "Iñupiaq" },
    Language { code: "io", name: "Ido", native: "Ido" },
    Language { code: "is", name: "İzlandaca", native: "Íslenska" },
    Language { code: "it", name: "İtalyanca", native: "Italiano" },
    Language { code: "iu", name: "İnuktitut", native: "ᐃᓄᒃᑎᑐᑦ" },
    Language { code: "ja", name: "Japonca", native: "日本語" },
    Language { code: "jv", name: "Cawaca", native: "basa Jawa" },
    Language { code: "ka", name: "Gürcüce", native: "ქართული" },
    Language { code: "kg", name: "Kongo", native: "Kikongo" },
    Language { code: "ki", name: "Kikuyu", native: "Gĩkũyũ" },
    Language { code: "kj", name: "Kuanama", native: "Kuanyama" },
    Language { code: "kk", name: "Kazakça", native: "қазақ тілі" },
    Language { code: "kl", name: "Kalaallisut", native: "kalaallisut" },
    Language { code: "km", name: "Kımerce", native: "ខ្មែរ" },
    Language { code: "kn", name: "Kannada", native: "ಕನ್ನಡ" },
    Language { code: "ko", name: "Korece", native: "한국어" },
    Language { code: "kr", name: "Kanuri", native: "Kanuri" },
    Language { code: "ks", name: "Keşmirce", native: "कश्मीरी" },
    Language { code: "ku", name: "Kürtçe", native: "Kurdî" },
    Language { code: "kv", name: "Komi", native: "коми кыв" },
    Language { code: "kw", name: "Kornca", native: "Kernewek" },
    Language { code: "ky", name: "Kırgızca", native: "Кыргызча" },
    Language { code: "la", name: "Latince", native: "latine" },
    Language { code: "lb", name: "Lüksemburgca", native: "Lëtzebuergesch" },
    Language { code: "lg", name: "Luganda", native: "Luganda" },
    Language { code: "li", name: "Limburga", native: "Limburgs" },
    Language { code: "ln", name: "Lingala", native: "Lingála" },
    Language { code: "lo", name: "Lao", native: "ພາສາລາວ" },
    Language { code: "lt", name: "Litvanca", native: "lietuvių kalba" },
    Language { code: "lu", name: "Luba-Katanga", native: "Kiluba" },
    Language { code: "lv", name: "Letonya", native: "latviešu valoda" },
    Language { code: "mg", name: "Malagaş", native: "fiteny malagasy" },
    Language { code: "mh", name: "Marshalca", native: "Kajin M̧ajeļ" },
    Language { code: "mi", name: "Maori", native: "te reo Māori" },
    Language { code: "mk", name: "Makedonca", native: "македонски" },
    Language { code: "ml", name: "Malayalamca", native: "മലയാളം" },
    Language { code: "mn", name: "Moğolca", native: "Монгол хэл" },
    Language { code: "mr", name: "Marathi", native: "मराठी" },
    Language { code: "ms", name: "Malayca", native: "Bahasa Melayu" },
    Language { code: "mt", name: "Malteca", native: "Malti" },
    Language { code: "my", name: "Birmanca", native: "မြန်မာစာ" },
    Language { code: "na", name: "Nauru", native: "Dorerin Naoero" },
    Language { code: "nb", name: "Norveç Bokmål", native: "norsk bokmål" },
    Language { code: "nd", name: "Kuzey Ndebele", native: "isiNdebele" },
    Language { code: "ne", name: "Nepalce", native: "नेपाली" },
    Language { code: "ng", name: "Ndonga", native: "Owambo" },
    Language { code: "nl", name: "Felemenkçe", native: "Nederlands" },
    Language { code: "nn", name: "Norveç Nynorsk", native: "norsk nynorsk" },
    Language { code: "no", name: "Norveççe", native: "norsk" },
    Language { code: "nr", name: "Güney Ndebele", native: "isiNdebele" },
    Language { code: "nv", name: "Navaho", native: "Diné bizaad" },
    Language { code: "ny", name: "Çiçeva", native: "chiCheŵa" },
    Language { code: "oc", name: "Oksitanca", native: "occitan" },
    Language { code: "oj", name: "Ojiibwa", native: "Ojibwe" },
    Language { code: "om", name: "Oromoca", native: "Afaan Oromoo" },
    Language { code: "or", name: "Odia", native: "ଓଡ଼ିଆ" },
    Language { code: "os", name: "Osetçe", native: "ирон æвзаг" },
    Language { code: "pa", name: "Pencabca", native: "ਪੰਜਾਬੀ" },
    Language { code: "pi", name: "Pali", native: "पाऴि" },
    Language { code: "pl", name: "Lehçe", native: "polski" },
    Language { code: "ps", name: "Peştuca", native: "پښتو" },
    Language { code: "pt", name: "Portekizce", native: "português" },
    Language { code: "pt-BR", name: "Brezilya Portekizcesi", native: "português brasileiro" },
    Language { code: "qu", name: "Kecuaca", native: "Runa Simi" },
    Language { code: "rm", name: "Romanşca", native: "rumantsch grischun" },
    Language { code: "rn", name: "Rundi", native: "Ikirundi" },
    Language { code: "ro", name: "Rumence", native: "română" },
    Language { code: "ru", name: "Rusça", native: "Русский" },
    Language { code: "rw", name: "Kinyarwanda", native: "Ikinyarwanda" },
    Language { code: "sa", name: "Sanskritçe", native: "संस्कृतम्" },
    Language { code: "sc", name: "Sarduca", native: "sardu" },
    Language { code: "sd", name: "Sindhi", native: "سنڌي" },
    Language { code: "se", name: "Kuzey Sami", native: "Davvisámegiella" },
    Language { code: "sg", name: "Sango", native: "yângâ tî sängö" },
    Language { code: "si", name: "Sinhalce", native: "සිංහල" },
    Language { code: "sk", name: "Slovakça", native: "slovenčina" },
    Language { code: "sl", name: "Slovence", native: "slovenščina" },
    Language { code: "sm", name: "Samoa", native: "gagana fa'a Samoa" },
    Language { code: "sn", name: "Şona", native: "chiShona" },
    Language { code: "so", name: "Somali", native: "Soomaaliga" },
    Language { code: "sq", name: "Arnavutça", native: "shqip" },
    Language { code: "sr", name: "Sırpça", native: "српски" },
    Language { code: "ss", name: "Svati", native: "siSwati" },
    Language { code: "st", name: "Güney Sotho", native: "Sesotho" },
    Language { code: "su", name: "Sundanaca", native: "Basa Sunda" },
    Language { code: "sv", name: "İsveçce", native: "svenska" },
    Language { code: "sw", name: "Svahili", native: "Kiswahili" },
    Language { code: "ta", name: "Tamilce", native: "தமிழ்" },
    Language { code: "te", name: "Telugu", native: "తెలుగు" },
    Language { code: "tg", name: "Tacikçe", native: "тоҷикӣ" },
    Language { code: "th", name: "Tayca", native: "ไทย" },
    Language { code: "ti", name: "Tigrinya", native: "ትግርኛ" },
    Language { code: "tk", name: "Türkmen", native: "Türkmençe" },
    Language { code: "tl", name: "Tagalog", native: "Wikang Tagalog" },
    Language { code: "tn", name: "Tswana", native: "Setswana" },
    Language { code: "to", name: "Tonga", native: "faka Tonga" },
    Language { code: "tr", name: "Türkçe", native: "Türkçe" },
    Language { code: "ts", name: "Tsonga", native: "Xitsonga" },
    Language { code: "tt", name: "Tatarca", native: "татар теле" },
    Language { code: "tw", name: "Twi", native: "Twi" },
    Language { code: "ty", name: "Tahitice", native: "Reo Tahiti" },
    Language { code: "ug", name: "Uygurca", native: "ئۇيغۇرچە" },
    Language { code: "uk", name: "Ukraynaca", native: "Українська" },
    Language { code: "ur", name: "Urduca", native: "اردو" },
    Language { code: "uz", name: "Özbek", native: "Oʻzbek" },
    Language { code: "ve", name: "Venda", native: "Tshivenḓa" },
    Language { code: "vi", name: "Vietnamesce", native: "Tiếng Việt" },
    Language { code: "vo", name: "Volapük", native: "volapük" },
    Language { code: "wa", name: "Valonca", native: "walon" },
    Language { code: "wo", name: "Wolof", native: "Wollof" },
    Language { code: "xh", name: "Xhosa", native: "isiXhosa" },
    Language { code: "yi", name: "Yidiş", native: "ייִדיש" },
    Language { code: "yo", name: "Yoruba", native: "Yorùbá" },
    Language { code: "za", name: "Zhuangca", native: "Saɯ cueŋƅ" },
    Language { code: "zh", name: "Çince", native: "中文" },
    Language { code: "zh-Hans", name: "Çince (Basitleştirilmiş)", native: "简体中文" },
    Language { code: "zh-Hant", name: "Çince (Geleneksel)", native: "繁體中文" },
    Language { code: "zu", name: "Zulu", native: "isiZulu" },
];

// Also keeps a hand-edited `settings.toml` from getting `../x` into a filename.
fn well_formed(code: &str) -> bool {
    let (primary, subtag) = match code.split_once('-') {
        Some((p, s)) => (p, Some(s)),
        None => (code, None),
    };
    let primary_ok = (2..=3).contains(&primary.len()) && primary.bytes().all(|b| b.is_ascii_alphabetic());
    if !primary_ok {
        return false;
    }
    match subtag {
        None => true,
        Some(s) => (2..=4).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric()),
    }
}

/// Some STT endpoints report the language by name instead of code.
pub fn from_display_name(name: &str) -> &str {
    let lowered = name.trim().to_lowercase();
    match lowered.as_str() {
        "turkish" | "turkce" | "türkçe" => "tr",
        "german" | "deutsch" => "de",
        "english" => "en",
        "spanish" | "espanol" | "español" => "es",
        "french" | "francais" | "français" => "fr",
        "italian" | "italiano" => "it",
        "japanese" => "ja",
        "korean" => "ko",
        "chinese" => "zh",
        "arabic" => "ar",
        "russian" => "ru",
        "portuguese" => "pt",
        "dutch" => "nl",
        "polish" => "pl",
        "azerbaijani" | "azerbaycanca" | "azerbayjanca" => "az",
        // Callers validate against the registry.
        _ => name.trim(),
    }
}

pub fn is_supported(code: &str) -> bool {
    by_code(code).is_some()
}

/// Case- and whitespace-insensitive.
pub fn by_code(code: &str) -> Option<&'static Language> {
    let trimmed = code.trim();
    if !well_formed(trimmed) {
        return None;
    }
    LANGUAGES
        .iter()
        .find(|l| l.code.eq_ignore_ascii_case(trimmed))
}

/// Dedupes and drops unknown codes; never returns an empty list.
pub fn normalise_all<S: AsRef<str>>(codes: &[S]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::with_capacity(codes.len());
    for code in codes {
        if let Some(lang) = by_code(code.as_ref()) {
            if !out.contains(&lang.code) {
                out.push(lang.code);
            }
        }
    }
    if out.is_empty() {
        out.push(DEFAULT_TARGET);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_internally_consistent() {
        let mut seen = Vec::new();
        for lang in LANGUAGES {
            assert!(!seen.contains(&lang.code), "duplicate code {}", lang.code);
            seen.push(lang.code);
            assert!(
                well_formed(lang.code),
                "{} is not a well-formed code",
                lang.code
            );
            assert!(!lang.name.trim().is_empty(), "{} has no name", lang.code);
            assert!(
                !lang.native.trim().is_empty(),
                "{} has no endonym",
                lang.code
            );
        }
        assert!(LANGUAGES.len() > 150, "registry looks truncated");
    }

    #[test]
    fn lookup_tolerates_casing_and_padding() {
        assert_eq!(by_code("EN").map(|l| l.code), Some("en"));
        assert_eq!(by_code("  de  ").map(|l| l.code), Some("de"));
        assert_eq!(by_code("zh-hans").map(|l| l.code), Some("zh-Hans"));
        assert_eq!(by_code("pt-br").map(|l| l.code), Some("pt-BR"));
    }

    #[test]
    fn subtitle_targets_are_present() {
        for code in [
            "en", "tr", "de", "fr", "es", "it", "pt", "pt-BR", "nl", "pl", "ru", "uk", "ar", "he",
            "fa", "hi", "bn", "ur", "zh-Hans", "zh-Hant", "ja", "ko", "id", "ms", "th", "vi", "ro",
            "hu", "cs", "sv", "da", "no", "fi", "el",
        ] {
            assert!(is_supported(code), "{code} must be selectable");
        }
    }

    #[test]
    fn unknown_codes_are_rejected() {
        for code in ["", "   ", "xx", "klingon", "e", "abcd", "en-x", "en-a-b"] {
            assert!(!is_supported(code), "{code:?} must not validate");
        }
    }

    #[test]
    fn path_shaped_input_never_validates() {
        // These reach `{target}.srt` and `{target}.track.json` filenames.
        for code in [
            "../../etc/passwd",
            "en/../x",
            "en/../../secret",
            "/etc/passwd",
            "en;rm -rf /",
            "..",
            "en\\x",
        ] {
            assert!(!is_supported(code), "{code:?} must not validate");
        }
    }

    #[test]
    fn normalise_all_drops_unknown_dedupes_and_defaults() {
        assert_eq!(normalise_all(&["de", "DE", "tr", "klingon"]), vec!["de", "tr"]);
        assert_eq!(normalise_all(&["klingon", "../../x"]), vec![DEFAULT_TARGET]);
        assert_eq!(normalise_all::<String>(&[]), vec![DEFAULT_TARGET]);
        assert_eq!(normalise_all(&["  EN "]), vec!["en"]);
    }

    #[test]
    fn reported_source_languages_resolve_to_registry_codes() {
        for reported in ["az", "tr", "en", "de", "ZH-HANS", "pt-BR"] {
            let lang = by_code(reported)
                .unwrap_or_else(|| panic!("{reported} picker'da görünmeli"));
            assert!(!lang.name.is_empty());
            assert!(!lang.native.is_empty());
            assert_eq!(by_code(lang.code).map(|l| l.code), Some(lang.code));
        }
        assert!(!is_supported("az-Latn-AZ"));
    }

    #[test]
    fn display_names_and_codes_share_one_table() {
        assert_eq!(from_display_name("Turkish"), "tr");
        assert_eq!(from_display_name("TÜRKÇE"), "tr");
        assert_eq!(from_display_name("german"), "de");
        assert_eq!(from_display_name("Azerbaycanca"), "az");
        assert_eq!(from_display_name("az"), "az");
        assert_eq!(from_display_name(" zh-Hans "), "zh-Hans");
        for name in ["Turkish", "german", "Azerbayjanca", "az"] {
            let code = from_display_name(name);
            assert!(is_supported(code), "{name} → {code} registry'de olmalı");
        }
    }
}
