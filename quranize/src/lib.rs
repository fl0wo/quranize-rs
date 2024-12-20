//! [Quranize] encodes alphabetic text into quran text, a.k.a. transliteration.
//!
//! # Examples
//!
//! ## Adding crate quranize to a project's dependencies
//!
//! Run `cargo add quranize`, or add the following lines to `Cargo.toml` file.
//! ```toml
//! [dependencies]
//! quranize = "1.0"
//! ```
//!
//! ## Encoding alphabetic text to quran text
//!
//! ```
//! let q = quranize::Quranize::new();
//!
//! assert_eq!(q.encode("bismillahirrohmanirrohim")[0].0, "بِسمِ اللَّهِ الرَّحمـٰنِ الرَّحيم");
//! assert_eq!(q.encode("amma yatasa alun")[0].0, "عَمَّ يَتَساءَلون");
//!
//! let (i, _) = q.find("عَمَّ يَتَساءَلون")[0];
//! let sura = q.get_sura(i).unwrap();
//! let aya = q.get_aya(i).unwrap();
//! assert_eq!((sura, aya), (78, 1));
//! ```

mod normalization;
mod suffix_tree;
mod transliteration;

use suffix_tree::{Edge, Index};
use transliteration::{contextual_map, harf_muqottoah_map, map};

