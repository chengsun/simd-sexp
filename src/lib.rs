pub mod find_quote_transitions;
pub mod ranges;
pub mod start_stop_transitions;
pub mod utils;
pub mod vector_classifier;
pub mod xor_masked_adjacent;

#[ocaml::func]
pub fn hello_world() -> &'static str {
    "hello, world!"
}
