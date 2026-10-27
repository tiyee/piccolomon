//! Concrete algorithms and storage adapters. No GPUI dependencies.
pub mod files;
pub mod highlight;
pub mod language;
pub mod math;
pub mod mermaid;
pub mod pdf;
pub mod pdf_v2;
pub mod updates;

mod print_layout;

#[cfg(test)]
mod pdf_tests;
