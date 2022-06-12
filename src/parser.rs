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
pub enum SegmentIndex {
    EntireFile,
    Segment(usize),
}

pub trait Stage2 {
    type Return;

    fn process_bof(&mut self, segment_index: SegmentIndex, input_size_hint: Option<usize>);

    /// Returns the input index that must be preserved for next call.
    fn process_one(&mut self, input: Input, this_index: usize, next_index: usize, is_eof: bool) -> Result<usize, Error>;

    fn process_eof(&mut self) -> Result<Self::Return, Error>;
}

pub trait WritingStage2 {
    fn process_bof<WriteT: Write>(&mut self, writer: &mut WriteT, segment_index: SegmentIndex);

    /// Returns the input index that must be preserved for next call.
    fn process_one<WriteT: Write>(&mut self, writer: &mut WriteT, input: Input, this_index: usize, next_index: usize, is_eof: bool) -> Result<usize, Error>;

    fn process_eof<WriteT: Write>(&mut self, writer: &mut WriteT) -> Result<(), Error>;
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
    fn process_bof(&mut self, segment_index: SegmentIndex, _input_size_hint: Option<usize>) {
        self.writing_stage2.process_bof(self.writer, segment_index)
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

    fn process_bof(&mut self, _segment_index: SegmentIndex, input_size_hint: Option<usize>) {
        self.visitor.bof(input_size_hint);
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

/// Must be >= 64. Doesn't affect correctness or impose limitations on sexp being parsed.
const INDICES_BUFFER_MAX_LEN: usize = 8092;

pub struct State<ClassifierT, Stage2T> {
    stage2: Stage2T,
    structural_classifier: ClassifierT,
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

impl<ClassifierT: structural::Classifier, Stage2T: Stage2> State<ClassifierT, Stage2T> {
    pub fn new(structural_classifier: ClassifierT, stage2: Stage2T) -> Self {
        State {
            stage2,
            structural_classifier,
        }
    }

    pub fn process_streaming<BufReadT: BufRead>(&mut self, segment_index: SegmentIndex, buf_reader: &mut BufReadT) -> Result<Stage2T::Return, Error> {
        let mut input_index = 0;
        let mut indices_len = 0;
        let mut indices_buffer = Box::new([0; INDICES_BUFFER_MAX_LEN]);

        let mut input_start_index = 0;
        let mut input;

        self.stage2.process_bof(segment_index, None);

        match buf_reader.fill_buf() {
            Ok(&[]) => { return self.stage2.process_eof(); },
            Ok(buf) => {
                input = buf.to_owned();
                let len = buf.len();
                std::mem::drop(buf);
                buf_reader.consume(len);
            },
            Err(e) => { return Err(Error::IOError(e.kind())) },
        }

        loop {
            self.structural_classifier.structural_indices_bitmask(
                &input[(input_index - input_start_index)..],
                |bitmask, bitmask_len| {
                    extract::safe_generic(|bit_offset| {
                        indices_buffer[indices_len] = input_index + bit_offset;
                        indices_len += 1;
                    }, bitmask);

                    input_index += bitmask_len;
                    if indices_len + 64 <= INDICES_BUFFER_MAX_LEN {
                        structural::CallbackResult::Continue
                    } else {
                        structural::CallbackResult::Finish
                    }
                });

            let mut input_index_to_keep = input_start_index;
            for indices_index in 0..(indices_len.saturating_sub(1)) {
                input_index_to_keep =
                    self.stage2.process_one(
                        Input { input: &input[..], offset: input_start_index },
                        indices_buffer[indices_index],
                        indices_buffer[indices_index + 1],
                        false)?;
                debug_assert!(input_index_to_keep <= indices_buffer[indices_index + 1]);
            }

            if unlikely(input_index - input_start_index >= input.len()) {
                match buf_reader.fill_buf() {
                    Ok(&[]) => {
                        if indices_len > 0 {
                            self.stage2.process_one(
                                Input { input: &input[..], offset: input_start_index },
                                indices_buffer[indices_len - 1],
                                input.len() + input_start_index,
                                true)?;
                        }
                        return self.stage2.process_eof();
                    },
                    Ok(buf) => {
                        if indices_len > 0 {
                            let length_to_chop = input_index_to_keep - input_start_index;
                            let length_to_keep = input.len() - length_to_chop;
                            input_start_index += length_to_chop;
                            unsafe { std::ptr::copy(&input[length_to_chop] as *const u8, &mut input[0] as *mut u8, length_to_keep); }
                            input.truncate(length_to_keep);
                        } else {
                            input_start_index += input.len();
                            input.clear();
                        }

                        input.extend_from_slice(buf);
                        let len = buf.len();

                        std::mem::drop(buf);
                        buf_reader.consume(len);
                    },
                    Err(e) => { return Err(Error::IOError(e.kind())) },
                }
            }

            if indices_len > 0 {
                indices_buffer[0] = indices_buffer[indices_len - 1];
                indices_len = 1;
            }
        }
    }

    pub fn process_all(&mut self, segment_index: SegmentIndex, input: &[u8]) -> Result<Stage2T::Return, Error> {
        let mut input_index = 0;
        let mut indices_len = 0;
        let mut indices_buffer = [0; INDICES_BUFFER_MAX_LEN];

        self.stage2.process_bof(segment_index, Some(input.len()));

        loop {
            self.structural_classifier.structural_indices_bitmask(
                &input[input_index..],
                |bitmask, bitmask_len| {
                    extract::safe_generic(|bit_offset| {
                        indices_buffer[indices_len] = input_index + bit_offset;
                        indices_len += 1;
                    }, bitmask);

                    input_index += bitmask_len;
                    if indices_len + 64 <= INDICES_BUFFER_MAX_LEN {
                        structural::CallbackResult::Continue
                    } else {
                        structural::CallbackResult::Finish
                    }
                });

            for indices_index in 0..(indices_len.saturating_sub(1)) {
                self.stage2.process_one(Input { input, offset: 0 }, indices_buffer[indices_index], indices_buffer[indices_index + 1], false)?;
            }

            if input_index >= input.len() {
                if indices_len > 0 {
                    self.stage2.process_one(Input { input, offset: 0 }, indices_buffer[indices_len - 1], input.len(), true)?;
                }
                return self.stage2.process_eof();
            }

            indices_buffer[0] = indices_buffer[indices_len - 1];
            indices_len = 1;
        }
    }
}

pub trait Parse {
    type Return;
    fn process(&mut self, segment_index: SegmentIndex, input: &[u8]) -> Result<Self::Return, Error>;
}

impl<ClassifierT: structural::Classifier, Stage2T: Stage2> Parse for State<ClassifierT, Stage2T> {
    type Return = Stage2T::Return;
    fn process(&mut self, segment_index: SegmentIndex, input: &[u8]) -> Result<Self::Return, Error> {
        self.process_all(segment_index, input)
    }
}

pub trait Stream<BufReadT> {
    type Return;
    fn process_streaming(&mut self, segment_index: SegmentIndex, buf_reader: &mut BufReadT) -> Result<Self::Return, Error>;
}

impl<BufReadT: BufRead, ClassifierT: structural::Classifier, Stage2T: Stage2> Stream<BufReadT> for State<ClassifierT, Stage2T> {
    type Return = Stage2T::Return;
    fn process_streaming(&mut self, segment_index: SegmentIndex, buf_reader: &mut BufReadT) -> Result<Self::Return, Error> {
        self.process_streaming(segment_index, buf_reader)
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

pub fn parser_from_visitor<'a, VisitorT: Visitor + 'a>(visitor: VisitorT) -> Box<dyn Parse<Return = VisitorT::Return> + 'a> {
    parser_new(VisitorState::new(visitor))
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
