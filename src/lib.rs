pub mod clmul;
pub mod escape;
pub mod escape_csv;
#[cfg(feature = "threads")]
pub mod exec_parallel;
pub mod extract;
pub mod find_quote_transitions;
#[cfg(feature = "ocaml")]
pub mod ocaml_parser;
pub mod parser;
#[cfg(feature = "threads")]
pub mod parser_parallel;
pub mod print;
pub mod ranges;
pub mod rust_generator;
pub mod rust_parser;
pub mod select;
pub mod start_stop_transitions;
pub mod structural;
pub mod utils;
pub mod varint;
pub mod visitor;
pub mod vector_classifier;
pub mod xor_masked_adjacent;
