use crate::{escape, extract, structural};
use crate::utils::*;
use crate::visitor::*;
use std::io::{BufRead, Write};

#[derive(Copy, Clone)]
pub struct Input<'a> {
    pub offset: usize,
    pub input: &'a [u8],
}

/// Sometimes a `Stage2` is asked to produce a segment of an input sexp, not the
/// whole thing. The `Stage2` might want to know about this (e.g. a CSV writer
/// might want to print out the header only when processing the actual beginning
/// of the sexp.)
pub trait Stage2 {
    type Return;

    fn reset(&mut self, input_size_hint: Option<usize>);

    /// Returns the input index that must be preserved for next call.
    fn process_one(&mut self, input: Input, this_index: usize, next_index: usize, is_eof: bool) -> Result<usize, Error>;

    fn process_eof(&mut self) -> Result<Self::Return, Error>;
}

pub trait WritingStage2 {
    fn reset(&mut self);

    /// Returns the input index that must be preserved for next call.
    fn process_one<WriteT: Write>(&mut self, writer: &mut WriteT, input: Input, this_index: usize, next_index: usize, is_eof: bool) -> Result<usize, Error>;

    fn process_eof<WriteT: Write>(&mut self, writer: &mut WriteT) -> Result<(), Error>;
}

pub trait ExtractPartialResult {
    type PartialReturn;

    fn extract_partial_result(&mut self) -> Self::PartialReturn;
}

/// Adapter for a WritingStage2 to become a Stage2
/// This adapter simply takes the mutable reference to the writer at
/// construction time, and retains this for the lifetime of the adapter. This is
/// not suitable for parallel parsing; for that, see the corresponding
/// parser_parallel::WritingStage2Adapter.
pub struct WritingStage2Adapter<'a, WritingStage2T, WriteT> {
    writing_stage2: WritingStage2T,
    writer: &'a mut WriteT,
}

impl<'a, WriteT: Write, WritingStage2T: WritingStage2> WritingStage2Adapter<'a, WritingStage2T, WriteT> {
    pub fn new(writing_stage2: WritingStage2T, writer: &'a mut WriteT) -> Self {
        Self { writing_stage2, writer }
    }
}

impl<'a, WriteT: Write, WritingStage2T: WritingStage2> Stage2 for WritingStage2Adapter<'a, WritingStage2T, WriteT> {
    type Return = ();
    fn reset(&mut self, _input_size_hint: Option<usize>) {
        self.writing_stage2.reset();
    }
    #[inline(always)]
    fn process_one(&mut self, input: Input, this_index: usize, next_index: usize, is_eof: bool) -> Result<usize, Error> {
        self.writing_stage2.process_one(self.writer, input, this_index, next_index, is_eof)
    }
    fn process_eof(&mut self) -> Result<Self::Return, Error> {
        self.writing_stage2.process_eof(self.writer)
    }
}

/// Adapter for a Visitor to become a Stage2
pub struct VisitorState<VisitorT: Visitor> {
    visitor: VisitorT,
    context_stack: Vec<VisitorT::Context>,
    unescape: escape::GenericUnescape,
}

impl<VisitorT: Visitor> VisitorState<VisitorT> {
    pub fn new(visitor: VisitorT) -> Self {
        let unescape = escape::GenericUnescape::new();

        Self {
            visitor,
            context_stack: Vec::new(),
            unescape,
        }
    }
}

impl<VisitorT: Visitor> Stage2 for VisitorState<VisitorT> {
    type Return = VisitorT::Return;

    fn reset(&mut self, input_size_hint: Option<usize>) {
        self.visitor.reset(input_size_hint);
    }

    #[inline]
    fn process_one(&mut self, input: Input, this_index: usize, next_index: usize, _is_eof: bool) -> Result<usize, Error> {
        match input.input[this_index - input.offset] {
            b'(' => {
                let new_context = self.visitor.list_open(self.context_stack.last_mut());
                self.context_stack.push(new_context);
            },
            b')' => {
                let context = self.context_stack.pop().ok_or(Error::UnmatchedCloseParen)?;
                self.visitor.list_close(context, self.context_stack.last_mut());
            },
            b'"' => {
                use escape::Unescape;
                let start_index = this_index + 1;
                let end_index =
                    // NOTE: can't subtract one here because of the case where
                    // there is an EOF before closing quote
                    next_index;
                let length_upper_bound = end_index - start_index;
                let mut atom = self.visitor.atom_reserve(length_upper_bound);
                let (_input_consumed, atom_string_len) =
                    self.unescape.unescape(&input.input[(start_index - input.offset)..],
                                           self.visitor.atom_borrow(&mut atom))
                    .ok_or(Error::BadQuotedAtom)?;
                self.visitor.atom(atom, atom_string_len, self.context_stack.last_mut());
            },
            ch => {
                if ch != b' ' && ch != b'\t' && ch != b'\n' {
                    let length = next_index - this_index;
                    let mut atom = self.visitor.atom_reserve(length);
                    {
                        let output = self.visitor.atom_borrow(&mut atom);
                        unsafe { std::ptr::copy_nonoverlapping(
                            &input.input[this_index - input.offset] as *const u8,
                            &mut output[0] as *mut u8,
                            length) };
                    }
                    self.visitor.atom(atom, length, self.context_stack.last_mut());
                }
            }
        }
        Ok(next_index)
    }