type EncodeResults = Vec<(String, usize, Vec<&'static str>)>;
type PrevMap = (char, &'static str);

const AYA_COUNT: usize = 6236;
const SURA_STARTS: [usize; 114] = [
    0, 7, 293, 493, 669, 789, 954, 1160, 1235, 1364, 1473, 1596, 1707, 1750, 1802, 1901, 2029,
    2140, 2250, 2348, 2483, 2595, 2673, 2791, 2855, 2932, 3159, 3252, 3340, 3409, 3469, 3503, 3533,
    3606, 3660, 3705, 3788, 3970, 4058, 4133, 4218, 4272, 4325, 4414, 4473, 4510, 4545, 4583, 4612,
    4630, 4675, 4735, 4784, 4846, 4901, 4979, 5075, 5104, 5126, 5150, 5163, 5177, 5188, 5199, 5217,
    5229, 5241, 5271, 5323, 5375, 5419, 5447, 5475, 5495, 5551, 5591, 5622, 5672, 5712, 5758, 5800,
    5829, 5848, 5884, 5909, 5931, 5948, 5967, 5993, 6023, 6043, 6058, 6079, 6090, 6098, 6106, 6125,
    6130, 6138, 6146, 6157, 6168, 6176, 6179, 6188, 6193, 6197, 6204, 6207, 6213, 6216, 6221, 6225,
    6230,
];
const QURAN_TXT: &str = include_str!("quran-simple-min.txt");

/// Quranize model, for doing transliteration, finding string, and getting aya.
pub struct Quranize {
    tree: suffix_tree::SuffixTree<'static>,
    saqs: Vec<(u8, u16, &'static str)>,
}

impl Quranize {
    const EXPECTED_VERTEX_COUNT: usize = 126_307;

    /// Create a new [`Quranize`] instance.
    pub fn new() -> Self {
        let mut tree = suffix_tree::SuffixTree::with_capacity(Self::EXPECTED_VERTEX_COUNT);
        let mut saqs = Vec::with_capacity(AYA_COUNT);
        let mut sura_num = 1;
        (0..AYA_COUNT)
            .zip(QURAN_TXT.split_inclusive('\n'))
            .map(|(i, q)| {
                sura_num += (i == SURA_STARTS.get(sura_num).copied().unwrap_or(AYA_COUNT)) as usize;
                let aya_num = i - SURA_STARTS[sura_num - 1] + 1;
                ((i, sura_num as u8, aya_num as u16), q)
            })
            .map(|((i, s, a), q)| ((i, s, a), Self::trim_basmalah(s, a, q)))
            .for_each(|((i, s, a), q)| {
                tree.construct(i, q);
                saqs.push((s, a, q.trim()));
            });
        Self { tree, saqs }
    }

    fn trim_basmalah(s: u8, a: u16, q: &str) -> &str {
        match (s, a) {
            (1, _) | (9, _) => q,
            (_, 1) => q.splitn(5, ' ').last().unwrap(),
            _ => q,
        }
    }

    /// Do transliteration on `s`, returning a list of tuple:
    /// - `String`: transliteration result / quran form
    /// - `usize`: location count where the quran form above is found in Alquran
    /// - `Vec<&'static str>`: explanation for each chars in the quran form above
    ///
    /// # Examples
    ///
    /// ```
    /// let q = quranize::Quranize::new();
    /// assert_eq!(q.encode("alif lam mim"), [("الم".to_string(), 912, vec!["alif", "lam", "mim"])]);
    /// assert_eq!(q.encode("minal jinnati wannas")[0].0, "مِنَ الجِنَّةِ وَالنّاس");
    /// ```
    pub fn encode(&self, s: &str) -> EncodeResults {
        let mut results: EncodeResults = match normalization::normalize(s).as_str() {
            "" => vec![],
            s => { self.tree.edges_from(0) }
                .flat_map(|&e| self.rev_encode(s, e, None))
                .collect(),
        }
        .into_iter()
        .chain(match normalization::normalize_muqottoah(s).as_str() {
            "" => vec![],
            s => { self.tree.edges_from(0) }
                .flat_map(|&e| self.rev_encode_muqottoah(s, e))
                .collect(),
        })
        .map(|(q, n, e)| (q.chars().rev().collect(), n, e.into_iter().rev().collect()))
        .collect();
        results.dedup_by(|x, y| x.0 == y.0);
        results
    }

    fn rev_encode(&self, s: &str, (v, w, l): Edge, pm: Option<PrevMap>) -> EncodeResults {
        let results_iter = l.chars().next().into_iter().flat_map(|c| -> EncodeResults {
            let tsls = map(c).iter().chain(contextual_map(pm.unzip().0, c));
            let tsl_results_iter = tsls.filter_map(|&tsl| -> Option<EncodeResults> {
                s.strip_prefix(tsl).map(|s| match s {
                    "" => vec![(c.to_string(), self.tree.count_data(w), vec![tsl])],
                    s => match &l[c.len_utf8()..] {
                        "" => { self.tree.edges_from(w) }
                            .flat_map(|&e| self.rev_encode(s, e, Some((c, tsl))))
                            .collect(),
                        l => self.rev_encode(s, (v, w, l), Some((c, tsl))),
                    }
                    .into_iter()
                    .map(|(mut q, n, mut e)| {
                        q.push(c);
                        e.push(tsl);
                        (q, n, e)
                    })
                    .collect(),
                })
            });
            tsl_results_iter.flatten().collect()
        });
        results_iter.collect()
    }

    fn rev_encode_muqottoah(&self, s: &str, (v, w, l): Edge) -> EncodeResults {
        let results_iter = l.chars().next().into_iter().flat_map(|c| -> EncodeResults {
            let tsls = harf_muqottoah_map(c).iter();
            let tsl_results_iter = tsls.filter_map(|&tsl| -> Option<EncodeResults> {
                s.strip_prefix(tsl).map(|s| match s {
                    "" => match self.tree.vertices[w].2 {
                        true => vec![(c.to_string(), self.tree.count_data(w), vec![tsl])],
                        false => vec![],
                    },
                    s => match &l[c.len_utf8()..] {
                        "" => { self.tree.edges_from(w) }
                            .flat_map(|&e| self.rev_encode_muqottoah(s, e))
                            .collect(),
                        l => self.rev_encode_muqottoah(s, (v, w, l)),
                    }
                    .into_iter()
                    .map(|(mut q, n, mut e)| {
                        q.push(c);
                        e.push(tsl);
                        (q, n, e)
                    })
                    .collect(),
                })
            });
            tsl_results_iter.flatten().collect()
        });
        results_iter.collect()
    }

    /// Find `s` in Alquran, returning a list of `Index`, where
    /// `Index` is a tuple, containing:
    /// - `usize`: aya row / aya offset (`0..6236`)
    /// - `usize`: string offset in a specific aya (`0..length of aya`)
    ///
    /// # Examples
    /// ```
    /// let q = quranize::Quranize::new();
    /// let index = q.find("عَمَّ يَتَساءَلون")[0];
    /// assert_eq!(index, (5672, 0));
    /// ```
    pub fn find(&self, s: &str) -> Vec<Index> {
        self.tree.find(s, 0)
    }

    /// Maps `i` into sura number, where `i` is an aya row / aya offset (`0..6236`).
    ///
    /// # Examples
    /// ```
    /// let q = quranize::Quranize::new();
    /// assert_eq!(q.get_sura(5672), Some(78));
    /// ```
    pub fn get_sura(&self, i: usize) -> Option<u8> {
        Some(self.saqs.get(i)?.0)
    }

    /// Maps `i` into aya number, where `i` is an aya row / aya offset (`0..6236`).
    ///
    /// # Examples
    /// ```
    /// let q = quranize::Quranize::new();
    /// assert_eq!(q.get_aya(5672), Some(1));
    /// ```
    pub fn get_aya(&self, i: usize) -> Option<u16> {
        Some(self.saqs.get(i)?.1)
    }

    /// Maps `i` into aya text, where `i` is an aya row / aya offset (`0..6236`).
    ///
    /// # Examples
    /// ```
    /// let q = quranize::Quranize::new();
    /// assert_eq!(q.get_quran(5672), Some("عَمَّ يَتَساءَلونَ"));
    /// ```
    pub fn get_quran(&self, i: usize) -> Option<&str> {
        Some(self.saqs.get(i)?.2)
    }

    pub fn decode(&self, s: &str) -> Vec<String> {
        let mut results = vec![];
        let mut harf_muqottoah = false;
        let mut buffer = String::new();

        for c in s.chars() {
            if harf_muqottoah {
                if let Some(tsl) = harf_muqottoah_map(c).first() {
                    buffer.push_str(tsl);
                    harf_muqottoah = false;
                }
            } else {
                if let Some(tsl) = map(c).first() {
                    buffer.push_str(tsl);
                } else {
                    // Push the character as-is if no mapping found
                    buffer.push(c);
                    harf_muqottoah = contains_harf_muqottoah(c);
                }
            }

            if !harf_muqottoah {
                results.push(buffer.clone());
                buffer.clear();
            }
        }

        // Combine results into a single string or a vector of strings
        let mut final_strs: Vec<String> = results.join(" ")
            .split("  ")
            .map(|s| s.to_string())
            .map(|s| s.trim().replace(" ", ""))
            .reduce(|mut a, b| {
                a.push(' ');
                a.push_str(&b);
                a
            })
            .into_iter()
            .collect();


        print!("{:?}", final_strs);

        // Now post-process each resulting string to handle the shadda
        for s in &mut final_strs {
            let mut chars: Vec<char> = s.chars().collect();
            let mut i = 0;
            while i < chars.len() {
                if chars[i] == '\u{0651}' {
                    // Found a shadda. If there's a previous character, duplicate it.
                    if i > 0 {
                        let prev_char = chars[i - 1];

                        // if it's already duplicated, skip the shadda
                        if i>1 {
                            let prev_prev_char = chars[i - 2];
                            if prev_char == prev_prev_char {
                                // If the previous char is the same as the one before it, remove the shadda
                                chars.remove(i);
                                i += 1;
                                continue;
                            }
                        }
                        // Replace the shadda with another instance of the previous char
                        // effectively: previous_char previous_char
                        chars.remove(i); // remove the shadda
                        chars.insert(i, prev_char);
                        // After insertion, the loop will move on, so we don't increment i here
                    } else {
                        // If shadda is at index 0 (unlikely), just remove it.
                        chars.remove(i);
                    }
                } else {
                    i += 1;
                }
            }

            // Convert back to string
            *s = chars.into_iter().collect();
        }

        final_strs
    }

}

fn contains_harf_muqottoah(p0: char) -> bool {
    matches!(p0, '\u{06D6}'..='\u{06DC}')
}

impl Default for Quranize {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    impl Quranize {
        fn e(&self, text: &str) -> Vec<String> {
            self.encode(text).into_iter().map(|r| r.0).collect()
        }
    }

    #[test]
    fn test_quranize_default() {
        let q: Quranize = Default::default();
        assert_eq!(q.e("illa billah"), ["إِلّا بِاللَّه"]);
        assert_eq!(q.e("alqur'an"), ["القُرآن"]);
        assert_eq!(q.e("bismillah"), ["بِسمِ اللَّه"]);
        assert_eq!(q.e("birobbinnas"), ["بِرَبِّ النّاس"]);
        assert_eq!(q.e("inna anzalnahu"), ["إِنّا أَنزَلناهُ"]);
        assert_eq!(q.e("wa'tasimu"), ["وَاعتَصِمو"]);
        assert_eq!(q.e("wa'tasimu bihablillah"), ["وَاعتَصِموا بِحَبلِ اللَّه"]);
        assert_eq!(q.e("idza qodho"), ["إِذا قَضَ"]);
        assert_eq!(q.e("masyaallah"), ["ما شاءَ اللَّه"]);
        assert_eq!(q.e("illa man taba"), ["إِلّا مَن تابَ"]);
        assert_eq!(q.e("alla tahzani"), ["أَلّا تَحزَني"]);
        assert_eq!(q.e("innasya niaka"), ["إِنَّ شانِئَكَ"]);
        assert_eq!(q.e("innasya ni'aka"), ["إِنَّ شانِئَكَ"]);
        assert_eq!(q.e("wasalamun alaihi"), ["وَسَلامٌ عَلَيهِ"]);
        assert_eq!(q.e("ulaika hum"), ["أُولـٰئِكَ هُم"]);
        assert_eq!(q.e("waladdoollin"), ["وَلَا الضّالّين"]);
        assert_eq!(q.e("undur kaifa"), ["انظُر كَيفَ"]);
        assert_eq!(q.e("lirrohman"), ["لِلرَّحمـٰن"]);
        assert_eq!(q.e("waantum muslimun"), ["وَأَنتُم مُسلِمون"]);
        assert_eq!(q.e("laa yukallifullah"), ["لا يُكَلِّفُ اللَّه"]);
        assert_eq!(q.e("robbil alamin"), ["رَبِّ العالَمين"]);
        assert_eq!(q.e("husnul maab"), ["حُسنُ المَآب"]);
        assert_eq!(q.e("khusnul ma'ab"), ["حُسنُ المَآب"]);
        assert_eq!(q.e("kufuwan"), ["كُفُوً"]);
        assert_eq!(q.e("yukhodiun"), ["يُخادِعون"]);
        assert_eq!(q.e("indallah"), ["عِندَ اللَّه"]);
        assert_eq!(q.e("alimul ghoibi"), ["عالِمُ الغَيبِ"]);
        assert_eq!(q.e("kaana dhoifa"), ["كانَ ضَعيفًا"]);
        assert_eq!(q.e("waantum muslimuna"), ["وَأَنتُم مُسلِمونَ"]);
        assert_eq!(q.e("kitabi la roiba"), ["الكِتابِ لا رَيبَ"]);
        assert_eq!(q.e("takwili"), ["تَأويلِ"]);
        assert_eq!(q.e("yu'minun"), ["يُؤمِنون"]);
        assert_eq!(q.e("hudan lil muttaqin"), ["هُدًى لِلمُتَّقين"]);
        assert_eq!(q.e("majreeha wamursaha"), ["مَجراها وَمُرساها"]);
        assert_eq!(q.e("fabiayyi alai"), ["فَبِأَيِّ آلاءِ"]);
        assert_eq!(q.e("wayuallimukumma"), ["وَيُعَلِّمُكُم ما"]);
        assert_eq!(q.e("wassolat"), ["وَالصَّلاة"]);
    }

    #[test]
    fn test_alfatihah() {
        let q = Quranize::new();
        assert_eq!(
            q.e("bismillahirrohmanirrohiim"),
            ["بِسمِ اللَّهِ الرَّحمـٰنِ الرَّحيم"]
        );
        assert_eq!(
            q.e("alhamdulilla hirobbil 'alamiin"),
            ["الحَمدُ لِلَّهِ رَبِّ العالَمين"]
        );
        assert_eq!(q.e("arrohma nirrohim"), ["الرَّحمـٰنِ الرَّحيم"]);
        assert_eq!(q.e("maliki yau middin"), ["مالِكِ يَومِ الدّين"]);
        assert_eq!(
            q.e("iyyakanakbudu waiyyakanastain"),
            ["إِيّاكَ نَعبُدُ وَإِيّاكَ نَستَعين"]
        );
        assert_eq!(q.e("ihdinassirotol mustaqim"), ["اهدِنَا الصِّراطَ المُستَقيم"]);
        assert_eq!(
            q.e("shirotolladzina an'amta 'alaihim ghoiril maghdzubi 'alaihim waladdoolliin"),
            ["صِراطَ الَّذينَ أَنعَمتَ عَلَيهِم غَيرِ المَغضوبِ عَلَيهِم وَلَا الضّالّين"]
        );
    }

    #[test]
    fn test_al_ikhlas() {
        let q = Quranize::new();
        assert_eq!(q.e("qulhuwallahuahad"), ["قُل هُوَ اللَّهُ أَحَد"]);
        assert_eq!(q.e("allahussomad"), ["اللَّهُ الصَّمَد"]);
        assert_eq!(q.e("lam yalid walam yulad"), ["لَم يَلِد وَلَم يولَد"]);
        assert_eq!(
            q.e("walam yakun lahu kufuwan ahad"),
            ["وَلَم يَكُن لَهُ كُفُوًا أَحَد"]
        );
    }

    #[test]
    fn test_harf_muqottoah() {
        let q = Quranize::new();
        assert_eq!(q.e("alif lam mim"), ["الم"]);
        assert_eq!(q.e("alif laaam miiim"), &["الم"]);
        assert_eq!(q.e("nuun"), &["ن"]);
        assert_eq!(q.e("kaaaf haa yaa aiiin shoood"), &["كهيعص"]);
        assert_eq!(q.e("kaf ha ya 'ain shod"), &["كهيعص"]);
        assert_eq!(q.e("alif lam ro"), &["الر"]);
    }

    #[test]
    fn test_quranize_empty_result() {
        let q = Quranize::new();
        let empty: [String; 0] = [];
        assert_eq!(q.e(""), empty);
        assert_eq!(q.e(" "), empty);
        assert_eq!(q.e(" -"), empty);
        assert_eq!(q.e("abcd"), empty);
        assert_eq!(q.e("1+2=3"), empty);
    }

    #[test]
    fn test_unique() {
        let q = Quranize::new();
        let results = q.e("masyaallah");
        let uresults = std::collections::HashSet::<&String>::from_iter(results.iter());
        let is_unique = results.len() == uresults.len();
        assert!(is_unique, "results are not unique. results: {:#?}", results);
    }

    #[test]
    fn test_tree_find() {
        let q = Quranize::new();
        assert!(q.find("بِسمِ").contains(&(0, 0)));
        assert_eq!(q.find("وَالنّاسِ").last(), Some(&(6235, 28)));
        assert!(q.find("الم").contains(&(7, 0)));
        assert_eq!(q.find("بِسمِ اللَّهِ الرَّحمـٰنِ الرَّحيمِ").len(), 2);
        assert!(q.find("").is_empty());
        assert!(q.find("نن").is_empty());
        assert!(q.find("ننن").is_empty());
        assert!(q.find("نننن").is_empty());
        assert!(q.find("2+3+4=9").is_empty());
        assert_eq!(q.find("بِسمِ اللَّهِ الرَّحمـٰنِ الرَّحيمِ").first(), Some(&(0, 0)));
        assert_eq!(q.find("الرَّحمـٰنِ الرَّحيمِ").first(), Some(&(0, 26)));
        assert_eq!(q.find("").first(), None);
        assert_eq!(q.find("abc").first(), None);
    }

    #[test]
    fn test_tree_props() {
        let t = Quranize::new().tree;
        assert_eq!(t.vertices.len(), t.edges.len() + 1);
        assert_eq!(t.count_data(0), t.collect_data(0).len());
        assert_eq!(t.vertices.len(), Quranize::EXPECTED_VERTEX_COUNT);
        assert!(t.vertices[0].2);
        assert!(!t.vertices[Quranize::EXPECTED_VERTEX_COUNT - 1].2);
    }

    #[test]
    fn test_decode() {
        let q = Quranize::new();

        assert_eq!(q.decode("إِنّا لِلَّهِ وَإِنّا إِلَيهِ رٰجِعونَ"), ["inna lillahi wa inna ilayhi raji wna"]);
        assert_eq!(q.decode("بِسمِ اللَّهِ الرَّحمـٰنِ الرَّحيم"), ["bismi allahi alrrahm ani alrrahym"]);
        assert_eq!(q.decode("الحَمدُ لِلَّهِ رَبِّ العالَمين"), ["alhamdu lillahi rabbi al alamyn"]);
        assert_eq!(q.decode("الرَّحمـٰنِ الرَّحيم"), ["alrrahm ani alrrahym"]);
        assert_eq!(q.decode("مالِكِ يَومِ الدّين"), ["maliki yawmi alddyn"]);
        assert_eq!(q.decode("إِيّاكَ نَعبُدُ وَإِيّاكَ نَستَعين"), ["iyyaka na budu wa iyyaka nasta yn"]);
        assert_eq!(q.decode("اهدِنَا الصِّراطَ المُستَقيم"), ["ahdinaa alssirata almustakym"]);
        assert_eq!(q.decode("صِراطَ الَّذينَ أَنعَمتَ عَلَيهِم غَيرِ المَغضوبِ عَلَيهِم وَلَا الضّالّين"), ["sirata alladyna an amta alayhim gayri almagdwbi alayhim walaa alddallyn"]);

        assert_eq!(q.decode("قُل هُوَ اللَّهُ أَحَد"), ["kul huwa allahu ahad"]);
        assert_eq!(q.decode("اللَّهُ الصَّمَد"), ["allahu alssamad"]);
        assert_eq!(q.decode("لَم يَلِد وَلَم يولَد"), ["lam yalid walam ywlad"]);
        assert_eq!(q.decode("وَلَم يَكُن لَهُ كُفُوًا أَحَد"), ["walam yakun lahu kufuwana ahad"]);

    }
}
