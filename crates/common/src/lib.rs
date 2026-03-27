pub mod dynamics;
pub mod inputs;
pub mod profile;
pub mod rpc;
pub mod sets;
pub mod structs;
pub mod util;

/// Panics on debug builds but only logs an error on release builds
#[macro_export]
macro_rules! debug_panic {
    ($($content:expr),+) => {
        debug_panic!(keyword: return, $($content),+);
    };
    (keyword: $branch:tt, $($content:expr),+) => {
        #[cfg(debug_assertions)]
        panic!($($content),+);
        #[cfg(not(debug_assertions))]
        {
            error!($($content),+);
            $branch;
        }
    };
}
