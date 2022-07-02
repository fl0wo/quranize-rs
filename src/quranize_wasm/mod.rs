use std::collections::HashMap;

use crate::Quranize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = Quranize)]
pub struct JsQuranize {
    quranize: Quranize,
    aya_map: HashMap<(u8, u16), &'static str>,
}

#[wasm_bindgen(js_class = Quranize)]
impl JsQuranize {
    #[wasm_bindgen(constructor)]
    pub fn new(word_count_limit: u8) -> Self {
        let mut aya_map = HashMap::new();
        for (s, a, t) in crate::quran::simple_plain_iter() {
            aya_map.insert((s, a), t);
        }
        Self {
            quranize: match word_count_limit {
                0 => Quranize::default(),
                n => Quranize::new(n),
            },
            aya_map,
        }
    }

    pub fn encode(&self, text: &str) -> JsValue {
        let encode_results = self
            .quranize
            .encode(text)
            .into_iter()
            .map(|(quran, locations, explanations)| {
                let word_count = quran.split_whitespace().count();
                JsEncodeResult {
                    quran,
                    locations: self.to_js_locations(locations, word_count),
                    explanations,
                }
            })
            .collect::<Vec<_>>();
        JsValue::from_serde(&encode_results).unwrap()
    }

    fn to_js_locations(&self, locations: &[(u8, u16, u8)], word_count: usize) -> Vec<JsLocation> {
        locations
            .iter()
            .map(|&(sura_number, aya_number, word_number)| {
                let aya_text = self.aya_map.get(&(sura_number, aya_number)).unwrap();
                let mut words = aya_text.split_whitespace();
                JsLocation {
                    sura_number,
                    aya_number,
                    before_text: take_join(&mut words, (word_number - 1).into()),
                    text: take_join(&mut words, word_count),
                    after_text: take_join(&mut words, usize::MAX),
                }
            })
            .collect()
    }
}

fn take_join<'a>(words: &mut impl Iterator<Item = &'a str>, n_take: usize) -> String {
    words.take(n_take).collect::<Vec<_>>().join(" ")
}

#[derive(serde::Serialize)]
struct JsEncodeResult<'a> {
    quran: String,
    locations: Vec<JsLocation>,
    explanations: Vec<&'a str>,
}

#[derive(serde::Serialize)]
struct JsLocation {
    sura_number: u8,
    aya_number: u16,
    before_text: String,
    text: String,
    after_text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_take_join() {
        let mut words = "ab cd ef g".split_whitespace();
        assert_eq!(&take_join(&mut words, 0), "");
        assert_eq!(&take_join(&mut words, 2), "ab cd");
        assert_eq!(&take_join(&mut words, usize::MAX), "ef g");
    }
}