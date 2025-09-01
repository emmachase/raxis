use std::{
    hash::{DefaultHasher, Hash, Hasher},
    panic::Location,
};

pub const fn hash(module_path: &'static str, file: &'static str, line: u32, column: u32) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    let prime = 0x00000100000001B3;

    let mut bytes = module_path.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(prime);
        i += 1;
    }

    bytes = file.as_bytes();
    i = 0;
    while i < bytes.len() {
        hash ^= bytes[i] as u64;
        hash = hash.wrapping_mul(prime);
        i += 1;
    }

    hash ^= line as u64;
    hash = hash.wrapping_mul(prime);
    hash ^= column as u64;
    hash = hash.wrapping_mul(prime);
    hash
}

#[macro_export]
macro_rules! w_id {
    () => {{
        $crate::util::unique::hash(
            core::module_path!(),
            core::file!(),
            core::line!(),
            core::column!(),
        )
    }};
}

pub const fn id_from_location(location: &'static Location<'static>) -> u64 {
    hash("mod", location.file(), location.line(), location.column())
}

pub fn combine_id(id: u64, child_id: impl Hash) -> u64 {
    let mut s = DefaultHasher::new();
    s.write_u64(id);
    child_id.hash(&mut s);
    s.finish()
}