    fn process_eof(&mut self) -> Result<VisitorT::Return, Error> {
        if self.context_stack.len() > 0 {
            return Err(Error::UnmatchedOpenParen);
        }
        Ok(self.visitor.eof())
    }

}

impl<VisitorT: Visitor + ExtractPartialResult> ExtractPartialResult for VisitorState<VisitorT> {
    type PartialReturn = VisitorT::PartialReturn;
    fn extract_partial_result(&mut self) -> Self::PartialReturn {
        self.visitor.extract_partial_result()
    }
}

/// Must be >= 64. Doesn't affect correctness or impose limitations on sexp being parsed.
const INDICES_BUFFER_MAX_LEN: usize = 8092;

pub struct State<ClassifierT, Stage2T> {
    stage2: Stage2T,
    structural_classifier: ClassifierT,
    input_index: usize,
    indices_len: usize,
    indices_buffer: Box<[usize; INDICES_BUFFER_MAX_LEN]>,
    input: Vec<u8>,
    input_start_index: usize,
    input_index_to_keep: usize,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    UnmatchedOpenParen,
    UnmatchedCloseParen,
    BadQuotedAtom,
    IOError(std::io::ErrorKind),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::UnmatchedOpenParen => { write!(f, "Unmatched open paren") }
            Error::UnmatchedCloseParen => { write!(f, "Unmatched close paren") }
            Error::BadQuotedAtom => { write!(f, "Bad quoted atom") }
            Error::IOError(e) => { write!(f, "IO error: {}", e) }
        }
    }
}

impl<ClassifierT, Stage2T: Stage2 + ExtractPartialResult> ExtractPartialResult for State<ClassifierT, Stage2T> {
    type PartialReturn = Stage2T::PartialReturn;

    fn extract_partial_result(&mut self) -> Self::PartialReturn {
        self.stage2.extract_partial_result()
    }
}

impl<ClassifierT: structural::Classifier, Stage2T: Stage2> State<ClassifierT, Stage2T> {
    pub fn new(structural_classifier: ClassifierT, stage2: Stage2T) -> Self {
        State {
            stage2,
            structural_classifier,
            input_index: 0,
            indices_len: 0,
            indices_buffer: Box::new([0; INDICES_BUFFER_MAX_LEN]),
            input: Vec::new(),
            input_start_index: 0,
            input_index_to_keep: 0,
        }
    }

    pub fn reset(&mut self, input_size_hint: Option<usize>) {
        self.stage2.reset(input_size_hint);
        self.input_index = 0;
        self.indices_len = 0;
        self.input.clear();
        self.input_start_index = 0;
        self.input_index_to_keep = 0;
    }

    pub fn process_partial(&mut self, new_input: &[u8]) -> Result<(), Error> {
        self.input.extend_from_slice(new_input);

        loop {
            self.structural_classifier.structural_indices_bitmask(
                &self.input[(self.input_index - self.input_start_index)..],
                |bitmask, bitmask_len| {
                    extract::safe_generic(|bit_offset| {
                        self.indices_buffer[self.indices_len] = self.input_index + bit_offset;
                        self.indices_len += 1;
                    }, bitmask);

                    self.input_index += bitmask_len;
                    if self.indices_len + 64 <= INDICES_BUFFER_MAX_LEN {
                        structural::CallbackResult::Continue
                    } else {
                        structural::CallbackResult::Finish
                    }
                });

            self.input_index_to_keep = self.input_start_index;
            for indices_index in 0..(self.indices_len.saturating_sub(1)) {
                self.input_index_to_keep =
                    self.stage2.process_one(
                        Input { input: &self.input[..], offset: self.input_start_index },
                        self.indices_buffer[indices_index],
                        self.indices_buffer[indices_index + 1],
                        false)?;
                debug_assert!(self.input_index_to_keep <= self.indices_buffer[indices_index + 1]);
            }

            if self.indices_len > 0 {
                self.indices_buffer[0] = self.indices_buffer[self.indices_len - 1];
                self.indices_len = 1;
            }

            if unlikely(self.input_index - self.input_start_index >= self.input.len()) {
                let length_to_chop = self.input_index_to_keep - self.input_start_index;
                let length_to_keep = self.input.len() - length_to_chop;
                self.input_start_index += length_to_chop;
                if length_to_keep > 0 {
                    unsafe { std::ptr::copy(&self.input[length_to_chop] as *const u8, &mut self.input[0] as *mut u8, length_to_keep); }
                }
                self.input.truncate(length_to_keep);
                break;
            }
        }

        Ok(())
    }

