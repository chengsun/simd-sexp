use crate::{parser_parallel, select};
use std::io::Write;

pub fn process_streaming<'a, KeysT: IntoIterator<Item = &'a [u8]>, BufReadT : std::io::BufRead, StdoutT: Write>
    (keys: KeysT,
     buf_reader: &mut BufReadT,
     stdout: &mut StdoutT)
     -> Result<(), parser_parallel::Error>
{
    let keys: Vec<&'a [u8]> = keys.into_iter().collect();
    let mut parser =
        parser_parallel::State::from_writing_stage2(|| {
            select::Stage2::new(keys.iter().map(|x| *x), select::OutputCsv::new(false))
        }, stdout);
    parser.process_streaming(buf_reader)
}
