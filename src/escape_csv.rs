pub fn escape_is_necessary(input: &[u8]) -> bool {
    memchr::memchr3(b'"', b',', b'\n', input).is_some()
}

pub fn escape<WriterT: std::io::Write>(input: &[u8], output: &mut WriterT) -> Result<(), std::io::Error> {
    output.write_all(b"\"")?;
    let mut i = 0;
    for next_quote in memchr::memchr_iter(b'"', input) {
        output.write_all(&input[i..next_quote])?;
        i = next_quote;
    }
    output.write_all(&input[i..])?;
    output.write_all(b"\"")?;
    Ok(())
}

