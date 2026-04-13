//! Upstream-parity search index and scorer.
//!
//! This is a Rust port of the tokenizer pipeline in
//! <https://github.com/Mattilsynet/matvaretabellen-deux/blob/main/src/matvaretabellen/search.cljc>.
//!
//! Key properties:
//!
//! * Pure library: no I/O. All inputs are passed as slices so this module is
//!   trivially testable and independent of `data.rs`.
//! * Case- and diacritic-insensitive: `bønne` and `bonne` both hit the same
//!   entries.
//! * Edge n-gram index keyed by prefixes of length 3..=min(len, 10). Tokens
//!   shorter than 3 are indexed as-is so single/two-letter searches still
//!   work.
//! * Stopwords filtered per locale (short `nb` / `en` lists).
//! * AND semantics across query tokens: every non-stopword query token must
//!   match.
//! * Scoring: +10 for a full normalized `foodName` match; +5 per whole-word
//!   query token hit in `foodName`; +3 per whole-word hit in `searchKeywords`;
//!   +1 per token matched via n-gram only. Ties broken by shorter `foodName`.

use crate::types::Food;
use std::collections::{HashMap, HashSet};

pub struct SearchIndex {
    /// Prefix -> sorted, deduped list of food indices that contain a token
    /// whose edge n-gram (or the token itself, for short tokens) equals the
    /// prefix.
    prefix_index: HashMap<String, Vec<usize>>,
    /// For each food index, the set of normalized whole-word tokens that
    /// appeared in its `foodName`. Used for the +5 / +1 distinction.
    name_tokens: Vec<HashSet<String>>,
    /// For each food index, the set of normalized whole-word tokens that
    /// appeared in any `searchKeywords` entry. Used for the +3 scoring term.
    keyword_tokens: Vec<HashSet<String>>,
    /// Precomputed fully-normalized `foodName` per food (single flat string
    /// with single-space separators between tokens). Used for the +10 exact
    /// match test and tie-breaking.
    normalized_names: Vec<String>,
    /// Original `foodName` length per food, used as the tie-breaker ("shorter
    /// name = more specific").
    name_lens: Vec<usize>,
    /// Stopword set for the locale the index was built against. Queries are
    /// tokenized with the same set so behavior is symmetric.
    stopwords: HashSet<&'static str>,
}

impl SearchIndex {
    pub fn build(foods: &[Food], locale_code: &str) -> Self {
        let stopwords = stopwords_for(locale_code);
        let mut prefix_index: HashMap<String, Vec<usize>> = HashMap::new();
        let mut name_tokens: Vec<HashSet<String>> = Vec::with_capacity(foods.len());
        let mut keyword_tokens: Vec<HashSet<String>> = Vec::with_capacity(foods.len());
        let mut normalized_names: Vec<String> = Vec::with_capacity(foods.len());
        let mut name_lens: Vec<usize> = Vec::with_capacity(foods.len());

        for (i, food) in foods.iter().enumerate() {
            let name_toks = tokenize(&food.food_name, &stopwords);
            let normalized_name = name_toks.join(" ");
            let mut name_set: HashSet<String> = HashSet::new();
            for t in &name_toks {
                name_set.insert(t.clone());
                index_token(&mut prefix_index, t, i);
            }

            let mut keyword_set: HashSet<String> = HashSet::new();
            for kw in &food.search_keywords {
                for t in tokenize(kw, &stopwords) {
                    keyword_set.insert(t.clone());
                    index_token(&mut prefix_index, &t, i);
                }
            }

            name_tokens.push(name_set);
            keyword_tokens.push(keyword_set);
            normalized_names.push(normalized_name);
            name_lens.push(food.food_name.len());
        }

        // Final dedup pass on every posting list. `index_token` skips trailing
        // duplicates as it writes, but two different tokens on the same food
        // can share a prefix (e.g. `eggerore` and `egg` both yield key
        // `egg`), so a proper sort+dedup is required for deterministic
        // intersection results.
        for postings in prefix_index.values_mut() {
            postings.sort_unstable();
            postings.dedup();
        }

        SearchIndex {
            prefix_index,
            name_tokens,
            keyword_tokens,
            normalized_names,
            name_lens,
            stopwords,
        }
    }