    // TODO: I think I want this function to consume self, but that plays
    // weirdly with Box<dyn Parser>
    pub fn process_eof(&mut self) -> Result<Stage2T::Return, Error> {
        if self.indices_len > 0 {
            debug_assert!(self.indices_len == 1);
            self.stage2.process_one(
                Input { input: &self.input[..], offset: self.input_start_index },
                self.indices_buffer[self.indices_len - 1],
                self.input.len() + self.input_start_index,
                true)?;
        }
        self.stage2.process_eof()
    }

    pub fn process_streaming<BufReadT: BufRead>(&mut self, buf_reader: &mut BufReadT) -> Result<Stage2T::Return, Error> {
        self.reset(None);

        loop {
            match buf_reader.fill_buf() {
                Ok(&[]) => { return self.process_eof(); },
                Ok(buf) => {
                    self.process_partial(buf)?;
                    let len = buf.len();
                    std::mem::drop(buf);
                    buf_reader.consume(len);
                },
                Err(e) => { return Err(Error::IOError(e.kind())) },
            }
        }
    }

    pub fn process_all(&mut self, input: &[u8]) -> Result<Stage2T::Return, Error> {
        self.reset(Some(input.len()));

        loop {
            self.structural_classifier.structural_indices_bitmask(
                &input[self.input_index..],
                |bitmask, bitmask_len| {
                    extract::safe_generic(|bit_offset| {
                        self.indices_buffer[self.indices_len] = self.input_index + bit_offset;
                        self.indices_len += 1;
                    }, bitmask);

                    self.input_index += bitmask_len;
                    if self.indices_len + 64 <= INDICES_BUFFER_MAX_LEN {
                        structural::CallbackResult::Continue
                    } else {
                        structural::CallbackResult::Finish
                    }
                });

            for indices_index in 0..(self.indices_len.saturating_sub(1)) {
                self.stage2.process_one(Input { input, offset: 0 }, self.indices_buffer[indices_index], self.indices_buffer[indices_index + 1], false)?;
            }

            if self.input_index >= input.len() {
                if self.indices_len > 0 {
                    self.stage2.process_one(Input { input, offset: 0 }, self.indices_buffer[self.indices_len - 1], input.len(), true)?;
                }
                return self.stage2.process_eof();
            }

            self.indices_buffer[0] = self.indices_buffer[self.indices_len - 1];
            self.indices_len = 1;
        }
    }
}

pub trait Parse {
    type Return;
    fn process(&mut self, input: &[u8]) -> Result<Self::Return, Error>;
}

impl<ClassifierT: structural::Classifier, Stage2T: Stage2> Parse for State<ClassifierT, Stage2T> {
    type Return = Stage2T::Return;
    fn process(&mut self, input: &[u8]) -> Result<Self::Return, Error> {
        self.process_all(input)
    }
}

pub trait ParsePartial: Parse + ExtractPartialResult {
    fn process_partial(&mut self, input: &[u8]) -> Result<(), Error>;
    fn process_eof(&mut self) -> Result<Self::Return, Error>;
}

impl<ClassifierT: structural::Classifier, Stage2T: Stage2 + ExtractPartialResult> ParsePartial for State<ClassifierT, Stage2T> {
    fn process_partial(&mut self, input: &[u8]) -> Result<(), Error> {
        self.process_partial(input)
    }
    fn process_eof(&mut self) -> Result<Self::Return, Error> {
        self.process_eof()
    }
}

pub trait Stream<BufReadT> {
    type Return;
    fn process_streaming(&mut self, buf_reader: &mut BufReadT) -> Result<Self::Return, Error>;
}

impl<BufReadT: BufRead, ClassifierT: structural::Classifier, Stage2T: Stage2> Stream<BufReadT> for State<ClassifierT, Stage2T> {
    type Return = Stage2T::Return;
    fn process_streaming(&mut self, buf_reader: &mut BufReadT) -> Result<Self::Return, Error> {
        self.process_streaming(buf_reader)
    }
}

struct MakeParserFromClassifierCps<Stage2T> {
    stage2: Stage2T,
}

impl<'a, Stage2T: Stage2 + 'a> structural::MakeClassifierCps<'a> for MakeParserFromClassifierCps<Stage2T> {
    type Return = Box<dyn Parse<Return = Stage2T::Return> + 'a>;
    fn f<ClassifierT: structural::Classifier + 'a>(self: Self, classifier: ClassifierT) -> Self::Return {
        Box::new(State::new(classifier, self.stage2))
    }
}

