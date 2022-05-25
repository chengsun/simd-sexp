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
const CHUNK_SIZE: usize = 256 * 1024;

pub struct State<JoinerT> {
    joiner: JoinerT,
    num_threads: usize,
}

impl<JoinerT: Stage2Joiner> State<JoinerT>
where
    JoinerT::Stage2 : Send,
    <JoinerT::Stage2 as parser::Stage2>::FinalReturnType : Send
{
    pub fn with_num_threads(joiner: JoinerT, num_threads: usize) -> Self {
        assert!(num_threads >= 2, "parser_parallel requires at least two threads: one for the I/O thread and at least one worker thread.");
        State {
            joiner,
            num_threads,
        }
    }

    pub fn new(joiner: JoinerT) -> Self {
        Self::with_num_threads(joiner, std::cmp::min(num_cpus::get_physical(), 6))
    }

    fn num_worker_threads(&self) -> usize {
        self.num_threads - 1
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
                                    // TODO: use crossbeam_utils::sync::Parker
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

        // The maximum number of chunks ahead of the last fully-joined (in order) chunk
        // that we're willing to start processing of.
        let lookahead_num_chunks = self.num_worker_threads() * 10;

        let results_queue = crossbeam_queue::ArrayQueue::new(lookahead_num_chunks);
        let mut output_queue: VecDeque<Option<<JoinerT::Stage2 as parser::Stage2>::FinalReturnType>> = VecDeque::with_capacity(lookahead_num_chunks);
        let mut output_queue_start_index = 0;

        let is_eof = AtomicBool::new(false);
        let mut is_eof_local = false; // same as is_eof but local to the main thread

        let threads_result = crossbeam_utils::thread::scope(|scope| {
            // TODO: if we return early in the main loop, nothing ever sets
            // is_eof to true, which means the other threads never die.

            for thread_index in 0..self.num_worker_threads() {
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
            // TODO: because I/O are currently interleaved in one thread, if
            // input blocks then we may not see output in a timely fashion.
            // Split into two threads
            loop {
                // handle all outputs
                loop {
                    match results_queue.pop() {
                        None => {
                            // TODO: use crossbeam_utils::sync::Parker (once
                            // split into its own thread), or change
                            // results_queue to a crossbeam_channel::bounded
                            break;
                        },
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
                if !is_eof_local && output_queue.len() < output_queue.capacity() {
                    #[cfg(feature = "vtune")] let task = ittapi::Task::begin(&domain, "handle_input");
                    let mut work_unit_to_dispatch = None;
                    let mut just_reached_eof = false;
                    while work_unit_to_dispatch.is_none() {
                        match buf_reader.fill_buf() {
                            Err(e) => { return Err(Error::IOError(e.kind())); },
                            Ok(&[]) => {
                                just_reached_eof = true;
                                work_unit_to_dispatch = Some(std::mem::take(&mut next_work_unit));
                            }
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
                                        work_unit_to_dispatch = Some(std::mem::take(&mut next_work_unit));
                                        next_work_unit.reserve(CHUNK_SIZE * 2);
                                        next_work_unit.extend_from_slice(&buffer[split_index..]);
                                    },
                                    None => {
                                        next_work_unit.extend_from_slice(&buffer[..]);
                                    },
                                };
                                std::mem::drop(buffer);
                                buf_reader.consume(len);
                            },
                        }
                    }
                    let work_unit_to_dispatch = work_unit_to_dispatch.unwrap();
                    let work_unit_index = next_work_unit_index;
                    work_queue.push(WorkUnit { index: work_unit_index, buffer: work_unit_to_dispatch });
                    output_queue.push_back(None);

                    next_work_unit_index += 1;

                    if just_reached_eof {
                        is_eof.store(true, Ordering::Release);
                        is_eof_local = true;
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

impl<BufReadT: std::io::BufRead, JoinerT: Stage2Joiner> parser::StateI<BufReadT> for State<JoinerT> where JoinerT::Stage2 : Send, <JoinerT::Stage2 as parser::Stage2>::FinalReturnType : Send
{
    type FinalReturnType = JoinerT::FinalReturnType;
    fn process_streaming(&mut self, segment_index: parser::SegmentIndex, buf_reader: &mut BufReadT) -> Result<Self::FinalReturnType, Error> {
        match segment_index {
            parser::SegmentIndex::EntireFile => (),
            parser::SegmentIndex::Segment(_) => unimplemented!(),
        }
        State::process_streaming(self, buf_reader)
    }
}
