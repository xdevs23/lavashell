// SPDX-License-Identifier: GPL-3.0-only
use proc_macro2::{Ident, TokenStream, TokenTree};

pub fn identifiers(tokens: TokenStream) -> Vec<Ident> {
    tokens
        .into_iter()
        .flat_map(|token| match token {
            TokenTree::Ident(name) => vec![name],
            TokenTree::Group(group) => identifiers(group.stream()),
            TokenTree::Punct(_) | TokenTree::Literal(_) => Vec::new(),
        })
        .collect()
}

pub fn name_words(name: &str) -> Vec<String> {
    (name.split(|character: char| !character.is_alphanumeric()))
        .flat_map(case_words)
        .collect()
}

fn case_words(segment: &str) -> Vec<String> {
    let characters: Vec<char> = segment.chars().collect();
    let starts: Vec<usize> = (0..characters.len())
        .filter(|&index| index == 0 || starts_word(&characters, index))
        .collect();
    let ends = starts.iter().skip(1).copied().chain([characters.len()]);
    (starts.iter().copied().zip(ends))
        .map(|(start, end)| {
            (characters[start..end].iter())
                .flat_map(|c| c.to_lowercase())
                .collect()
        })
        .collect()
}

fn starts_word(characters: &[char], index: usize) -> bool {
    let (previous, current) = (characters[index - 1], characters[index]);
    current.is_uppercase() && (previous.is_lowercase() || previous.is_numeric())
}
