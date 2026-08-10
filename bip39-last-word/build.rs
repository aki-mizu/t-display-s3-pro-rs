use bip39::Language;
use pinyin::ToPinyinMulti;
use std::{env, fmt::Write as _, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let mut entries = Vec::new();
    for (index, word) in Language::SimplifiedChinese.word_list().iter().enumerate() {
        let mut characters = word.chars();
        let character = characters
            .next()
            .expect("Simplified Chinese BIP39 words must not be empty");
        assert!(
            characters.next().is_none(),
            "Simplified Chinese BIP39 words must contain one character: {word}"
        );

        let pronunciations = character
            .to_pinyin_multi()
            .unwrap_or_else(|| panic!("missing pinyin for Simplified Chinese BIP39 word: {word}"));
        for pronunciation in pronunciations {
            let spelling = pronunciation
                .plain()
                .replace("u:", "v")
                .replace('\u{00fc}', "v");
            assert!(
                spelling.bytes().all(|byte| byte.is_ascii_lowercase()),
                "pinyin spelling must be lowercase ASCII: {spelling}"
            );
            entries.push((
                spelling,
                u16::try_from(index).expect("BIP39 index fits in u16"),
            ));
        }
    }

    entries.sort_unstable();
    entries.dedup();

    let mut generated = String::new();
    writeln!(
        generated,
        "static SIMPLIFIED_CHINESE_PINYIN_ENTRIES: [PinyinEntry; {}] = [",
        entries.len()
    )
    .expect("write generated pinyin table");
    for (spelling, index) in entries {
        writeln!(
            generated,
            "    PinyinEntry {{ spelling: {spelling:?}, index: {index} }},"
        )
        .expect("write generated pinyin entry");
    }
    generated.push_str("];\n");

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"))
        .join("simplified_chinese_pinyin.rs");
    fs::write(output, generated).expect("write generated pinyin table");
}
