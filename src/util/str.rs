use string_interner::backend::{Backend, StringBackend};

use crate::layout::UIArenas;

#[derive(Debug, Clone, Hash)]
pub enum StableString {
    Static(&'static str),
    Interned(<StringBackend as Backend>::Symbol),
    Heap(String),
}

impl StableString {
    pub fn resolve<'a>(&'a self, arenas: &'a UIArenas) -> Option<&'a str> {
        match self {
            StableString::Static(s) => Some(s),
            StableString::Interned(s) => arenas.strings.resolve(*s),
            StableString::Heap(s) => Some(s.as_str()),
        }
    }
}

impl From<&'static str> for StableString {
    fn from(value: &'static str) -> Self {
        Self::Static(value)
    }
}
