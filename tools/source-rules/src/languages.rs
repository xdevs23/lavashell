// SPDX-License-Identifier: GPL-3.0-only
use crate::safety_comments::safety_comments;
use std::collections::BTreeSet;
use tree_sitter::Node;
use tree_sitter_language::LanguageFn;

const COMMENT_NODES: &str = "(comment) @comment";
const LINE_AND_BLOCK_COMMENT_NODES: &str = "[(line_comment) (block_comment)] @comment";
const RUST_COMMENT_NODES: &str = concat!(
    "[(line_comment) (block_comment)] @comment\n",
    "(attribute (identifier) @comment value: (_) (#eq? @comment \"doc\"))\n",
    "(token_tree (identifier) @comment . \"=\" (#eq? @comment \"doc\"))\n",
);
pub const HASH: CommentMarkers = CommentMarkers {
    open: "#",
    close: "",
};
const SLASHES: CommentMarkers = CommentMarkers {
    open: "//",
    close: "",
};
const NO_PREAMBLE: &[&str] = &[];
const SHEBANG: &[&str] = &["#!"];
const XML_DECLARATION: &[&str] = &["<?xml"];

pub type NecessaryComments = fn(&[Node<'_>], &str) -> BTreeSet<usize>;

#[derive(Clone, Copy, Debug)]
pub struct CommentMarkers {
    pub open: &'static str,
    pub close: &'static str,
}

pub struct Language {
    pub name: &'static str,
    pub grammar: LanguageFn,
    pub comments: &'static str,
    pub markers: CommentMarkers,
    pub preamble: &'static [&'static str],
    pub necessary_comments: NecessaryComments,
}

impl Language {
    pub fn has_preamble(&self, text: &str) -> bool {
        self.preamble.iter().any(|prefix| text.starts_with(prefix))
    }

    pub fn is(&self, language: &'static Language) -> bool {
        std::ptr::eq(self, language)
    }
}

fn no_necessary_comments(_comments: &[Node<'_>], _text: &str) -> BTreeSet<usize> {
    BTreeSet::new()
}

pub static BASH: Language = Language {
    name: "Bash",
    grammar: tree_sitter_bash::LANGUAGE,
    comments: COMMENT_NODES,
    markers: HASH,
    preamble: SHEBANG,
    necessary_comments: no_necessary_comments,
};

pub static CPP: Language = Language {
    name: "C++",
    grammar: tree_sitter_cpp::LANGUAGE,
    comments: COMMENT_NODES,
    markers: SLASHES,
    preamble: NO_PREAMBLE,
    necessary_comments: no_necessary_comments,
};

pub static KOTLIN: Language = Language {
    name: "Kotlin",
    grammar: tree_sitter_kotlin_ng::LANGUAGE,
    comments: LINE_AND_BLOCK_COMMENT_NODES,
    markers: SLASHES,
    preamble: NO_PREAMBLE,
    necessary_comments: no_necessary_comments,
};

pub static NIX: Language = Language {
    name: "Nix",
    grammar: tree_sitter_nix::LANGUAGE,
    comments: COMMENT_NODES,
    markers: HASH,
    preamble: NO_PREAMBLE,
    necessary_comments: no_necessary_comments,
};

pub static PYTHON: Language = Language {
    name: "Python",
    grammar: tree_sitter_python::LANGUAGE,
    comments: "(comment) @comment (expression_statement . (string) .) @comment",
    markers: HASH,
    preamble: SHEBANG,
    necessary_comments: no_necessary_comments,
};

pub static RUST: Language = Language {
    name: "Rust",
    grammar: tree_sitter_rust::LANGUAGE,
    comments: RUST_COMMENT_NODES,
    markers: SLASHES,
    preamble: NO_PREAMBLE,
    necessary_comments: safety_comments,
};

pub static SKSL: Language = Language {
    name: "SkSL",
    grammar: tree_sitter_glsl::LANGUAGE_GLSL,
    comments: COMMENT_NODES,
    markers: SLASHES,
    preamble: NO_PREAMBLE,
    necessary_comments: no_necessary_comments,
};

pub static TOML: Language = Language {
    name: "TOML",
    grammar: tree_sitter_toml_ng::LANGUAGE,
    comments: COMMENT_NODES,
    markers: HASH,
    preamble: NO_PREAMBLE,
    necessary_comments: no_necessary_comments,
};

pub static XML: Language = Language {
    name: "XML",
    grammar: tree_sitter_xml::LANGUAGE_XML,
    comments: "(Comment) @comment",
    markers: CommentMarkers {
        open: "<!--",
        close: " -->",
    },
    preamble: XML_DECLARATION,
    necessary_comments: no_necessary_comments,
};
