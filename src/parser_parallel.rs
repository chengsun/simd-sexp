use crate::parser;
use std::io::Write;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};

pub type Error = parser::Error;
pub type Input<'a> = parser::Input<'a>;

pub trait Stage2Joiner {
    type Stage2 : parser::Stage2;
    type FinalReturnType;

    fn process_bof(&mut self, input_size_hint: Option<usize>);
    fn create_stage2(&mut self) -> Self::Stage2;
    fn join(&mut self, result: <Self::Stage2 as parser::Stage2>::FinalReturnType) -> Result<(), Error>;
    fn process_eof(&mut self) -> Result<Self::FinalReturnType, Error>;
}

pub struct State<JoinerT> {
    joiner: JoinerT,
}

/// An internal adapter used by [WritingJoiner].
pub struct WritingStage2Adapter<WritingStage2> {
    buffer: Vec<u8>,
    writing_stage2: WritingStage2,
}

impl<WritingStage2T: parser::WritingStage2> WritingStage2Adapter<WritingStage2T> {
    fn new(writing_stage2: WritingStage2T) -> Self {
        let buffer = Vec::new();
        Self { buffer, writing_stage2 }
    }
}

impl<WritingStage2T: parser::WritingStage2> parser::Stage2 for WritingStage2Adapter<WritingStage2T> {
    type FinalReturnType = Vec<u8>;
    #[inline(always)]
    fn process_bof(&mut self, segment_index: parser::SegmentIndex, input_size_hint: Option<usize>) {
        self.buffer.clear();
        input_size_hint.map(|input_size_hint| { self.buffer.reserve(input_size_hint) });
        self.writing_stage2.process_bof(&mut self.buffer, segment_index)
    }
    #[inline(always)]
    fn process_one(&mut self, input: parser::Input, this_index: usize, next_index: usize) -> Result<usize, parser::Error> {
        self.writing_stage2.process_one(&mut self.buffer, input, this_index, next_index)
    }
    #[inline(always)]
    fn process_eof(&mut self) -> Result<Self::FinalReturnType, parser::Error> {
        let () = self.writing_stage2.process_eof(&mut self.buffer)?;
        Ok(std::mem::take(&mut self.buffer))
    }
}

pub struct WritingJoiner<'a, WriteT, WritingStage2T> {
    create_writing_stage2: Box<dyn Fn() -> WritingStage2T + 'a>,
    writer: &'a mut WriteT,
}

impl<'a, WriteT: Write, WritingStage2T: parser::WritingStage2> WritingJoiner<'a, WriteT, WritingStage2T> {
    fn new(create_writing_stage2: Box<dyn Fn() -> WritingStage2T + 'a>, writer: &'a mut WriteT) -> Self {
        Self {
            create_writing_stage2,
            writer,
        }
    }
}

impl<'a, WriteT: Write, WritingStage2T: parser::WritingStage2> Stage2Joiner for WritingJoiner<'a, WriteT, WritingStage2T> {
    type Stage2 = WritingStage2Adapter<WritingStage2T>;
    type FinalReturnType = ();
    fn process_bof(&mut self, _input_size_hint: Option<usize>) {
    }
    fn create_stage2(&mut self) -> Self::Stage2 {
        WritingStage2Adapter::new((self.create_writing_stage2)())
    }
    fn join(&mut self, result: <Self::Stage2 as parser::Stage2>::FinalReturnType) -> Result<(), Error> {
        self.writer.write_all(&result[..]).map_err(|e| {
            Error::IOError(e.kind())
        })
    }
    fn process_eof(&mut self) -> Result<Self::FinalReturnType, Error> {
        Ok(())
    }
}

// Start of main parallel parser implementation

struct WorkUnit {
    index: usize,
    buffer: Vec<u8>,
}

struct WorkResult<ResultT> {
    index: usize,
    result: ResultT,
}

