//! The domain layer of mm: data types and rules without infrastructure access.

#[path = "../src/domain/mod.rs"]
pub mod domain;

#[cfg(test)]
#[allow(dead_code)]
#[path = "../src/test_support.rs"]
pub(crate) mod test_support;
