pub mod str;
pub mod unique;
pub mod windows;

#[macro_export]
macro_rules! impl_numeric {
    (From<[$($source:ty),+ $(,)?]> for $target:ty => |$value:ident| $impl:block) => {
        $(
            impl From<$source> for $target {
                fn from($value: $source) -> Self {
                    $impl
                }
            }
        )+
    };
}