pub fn parser_new<'a, Stage2T: Stage2 + 'a>(stage2: Stage2T) -> Box<dyn Parse<Return = Stage2T::Return> + 'a> {
    structural::make_classifier_cps(MakeParserFromClassifierCps { stage2 })
}

struct MakePartialParserFromClassifierCps<Stage2T> {
    stage2: Stage2T,
}

impl<'a, Stage2T: Stage2 + ExtractPartialResult + 'a> structural::MakeClassifierCps<'a> for MakePartialParserFromClassifierCps<Stage2T> {
    type Return = Box<dyn ParsePartial<Return = Stage2T::Return, PartialReturn = Stage2T::PartialReturn> + 'a>;
    fn f<ClassifierT: structural::Classifier + 'a>(self: Self, classifier: ClassifierT) -> Self::Return {
        Box::new(State::new(classifier, self.stage2))
    }
}

pub fn partial_parser_new<'a, Stage2T: Stage2 + ExtractPartialResult + 'a>(stage2: Stage2T) -> Box<dyn ParsePartial<Return = Stage2T::Return, PartialReturn = Stage2T::PartialReturn> + 'a> {
    structural::make_classifier_cps(MakePartialParserFromClassifierCps { stage2 })
}

pub fn parser_from_visitor<'a, VisitorT: Visitor + 'a>(visitor: VisitorT) -> Box<dyn Parse<Return = VisitorT::Return> + 'a> {
    parser_new(VisitorState::new(visitor))
}

pub fn partial_parser_from_visitor<'a, VisitorT: Visitor + ExtractPartialResult + 'a>(visitor: VisitorT) -> Box<dyn ParsePartial<Return = VisitorT::Return, PartialReturn = VisitorT::PartialReturn> + 'a> {
    partial_parser_new(VisitorState::new(visitor))
}

pub fn parser_from_writing_stage2<'a, WriteT: Write, WritingStage2T: WritingStage2 + 'a>
    (writing_stage2: WritingStage2T, writer: &'a mut WriteT)
     -> Box<dyn Parse<Return = ()> + 'a>
{
    parser_new(WritingStage2Adapter::new(writing_stage2, writer))
}

pub fn parser_from_sexp_factory<'a, SexpFactoryT: SexpFactory + 'a>
    (sexp_factory: SexpFactoryT)
     -> Box<dyn Parse<Return = Vec<SexpFactoryT::Sexp>> + 'a>
{
    parser_from_visitor(SimpleVisitor::new(sexp_factory))
}

struct MakeStreamingFromClassifierCps<Stage2T, BufReadT> {
    stage2: Stage2T,
    phantom: std::marker::PhantomData<*const BufReadT>,
}

impl<'a, Stage2T: Stage2 + 'a, BufReadT: BufRead> structural::MakeClassifierCps<'a> for MakeStreamingFromClassifierCps<Stage2T, BufReadT> {
    type Return = Box<dyn Stream<BufReadT, Return = Stage2T::Return> + 'a>;
    fn f<ClassifierT: structural::Classifier + 'a>(self: Self, classifier: ClassifierT) -> Self::Return {
        Box::new(State::new(classifier, self.stage2))
    }
}

pub fn streaming_new<'a, Stage2T: Stage2 + 'a, BufReadT: BufRead>(stage2: Stage2T) -> Box<dyn Stream<BufReadT, Return = Stage2T::Return> + 'a> {
    structural::make_classifier_cps(MakeStreamingFromClassifierCps { stage2, phantom: std::marker::PhantomData })
}

pub fn streaming_from_visitor<'a, VisitorT: Visitor + 'a, BufReadT: BufRead>(visitor: VisitorT) -> Box<dyn Stream<BufReadT, Return = VisitorT::Return> + 'a> {
    streaming_new(VisitorState::new(visitor))
}

pub fn streaming_from_writing_stage2<'a, WriteT: Write, WritingStage2T: WritingStage2 + 'a, BufReadT: BufRead>
    (writing_stage2: WritingStage2T, writer: &'a mut WriteT)
     -> Box<dyn Stream<BufReadT, Return = ()> + 'a>
{
    streaming_new(WritingStage2Adapter::new(writing_stage2, writer))
}

pub fn streaming_from_sexp_factory<'a, SexpFactoryT: SexpFactory + 'a, BufReadT: BufRead>
    (sexp_factory: SexpFactoryT)
     -> Box<dyn Stream<BufReadT, Return = Vec<SexpFactoryT::Sexp>> + 'a>
{
    streaming_from_visitor(SimpleVisitor::new(sexp_factory))
}
