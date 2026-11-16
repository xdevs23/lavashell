// SPDX-License-Identifier: GPL-3.0-only
use crate::error::Error;
use crate::languages::Language;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::path::Path;
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator, Tree};

pub struct Grammar {
    pub language: &'static Language,
    parser: Parser,
    comments: Query,
}

impl Grammar {
    fn load(language: &'static Language) -> Result<Self, Error> {
        let grammar = tree_sitter::Language::new(language.grammar);
        let mut parser = Parser::new();
        parser
            .set_language(&grammar)
            .map_err(|source| Error::LoadGrammar {
                language: language.name,
                source,
            })?;
        let comments =
            Query::new(&grammar, language.comments).map_err(|source| Error::CommentQuery {
                language: language.name,
                source,
            })?;
        Ok(Self {
            language,
            parser,
            comments,
        })
    }

    pub fn parse(&mut self, path: &Path, text: &str) -> Result<Tree, Error> {
        self.parser
            .parse(text, None)
            .ok_or_else(|| Error::ParseTree {
                path: path.to_path_buf(),
            })
    }

    pub fn comment_nodes<'tree>(&self, tree: &'tree Tree, text: &str) -> Vec<Node<'tree>> {
        let mut cursor = QueryCursor::new();
        let mut captures = cursor.captures(&self.comments, tree.root_node(), text.as_bytes());
        let mut comments = Vec::new();
        while let Some((found, index)) = captures.next() {
            comments.push(found.captures()[*index].node);
        }
        comments
    }
}

#[derive(Default)]
pub struct Grammars(BTreeMap<&'static str, Grammar>);

impl Grammars {
    pub fn of(&mut self, language: &'static Language) -> Result<&mut Grammar, Error> {
        Ok(match self.0.entry(language.name) {
            Entry::Occupied(grammar) => grammar.into_mut(),
            Entry::Vacant(slot) => slot.insert(Grammar::load(language)?),
        })
    }
}

pub fn source_of<'text>(node: &Node<'_>, text: &'text str) -> &'text str {
    text[node.byte_range()].trim_end()
}