    pub fn search(&self, query: &str) -> Vec<usize> {
        let q_tokens = tokenize(query, &self.stopwords);
        if q_tokens.is_empty() {
            return Vec::new();
        }
        let full_query_normalized = q_tokens.join(" ");

        // AND across query tokens: intersect posting lists.
        let mut candidates: Option<HashSet<usize>> = None;
        for qt in &q_tokens {
            let key = prefix_key(qt);
            let postings: HashSet<usize> = match self.prefix_index.get(&key) {
                Some(v) => v.iter().copied().collect(),
                None => return Vec::new(),
            };
            candidates = Some(match candidates {
                None => postings,
                Some(prev) => prev.intersection(&postings).copied().collect(),
            });
        }
        let candidates = match candidates {
            Some(c) if !c.is_empty() => c,
            _ => return Vec::new(),
        };

        // Score each candidate.
        let mut scored: Vec<(i64, usize, usize)> = candidates
            .into_iter()
            .map(|idx| {
                let mut score: i64 = 0;
                if self.normalized_names[idx] == full_query_normalized {
                    score += 10;
                }
                for qt in &q_tokens {
                    let in_name = self.name_tokens[idx].contains(qt);
                    let in_keywords = self.keyword_tokens[idx].contains(qt);
                    if in_name {
                        score += 5;
                    }
                    if in_keywords {
                        score += 3;
                    }
                    if !in_name && !in_keywords {
                        // Matched only via n-gram prefix.
                        score += 1;
                    }
                }
                (score, self.name_lens[idx], idx)
            })
            .collect();

        // Sort: score desc, then name_len asc (shorter = more specific), then
        // idx asc for a stable deterministic order.
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

        scored.into_iter().map(|(_, _, idx)| idx).collect()
    }
}

/// Derive the lookup key for a query token: the first `min(len, 10)` chars of
/// the normalized token. Short tokens (< 3 chars) are indexed as-is, matching
/// the build step.
fn prefix_key(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    let n = chars.len().min(10);
    chars.into_iter().take(n).collect()
}

/// Normalize + split + stopword-filter. Pure, so reused by both build and
/// search to guarantee symmetric behavior.
fn tokenize(s: &str, stopwords: &HashSet<&'static str>) -> Vec<String> {
    let normalized = normalize(s);
    normalized
        .split_whitespace()
        .filter(|t| !t.is_empty() && !stopwords.contains(*t))
        .map(|t| t.to_string())
        .collect()
}

/// Lowercase, fold diacritics, collapse non-alphanumeric runs to a single
/// space. The fold table follows the Norwegian + common European letter set
/// referenced by upstream `search.cljc` (which folds via a small table, not
/// full Unicode NFD) plus the ones we actually observe in the food names.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let low = ch.to_lowercase();
        for c in low {
            let folded: &str = match c {
                'ø' => "o",
                'å' => "a",
                'æ' => "ae",
                'é' | 'è' | 'ê' | 'ë' => "e",
                'á' | 'à' | 'â' | 'ä' => "a",
                'í' | 'ì' | 'î' | 'ï' => "i",
                'ó' | 'ò' | 'ô' | 'ö' => "o",
                'ú' | 'ù' | 'û' | 'ü' => "u",
                'ñ' => "n",
                'ç' => "c",
                'ß' => "ss",
                other if other.is_ascii_alphanumeric() => {
                    out.push(other);
                    continue;
                }
                other if other.is_alphanumeric() => {
                    // Unknown letter/digit with no explicit fold — pass
                    // through lowercased. Queries via this path round-trip
                    // consistently.
                    out.push(other);
                    continue;
                }
                _ => {
                    // Punctuation, whitespace, symbols → single space.
                    if !out.ends_with(' ') {
                        out.push(' ');
                    }
                    continue;
                }
            };
            out.push_str(folded);
        }
    }
    out.trim().to_string()
}

/// Emit all edge n-grams for a token and stamp `food_idx` into each bucket.
/// For tokens shorter than 3 chars, use the token itself as the only key so
/// single/two-letter searches still work (matches the "short tokens indexed
/// as-is" rule from the plan).
fn index_token(prefix_index: &mut HashMap<String, Vec<usize>>, token: &str, food_idx: usize) {
    let chars: Vec<char> = token.chars().collect();
    let n = chars.len();
    if n == 0 {
        return;
    }
    if n < 3 {
        let key: String = chars.iter().collect();
        push_unique(prefix_index.entry(key).or_default(), food_idx);
        return;
    }
    let upper = n.min(10);
    for len in 3..=upper {
        let key: String = chars[..len].iter().collect();
        push_unique(prefix_index.entry(key).or_default(), food_idx);
    }
}