/// The minimum size of a chunk. The first valid break point (new line) after
/// this point will form the end of the chunk.
const CHUNK_SIZE: usize = 1048576;

/// The number of worker threads to spawn.
const N_THREADS: usize = 3;

/// The maximum number of chunks ahead of the last fully-joined (in order) chunk
/// that we're willing to start processing of.
const CHUNK_LOOKAHEAD: usize = 10 * N_THREADS;

impl<JoinerT: Stage2Joiner> State<JoinerT>
where
    JoinerT::Stage2 : Send,
    <JoinerT::Stage2 as parser::Stage2>::FinalReturnType : Send
{
    pub fn new(joiner: JoinerT) -> Self {
        State {
            joiner,
        }
    }

    fn thread(
        stage2: JoinerT::Stage2,
        local: crossbeam_deque::Worker<WorkUnit>,
        global: &crossbeam_deque::Injector<WorkUnit>,
        results: &crossbeam_queue::ArrayQueue<WorkResult<<JoinerT::Stage2 as parser::Stage2>::FinalReturnType>>,
        is_eof: &AtomicBool)
        -> Result<(), Error>
    {
        #[cfg(feature = "vtune")] let domain = ittapi::Domain::new(std::thread::current().name().unwrap());

        let mut state = parser::State::new(stage2);
        loop {
            let work_unit = loop {
                match local.pop() {
                    Some(work_unit) => {
                        break work_unit;
                    },
                    None => {
                        // We load the atomic bool here, with "acquire"
                        // ordering, which means that if this is true, we're
                        // guaranteed that all of the work in the queue that was
                        // inserted prior to EOF are visible to this thread.
                        //
                        // Note that this is the latest we can load this with
                        // acquire ordering -- in particular it would not be
                        // correct only to check is_eof after we have obtained a
                        // steal_result of None.
                        let is_eof = is_eof.load(Ordering::Acquire);
                        let mut steal_result;
                        loop {
                            steal_result = global.steal();
                            if !steal_result.is_retry() {
                                break;
                            }
                        }
                        match steal_result.success() {
                            Some(work_unit) => { break work_unit; },
                            None => {
                                if is_eof {
                                    return Ok(());
                                } else {
                                    std::hint::spin_loop();
                                    continue;
                                };
                            }
                        }
                    },
                }
            };

            #[cfg(feature = "vtune")] let task = ittapi::Task::begin(&domain, "work_unit");
            let result = state.process_all(parser::SegmentIndex::Segment(work_unit.index), &work_unit.buffer[..])?;
            results.push(WorkResult{ index: work_unit.index, result }).map_err(|_| ()).unwrap();
            #[cfg(feature = "vtune")] task.end();
        }
    }

    pub fn process_streaming<'a, BufReadT : std::io::BufRead>(&'a mut self, buf_reader: &mut BufReadT) -> Result<JoinerT::FinalReturnType, Error> {
        #[cfg(feature = "vtune")] let domain = ittapi::Domain::new("IO");

        self.joiner.process_bof(None);
        let work_queue = crossbeam_deque::Injector::new();
        let results_queue = crossbeam_queue::ArrayQueue::new(CHUNK_LOOKAHEAD);
        let mut output_queue: VecDeque<Option<<JoinerT::Stage2 as parser::Stage2>::FinalReturnType>> = VecDeque::with_capacity(CHUNK_LOOKAHEAD);
        let mut output_queue_start_index = 0;

        let is_eof = AtomicBool::new(false);
        let mut is_eof_local = false; // same as is_eof but local to the main thread

        let threads_result = crossbeam_utils::thread::scope(|scope| {
            // TODO: if we return early in the main loop, nothing ever sets
            // is_eof to true, which means the other threads never die.

            for thread_index in 0..N_THREADS {
                let stage2 = self.joiner.create_stage2();
                // TODO: because we never actually steal a batch into the local
                // worker queue, there's no actual use for [local_work_queue].
                // Change one of these two facts.
                let local_work_queue = crossbeam_deque::Worker::new_fifo();
                scope.builder().name(format!("worker #{}", thread_index + 1)).spawn(|_| {
                    // TODO: figure out a better error handling story
                    Self::thread(stage2, local_work_queue, &work_queue, &results_queue, &is_eof).unwrap()
                }).unwrap();
            }

            let mut next_work_unit = Vec::new();
            let mut next_work_unit_index = 0;
            loop {
                // handle all outputs
                loop {
                    match results_queue.pop() {
                        None => { break; },
                        Some(result) => {
                            let rel_index = result.index - output_queue_start_index;
                            debug_assert!(output_queue[rel_index].is_none());
                            output_queue[rel_index] = Some(result.result);
                            while let Some(Some(_)) = output_queue.front() {
                                #[cfg(feature = "vtune")] let task = ittapi::Task::begin(&domain, "handle_output");
                                let in_order_result = output_queue.pop_front().unwrap().unwrap();
                                output_queue_start_index += 1;
                                self.joiner.join(in_order_result)?;
                                #[cfg(feature = "vtune")] task.end();
                            }
                        },
                    }
                }

                // handle one input, if we can
                if !is_eof_local && output_queue.len() <= CHUNK_LOOKAHEAD {
                    #[cfg(feature = "vtune")] let task = ittapi::Task::begin(&domain, "handle_input");
                    match buf_reader.fill_buf() {
                        Err(e) => { return Err(Error::IOError(e.kind())) },
                        Ok(&[]) => {
                            let work_unit_to_dispatch = std::mem::take(&mut next_work_unit);
                            let work_unit_index = next_work_unit_index;
                            work_queue.push(WorkUnit { index: work_unit_index, buffer: work_unit_to_dispatch });
                            output_queue.push_back(None);

                            next_work_unit_index += 1;

                            is_eof.store(true, Ordering::Release);
                            is_eof_local = true;
                        },
                        Ok(buffer) => {
                            let len = buffer.len();
                            let split_index =
                                if next_work_unit.len() + len > CHUNK_SIZE {
                                    let offset = CHUNK_SIZE.saturating_sub(next_work_unit.len());
                                    memchr::memchr_iter(b'\n', &(*buffer)[offset..])
                                        .map(|x| x + offset)
                                        .find(|split_index| { split_index + 1 < len && buffer[split_index + 1] != b' ' })
                                } else {
                                    None
                                };
                            match split_index {
                                Some(split_index) => {
                                    next_work_unit.extend_from_slice(&buffer[..split_index]);
                                    let work_unit_to_dispatch = std::mem::take(&mut next_work_unit);
                                    let work_unit_index = next_work_unit_index;
                                    work_queue.push(WorkUnit { index: work_unit_index, buffer: work_unit_to_dispatch });
                                    output_queue.push_back(None);

                                    next_work_unit.extend_from_slice(&buffer[split_index..]);
                                    next_work_unit_index += 1;
                                },
                                None => {
                                    next_work_unit.extend_from_slice(&buffer[..]);
                                },
                            }
                            std::mem::drop(buffer);
                            buf_reader.consume(len);
                        },
                    }
                    #[cfg(feature = "vtune")] task.end();
                } else {
                    if is_eof_local && output_queue.is_empty() {
                        break Ok(());
                    }
                    std::hint::spin_loop();
                }
            }
        });

        let () = match threads_result {
            Err(e) => std::panic::resume_unwind(e),
            Ok(result) => result?,
        };

        self.joiner.process_eof()
    }
}

impl<'a, WriteT: Write, WritingStage2T: parser::WritingStage2 + Send> State<WritingJoiner<'a, WriteT, WritingStage2T>> {
    pub fn from_writing_stage2<F: Fn() -> WritingStage2T + 'a>(create_writing_stage2: F, writer: &'a mut WriteT) -> Self {
        Self::new(WritingJoiner::new(Box::new(create_writing_stage2), writer))
    }
}