fn push_unique(v: &mut Vec<usize>, x: usize) {
    if v.last() != Some(&x) {
        v.push(x);
    }
}

fn stopwords_for(locale_code: &str) -> HashSet<&'static str> {
    let list: &[&'static str] = match locale_code {
        "nb" | "no" => &[
            "og", "i", "til", "med", "av", "på", "en", "et", "er", "som", "for", "eller", "fra",
        ],
        _ => &[
            "and", "or", "of", "with", "to", "the", "a", "an", "in", "on", "for", "from", "by",
        ],
    };
    let mut set: HashSet<&'static str> = HashSet::new();
    for w in list {
        set.insert(*w);
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Food;

    const NB_FOODS_JSON: &str = include_str!("../data/nb/foods.json");

    fn load_nb() -> Vec<Food> {
        let env: crate::types::FoodsEnvelope =
            serde_json::from_str(NB_FOODS_JSON).expect("foods.json parses");
        env.foods
    }

    #[test]
    fn normalize_folds_diacritics() {
        assert_eq!(normalize("Bønne"), "bonne");
        assert_eq!(normalize("Røykelaks"), "roykelaks");
        assert_eq!(normalize("Adzukibønner, tørr"), "adzukibonner torr");
        assert_eq!(normalize("café"), "cafe");
    }

    #[test]
    fn empty_query_returns_empty() {
        let foods = load_nb();
        let idx = SearchIndex::build(&foods, "nb");
        assert!(idx.search("").is_empty());
        assert!(idx.search("   ").is_empty());
    }

    #[test]
    fn stopword_only_query_returns_empty() {
        let foods = load_nb();
        let idx = SearchIndex::build(&foods, "nb");
        assert!(idx.search("og").is_empty());
        assert!(idx.search("og i til").is_empty());
    }

    #[test]
    fn bonne_finds_adzukibonner() {
        let foods = load_nb();
        let idx = SearchIndex::build(&foods, "nb");
        let hits = idx.search("bønne");
        assert!(!hits.is_empty(), "expected hits for 'bønne'");
        let any_bonn = hits
            .iter()
            .take(10)
            .any(|&i| normalize(&foods[i].food_name).contains("bonn"));
        assert!(
            any_bonn,
            "no 'bonn' match in top hits: {:?}",
            hits.iter()
                .take(10)
                .map(|&i| &foods[i].food_name)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn bonne_without_diacritic_matches_same() {
        let foods = load_nb();
        let idx = SearchIndex::build(&foods, "nb");
        let a = idx.search("bønne");
        let b = idx.search("bonne");
        assert!(!a.is_empty());
        assert!(!b.is_empty());
        // Top hit should be identical: diacritic folding must produce the same
        // posting list for both queries.
        assert_eq!(
            a.first(),
            b.first(),
            "top hit differs: bønne={:?} bonne={:?}",
            a.first().map(|&i| &foods[i].food_name),
            b.first().map(|&i| &foods[i].food_name),
        );
    }

    #[test]
    fn egg_matches_compound_names() {
        let foods = load_nb();
        let idx = SearchIndex::build(&foods, "nb");
        let hits = idx.search("egg");
        assert!(!hits.is_empty(), "expected hits for 'egg'");
        let has_egg = hits.iter().take(20).any(|&i| {
            let n = normalize(&foods[i].food_name);
            n.contains("egg") || n.contains("egge")
        });
        assert!(has_egg);
    }

    #[test]
    fn laks_matches() {
        let foods = load_nb();
        let idx = SearchIndex::build(&foods, "nb");
        let hits = idx.search("laks");
        assert!(!hits.is_empty());
        let has_laks = hits
            .iter()
            .take(20)
            .any(|&i| normalize(&foods[i].food_name).contains("laks"));
        assert!(has_laks);
    }

    #[test]
    fn and_semantics_intersect() {
        let foods = load_nb();
        let idx = SearchIndex::build(&foods, "nb");
        // Any two-token query must return a subset of either single-token
        // query's hits.
        let both = idx.search("egg kokt");
        let only_egg: HashSet<usize> = idx.search("egg").into_iter().collect();
        for i in &both {
            assert!(
                only_egg.contains(i),
                "AND semantics broken: {} not in single-token 'egg' hits",
                foods[*i].food_name
            );
        }
    }
}
