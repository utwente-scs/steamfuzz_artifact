use std::{
    iter, ops::{Range, RangeInclusive}, path::{Path, PathBuf}, rc::Rc, sync::atomic::Ordering, vec
};

use anyhow::{Context, Result};
use archive::{
    tar::{write_file, write_serialized},
    Archive, ArchiveBuilder,
};
use common::{
    config::{
        corpus::REPLACE_WITH_SHORTER_INPUT, fuzzer::{
            ARCHIVE_EARLY_WRITE, ARCHIVE_KEEP_SHORTER_INPUT, INSERT_MESSAGE_CHANCE, INTERVAL_MUTATION_CHANCE, NO_SPECIAL_MUTATION_CHANCE, DEFEAULT_INTERVAL_SIZE, ENABLE_BINARY_SEARCH_INTERRUPT_MINIMIZE, ENABLE_SANITY_CHECKS, LOG_FOR_VIEWER, ENABLE_SMART_INTERRUPT_MINIMIZE, MERGE_LEFTOVER_INTERVAL, MINIMIZE_INPUT_LENGTH, MESSAGE_WINDOW_INFERENCE, MINIMIZE_MUTATION_CHAIN, MUTATION_COUNT_POW2, MUTATION_MODE_DISTRIBUTION, MUTATION_MODE_MONO, MUTATION_MODE_SWITCH_CHANCE, MUTATION_STACKING, MUTATOR_DISTRIBUTION, RANDOM_CHANCE_INPUT, RANDOM_NO_VIABLE_MUTATION, REMOVE_UNREAD_VALUES, SNAPSHPOT_MUTATION_LIMIT, STREAM_RANDOM_DISTRIBUTION,
        }, mutation::MAX_RETRY, statistics::EXECUTIONS_HISTORY
    }, exit::{EXIT, signal_exit_point}, fs::decoder, random::{DeriveRandomSeed, FastRand}, time::epoch
};
use emulator::{Emulator, EmulatorSnapshot, ExecutionResult, RunMode, StopReason};
use enum_index::IndexEnum;
use modeling::{hardware::Input, input::{InputFile, StreamContext, stream::Stream, value::{InputValue, InputValueType}}};
use rand_distr::{Distribution, WeightedAliasIndex};
use crate::{
    corpus::{Corpus, CorpusResult, CorpusResultKind, InputInfo, InputResult, NewCoverage},
    corpus_archive::{IntoInputFileIter, write_input_file},
    dict::Dictionary, mutation::{Mutation, MutationContext, MutationLog, MutationMode, MutatorKind, Random},
    statistics::{Statistics, StatisticsInfo},
    stream::{ChronoStream, StreamIndex},
    stream_distribution::{StreamDistribution, StreamRandomDistribution}
};

macro_rules! log_for_viewer {
    ($($arg:tt)*) => {
        if LOG_FOR_VIEWER {
            println!($($arg)*);
        }
    };
}


pub struct Fuzzer {
    archive: ArchiveBuilder,
    emulator: Emulator<InputFile>,
    pre_fuzzing: EmulatorSnapshot,
    corpus: Corpus,
    statistics: Statistics,
    dictionary: Dictionary,
    snapshots: bool,

    seed: u64,

    distribution_mutator: WeightedAliasIndex<usize>,
    distribution_mutation_mode: WeightedAliasIndex<usize>,
    distribution_stream_select: WeightedAliasIndex<usize>,

    mutation_log: Vec<Rc<MutationLog>>,
    random: Option<Random>,
}


#[derive(Debug, Clone)]
pub enum InputFork {
    BaseFork {
        input: InputFile,
        chrono_stream: Rc<ChronoStream>,
    },
    ExecutedFork {
        result: InputResult,
        stream_distribution: Box<StreamDistribution>,
    },
}

impl InputFork {
    pub fn from_base(input: InputFile, chrono_stream: Rc<ChronoStream>) -> Self {
        InputFork::BaseFork {
            input,
            chrono_stream,
        }
    }

    pub fn from_result(result: InputResult) -> Self {
        InputFork::ExecutedFork {
            result,
            stream_distribution: Box::default(),
        }
    }

    pub fn into_inner(self) -> InputFile {
        match self {
            InputFork::BaseFork { input, .. } => input,
            InputFork::ExecutedFork { result, .. } => result.into_inner(),
        }
    }

    pub fn inner_ref(&self) -> (&InputFile, &ChronoStream) {
        match self {
            InputFork::BaseFork {
                input,
                chrono_stream,
            } => (input, chrono_stream),
            InputFork::ExecutedFork { result, .. } => (result.file(), result.chrono_stream()),
        }
    }

    pub fn inner_ref_mut(&mut self) -> (&mut InputFile, &ChronoStream) {
        match self {
            InputFork::BaseFork {
                ref mut input,
                chrono_stream,
            } => (input, chrono_stream),
            InputFork::ExecutedFork { result, .. } => result.inner_ref_mut(),
        }
    }

    pub fn file(&self) -> &InputFile {
        match self {
            InputFork::BaseFork { input, .. } => input,
            InputFork::ExecutedFork { result, .. } => result.file(),
        }
    }

    pub fn file_mut(&mut self) -> &mut InputFile {
        match self {
            InputFork::BaseFork { input, .. } => input,
            InputFork::ExecutedFork { result, .. } => result.file_mut(),
        }
    }

    pub fn chrono_stream(&self) -> &ChronoStream {
        match self {
            InputFork::BaseFork { chrono_stream, .. } => chrono_stream,
            InputFork::ExecutedFork { result, .. } => result.chrono_stream(),
        }
    }
}

impl Fuzzer {
    pub fn new(
        name: String,
        seed: Option<u64>,
        import_corpus: Vec<PathBuf>,
        statistics: bool,
        snapshots: bool,
        archive: ArchiveBuilder,
        mut emulator: Emulator<InputFile>,
    ) -> Result<Self> {
        // pre-fuzzer snapshot
        let pre_fuzzing = emulator
            .snapshot_create()
            .context("Failed to create pre-fuzzer snapshot")?;

        // set seed
        let seed = seed.unwrap_or_else(|| {
            // "random" seed (based on time + thread id)
            fastrand::u64(..)
        });
        fastrand::seed(seed);
        log::debug!("initial random seed = {:#x?}", seed);

        // collect dictionary
        let mut dictionary = Dictionary::default();
        for memory_block in emulator.memory_blocks().filter(|mem| mem.readonly) {
            dictionary.scan_memory_block(memory_block.data);
        }

        // create fuzzer
        let mut fuzzer = Self {
            archive,
            emulator,
            pre_fuzzing,
            corpus: Corpus::new(),
            statistics: Statistics::new(name, statistics),
            dictionary,
            snapshots,
            seed,
            distribution_mutator: WeightedAliasIndex::new(MUTATOR_DISTRIBUTION.to_vec())
                .context("Failed to create a weighted mutator distribution.")?,
            distribution_mutation_mode: WeightedAliasIndex::new(
                MUTATION_MODE_DISTRIBUTION.to_vec(),
            )
            .context("Failed to create a weighted mutation mode distribution.")?,
            distribution_stream_select: WeightedAliasIndex::new(
                STREAM_RANDOM_DISTRIBUTION.to_vec(),
            )
            .context("Failed to create a weighted stream select distribution.")?,
            mutation_log: vec![],
            random: None,
        };

        // verify config distribution count matches enum len
        assert_eq!(MUTATOR_DISTRIBUTION.len(), MutatorKind::VARIANT_COUNT);
        assert_eq!(
            MUTATION_MODE_DISTRIBUTION.len(),
            MutationMode::VARIANT_COUNT
        );
        assert_eq!(
            STREAM_RANDOM_DISTRIBUTION.len(),
            StreamRandomDistribution::VARIANT_COUNT
        );

        // write fuzzer config to corpus
        fuzzer.write_config()?;

        // load and run input files from old corpus
        if !import_corpus.is_empty() {
            log::info!("Re-run existing corpus ...");
            for corpus in import_corpus {
                if let Err(err) = fuzzer.load_corpus(&corpus) {
                    log::error!("Failed to load corpus: {:?}", err);
                }
            }
        }

        // add empty input if corpus is empty
        if fuzzer.corpus.is_empty() {
            fuzzer.run_fuzzer_input(InputFile::default(), &fuzzer.pre_fuzzing.clone(), false)?;
        }

        Ok(fuzzer)
    }

    fn load_corpus(&mut self, corpus: &Path) -> Result<()> {
        // load input files
        log::info!("Loading corpus archive {:?} ...", corpus);
        let mut corpus_archive = Archive::from_reader(decoder(corpus)?);

        // run inputs
        for result in corpus_archive.iter()?.input_files() {
            signal_exit_point()?;

            // get next input file
            let entry = match result {
                Ok(input) => input,
                Err(err) => {
                    log::error!("Failed to parse entry: {:?}", err);
                    continue;
                }
            };

            // original input id
            let mut input = entry.input;
            let id = input.id();

            // set new input id
            input.replace_id(&InputFile::default());

            // run input
            log::info!("Running input {} ...", id);
            self.run_fuzzer_input(input, &self.pre_fuzzing.clone(), true)?;
        }

        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        if self.snapshots {
            self.run_snapshot_fuzzer()?;
        } else {
            self.run_plain_fuzzer()?;
        }

        if !ARCHIVE_EARLY_WRITE {
            self.write_input_files()?;
        }

        self.write_statistics()
    }

    fn run_plain_fuzzer(&mut self) -> Result<()> {
        log::info!("Started plain fuzzing...");
        while !EXIT.load(Ordering::Relaxed) {

            // log::info!("##########################################################");
            // log::info!("New input starting now!");
            // log::info!("##########################################################");

            // log::info!("Lookint to fetch {:?}", input_src);

            // random input for mutation
            let input = self
                .next_input()
                .context("Failed to get random input.")?;

            let input = input.fork();


            // log::info!("Still there? {}", input.clone().into_inner());


            self.run_mutations(input, None, &self.pre_fuzzing.clone())?;
        }

        Ok(())
    }

    fn run_snapshot_fuzzer(&mut self) -> Result<()> {
        if MINIMIZE_MUTATION_CHAIN {
            // TODO: add support:
            // - pass base_input into minimize_mutation_chain
            // - set cursor when base_input is some
            anyhow::bail!("Snapshot fuzzer doesn't support MINIMIZE_MUTATION_CHAIN");
        }

        log::info!("Started snapshot fuzzing...");
        while !EXIT.load(Ordering::Relaxed) {
            // get random base input
            let base_info = self.next_input().context("Failed to get random input.")?;
            let mut base_input = base_info.result().file().clone();
            base_input.set_read_limit(fastrand::usize(0..=base_input.len()));

            // emulator counts before execution
            let counts = EXECUTIONS_HISTORY.then(|| self.emulator.counts());

            // run base input
            let base_result = self
                .emulator
                .run(base_input, RunMode::Normal)
                .context("run emulator")?;

            // track emulator counts
            if let Some(base) = counts {
                self.statistics
                    .process_counts(base_result.counts.clone() - base);
            }

            // base input
            let base_input = InputResult::from(base_result).as_fork();
            log::trace!("base_input: {}", base_input.file());

            // emulator snapshot (between input parts)
            let snapshot = self
                .emulator
                .snapshot_create()
                .context("Failed to create emulator snapshpot")?;

            for _ in 0..SNAPSHPOT_MUTATION_LIMIT {
                if self.run_mutations(base_input.clone(), Some(base_input.file()), &snapshot)? {
                    break;
                }
            }

            // restore emulator
            self.emulator.snapshot_restore(&self.pre_fuzzing);
        }

        Ok(())
    }

    fn run_mutations(
        &mut self,
        mut input: InputFork,
        base_input: Option<&InputFile>,
        snapshot: &EmulatorSnapshot,
    ) -> Result<bool> {
        log::debug!(
            "mutate new input forked from base input {:?}",
            input.file().parent()
        );

        let mutation_stack = 1 << fastrand::usize(MUTATION_COUNT_POW2);

        // random seed based on fuzzer seed and input id
        let random_seed = self.seed.derive(&input.file().id());
        input.file_mut().set_random_seed(random_seed);

        fn get_first_mutation() -> Option<MutatorKind> {
            let option_range= INSERT_MESSAGE_CHANCE + INTERVAL_MUTATION_CHANCE + NO_SPECIAL_MUTATION_CHANCE;
            let rand = fastrand::u8(0..=option_range);
            if rand < INSERT_MESSAGE_CHANCE {
                Some(MutatorKind::InsertMessageWindow)
            } else if rand < INSERT_MESSAGE_CHANCE + INTERVAL_MUTATION_CHANCE {
                Some(MutatorKind::InterruptInterval)
            }
            else {
                None
            }

        }

        // log::info!("Sanity check: mutating input from {:?}", input_src);

        for i in 0..mutation_stack {
            // let force_stream_concat_mut = *input_src != InputSource::CoverageCorpus
            //     && i < std::cmp::min(mutation_stack / 2, 8);
            let force_mutator =
                if i == 0 {
                    get_first_mutation()
                } else {
                    None
                };


            let mut last_mutation = (i + 1) == mutation_stack;

            // add one input mutation
            // log::info!("force mutator {:?}", force_mutator);
            let mutated = self.mutate(&mut input, force_mutator).context("mutate input file")?;

            // add random mutation after last mutation with 1/4 chance or when no viable mutation was found
            let force_random = RANDOM_NO_VIABLE_MUTATION && !mutated;
            let input_random = last_mutation
                && RANDOM_CHANCE_INPUT
                    .map(|chance| fastrand::u8(0..chance) == 0)
                    .unwrap_or(false);
            let random = if force_random || input_random {
                last_mutation = true;
                self.add_random_count(&mut input)
            } else {
                false
            };

            // skip execution if neither successful mutated nor random count added
            if !mutated && !random {
                break;
            }

            // execute input:
            // - !MUTATION_STACKING: after each mutation (libfuzzer like)
            // - MUTATION_STACKING: after last mutation (afl like)
            if !MUTATION_STACKING || last_mutation {
                let file = input.into_inner();
                assert!(file.random_seed().is_some());
                // log::info!("Running mutated input {}", file);
                // log::info!("Running after applying mutations {:?} seed: {:?} random: {:?}", file.id(), file.random_seed(), file.random_count());
                if let Some(result) = self.run_fuzzer_input(file, snapshot, false)? {
                    // no new coverage found => continue mutating input
                    input = result.as_fork();

                    // set/reset cursor
                    match base_input {
                        Some(base) => input.file_mut().set_cursor(base),
                        None => input.file_mut().reset_cursor(),
                    }
                } else {
                    // new coverage found => input was added to corpus
                    // or emulator exit
                    return Ok(true);
                }
            }

            // make sure we exit after last viable mutation was run
            // this can be early when only the random mutation was viable (empty input)
            if last_mutation {
                break;
            }
        }

        Ok(false)
    }

    fn run_fuzzer_input(
        &mut self,
        mut input: InputFile,
        snapshot: &EmulatorSnapshot,
        import: bool,
    ) -> Result<Option<InputResult>> {

        // log::info!("running input");

        // restore emulator
        self.emulator.snapshot_restore(snapshot);


        // emulator counts before execution
        let counts: Option<emulator::EmulatorCounts> = EXECUTIONS_HISTORY.then(|| self.emulator.counts());

        input.reset_interrupt_context();

        // run input
        let result = self
            .emulator
            .run(input, RunMode::Leaf)
            .context("run emulator")?;

        if import {
            log::info!("Result: {}", result);
        }

        if result.stop_reason == StopReason::UserExitRequest {
            return Ok(None);
        }

        // track emulator counts
        if let Some(base) = counts {
            self.statistics.process_counts(result.counts.clone() - base);
        }

        // process results
        let result = self
            .process_result(result, import)
            .context("Process execution result")?;

        // // restore emulator
        // self.emulator.snapshot_restore(snapshot);

        Ok(result)
    }

    fn run_minimized_input(&mut self, mut input: InputFile) -> Result<CorpusResult> {
        // log::info!("Running minimized input: {}", input);
        // restore emulator
        self.emulator.snapshot_restore(&self.pre_fuzzing);

        // log::info!("running input minimized");
        input.reset_interrupt_context();
        assert!(input.read_count() == 0);

        // emulator counts before execution
        let counts = EXECUTIONS_HISTORY.then(|| self.emulator.counts());

        // run input
        let result = self
            .emulator
            .run(input, RunMode::Leaf)
            .context("run emulator")?;

        // track emulator counts
        if let Some(base) = counts {
            self.statistics.process_counts(result.counts.clone() - base);
        }
        // assert!(matches!(result.stop_reason, StopReason::LimitReached(_)) || matches!(result.stop_reason, StopReason::EndOfInput), "Stopreason was {:?}", result.stop_reason);

        // process results
        let input_result = InputResult::new(
            result.hardware.input,
            epoch()?,
            result.counts.basic_block(),
            result.stop_reason,
            result.hardware.access_log,
        );

        self.statistics.process_minimization();
        self.corpus.process_result(
            input_result,
            self.emulator.get_coverage_bitmap(),
            self.mutation_log
                .iter()
                .map(|log| &log.mutation.target().context),
            false,
        )
    }

    pub fn next_input(&mut self) -> Result<&InputInfo> {
        self.mutation_log.clear();
        self.random = None;
        self.corpus.random_input()
    }

    pub fn mutate(&mut self, input: &mut InputFork, force_mutator: Option<MutatorKind>) -> Result<bool> {
        // can't mutate empty input with no streams
        if input.file().input_streams().is_empty() {
            return Ok(false);
        }

        // next mutation context (stream / mono)
        let mutation_context = if MUTATION_MODE_MONO {
            self.next_mutation_context()
        } else {
            MutationContext::Stream(self.next_stream_random_distribution())
        };

        // mutate input stream
        if let Some(mutation) = self.mutate_stream(input, mutation_context, force_mutator)? {
            self.mutation_log.push(Rc::new(mutation));
            return Ok(true);
        }

        log::debug!(
            "no viable mutation was found for input {} (parent {:?}), this should happen very rarely",
            input.file().id(),
            input.file().parent()
        );
        Ok(false)
    }

    fn next_mutation_context(&mut self) -> MutationContext {
        // mutations depending on last stream mutation target
        let last_target = self
            .mutation_log
            .last()
            .map(|log| (&log.mode, log.mutation.target()));

        match last_target {
            Some((old_mode, last_target)) => {
                // switch mode with a 1/8 chance
                let new_mode = if fastrand::u8(0..MUTATION_MODE_SWITCH_CHANCE) == 0 {
                    MutationMode::index_enum(self.distribution_mutation_mode.sample(&mut FastRand))
                        .expect("MutationMode index is valid")
                } else {
                    *old_mode
                };

                match new_mode {
                    MutationMode::Stream => {
                        MutationContext::Stream(self.next_stream_random_distribution())
                    }
                    MutationMode::Mono => MutationContext::Mono {
                        context: last_target.context.clone(),
                    },
                }
            }
            // no previous target => fallback to stream
            None => MutationContext::Stream(self.next_stream_random_distribution()),
        }
    }

    fn add_random_count(&mut self, input: &mut InputFork) -> bool {
        let random = Random::new();

        if random.is_valid_and_effective(input.file()) {
            random.mutate(input.file_mut());
            self.random = Some(random);
            true
        } else {
            false
        }
    }

    fn mutate_stream(
        &mut self,
        input: &mut InputFork,
        context: MutationContext,
        force_mutator: Option<MutatorKind>,
    ) -> Result<Option<MutationLog>> {
        for _ in 0..MAX_RETRY {
            if let Some(target) = self.mutation_target(input, &context)? {
                // log::info!("mut stream {:?}", target);
                // if target.context.value_type() == InputValueType::Choice(1) {
                //     continue;
                // }
                if let Some(mutation) = self.random_mutation(input, target, force_mutator)? {
                    // log::info!("selected mutation {:?}", mutation);

                    let effective = mutation.apply(input)?;

                    if effective {
                        return Ok(Some(MutationLog {
                            mode: MutationMode::from(&context),
                            mutation,
                            distribution: context.distribution(),
                        }));
                    }
                }
            }
        }

        Ok(None)
    }

    fn mutation_target(
        &mut self,
        input: &mut InputFork,
        context: &MutationContext,
    ) -> Result<Option<StreamIndex>> {
        let context = match context {
            MutationContext::Stream(distribution) => match input {
                InputFork::BaseFork { input, .. } => input
                    .parent()
                    .and_then(|parent| self.corpus.random_stream_index(parent, *distribution))
                    .context("InputFork missing parent stream distribution")?,
                InputFork::ExecutedFork {
                    result,
                    stream_distribution,
                } => {
                    let info = result
                        .file()
                        .parent()
                        .and_then(|parent| self.corpus.input(parent))
                        .context("InputFork missing parent")?;

                    stream_distribution
                        .random_stream_index(info.stream_info(), result, *distribution)
                        .context("Random stream index pick failed for executed fork input")
                }
            }
            .context("Failed to pick random input stream")?
            .clone(),
            MutationContext::Mono { context } => context.clone(),
        };

        if let InputValueType::InterruptChoice(0) = context.value_type() {
            return Ok(None)
        }

        let is = input
            .file()
            .input_streams()
            .get(&context)
            .context("Failed to get input stream.")?;

        Ok(Some(StreamIndex {
            context,
            index: if is.len() > is.cursor() {
                fastrand::usize(is.cursor()..is.len())
            } else {
                is.cursor()
            },
        }))
    }

    fn random_mutation(
        &self, input: &InputFork,
        target: StreamIndex,
        force_mutator: Option<MutatorKind>
    ) -> Result<Option<Mutation>> {
        let mutator = force_mutator.unwrap_or_else(|| self.next_mutator());
        // log::info!("mutator {:?}", mutator);
        Mutation::create(mutator, target, input, &self.dictionary, || {
            self.corpus.random_input().map(|info| info.result()).ok()
        })
        .with_context(|| format!("Failed to create {mutator:?} mutator"))
    }

    fn sanity_check_minimization(&mut self, info: &NewCoverage, check_nr: usize) {
        let mut satis_input = info.result().file().clone();
        // satis_input.set_read_limit(satis_input.read_count());
        satis_input.remove_read_limit();
        // satis_input.read_limit = None;
        satis_input.remove_random_count();
        // if self.random.is_some() {
        // satis_input.set_random_count(0);
        // } else {
        //     satis_input.remove_random_count();
        // }
        satis_input.remove_random_seed();
        satis_input.reset_cursor();
        log::info!("Startying check {}. read limit {:?} seed {:?} random {:?}", check_nr, satis_input.read_limit, satis_input.random_seed(), satis_input.random_count());
        // log::info!("San check 3. Running inp {}", satis_input);
        match self.run_minimized_input(satis_input).expect("running minimized sancheck 3") {
            CorpusResult::NewCoverage(minimized) => {
                // log::info!("inp {}", minimized.result().file());
                if !verify_minimization(&minimized, &info) {
                    log::info!("Sanity check {} failed! NewCov but not verified. Target", check_nr);
                    log::info!("San check {}. Got new coverage. covfeats ({}) {:?} {:?}",check_nr, minimized.uniq_features().len(),  minimized.uniq_features(), minimized.result().stop_reason());
                    log::info!("San check {}. Should at least contain. covfeats ({}) {:?} ", check_nr, info.uniq_features().len(), info.uniq_features());
                    log::info!("i started with len {} {:?} {}", info.result().read_count(), info.result().stop_reason(), info.result().file());
                    log::info!("ended with len {} {:?} {}", minimized.result().read_count(), info.result().stop_reason(), minimized.result().file());
                } else {
                    // log::info!("Sanity check 3 passed");
                }
            }
            CorpusResult::ShorterInput(_result) => {
                log::info!("Sanity check {} failed! ShorterInput", check_nr);
            },
            CorpusResult::Uninteresting(minimized) =>  {
                log::info!("Sanity check {} failed! Uninteresting", check_nr);
                log::info!("San check {}. Should at least contain. covfeats {:?} {:?} ", check_nr, info.uniq_features(), info.result().stop_reason());
                log::info!("i started with len {} {}", info.result().read_count(), info.result().file());
                log::info!("ended with len {} {}", minimized.result.read_count(), minimized.result.file());
             },
        }
    }



    fn minimize_and_store(
        &mut self,
        mut info: NewCoverage,
        import: bool,
        statistics_info: &mut Option<StatisticsInfo>,
        ) -> Result<Option<InputResult>> {

        if REMOVE_UNREAD_VALUES {
            info.result_mut().file_mut().remove_unread_values();
            info.result_mut().file_mut().remove_empty_streams();
        }

        if info.result().category().schedule() {
            // sanity check 3
            if ENABLE_SANITY_CHECKS {
                log::info!("adding input to corpus {} cat {:?}", info.result().file(), info.result().category());
                self.sanity_check_minimization(&info, 3);
            }

            let mut start_uniq_features = None;
            if ENABLE_SANITY_CHECKS {
                start_uniq_features = Some(info.uniq_features().clone());
            }

            // remove uneffective mutations
            // skip mutation chain minimizations for imported inputs
            if !import && MINIMIZE_MUTATION_CHAIN {
                info = self
                    .minimize_mutation_chain(info)
                    .context("minimize mutations")?;
            }
            // log::info!("Minimizing 1. Inputs: {}", info.result().file());
            // if target_corpus == TargetCorpus::Coverage {
            //     log::info!("Curr uniq2. len: {}. covs {:?}", info.uniq_features().len(), info.uniq_features());
            // }


            // sanity check 4
            if ENABLE_SANITY_CHECKS {
                self.sanity_check_minimization(&info, 4);
            }


            // trim end of input file (binary search for uneffective input values)
            if MINIMIZE_INPUT_LENGTH {
                // log::info!("Before min {}", info.result().file().len());
                let (new_info, read_limit) = self
                    .minimize_input_length(info)
                    .context("minimize input length")?;
                info = new_info;
                if let Some(statistics_info) = statistics_info {
                    statistics_info.read_limit = read_limit;
                }
                // log::info!("minimized len to {:?} (file {})", read_limit, info.result().file().len());
            }
            // if target_corpus == TargetCorpus::Coverage {
            //     log::info!("Curr uniq3. len: {}. covs {:?}", info.uniq_features().len(), info.uniq_features());
            //     log::info!("Minimizing 2. Inputs: {}", info.result().file());
            // }
            // sanity check 4.5
            if ENABLE_SANITY_CHECKS {
                self.sanity_check_minimization(&info, 45);
            }


            // trim end of input file (unread input values)
            if REMOVE_UNREAD_VALUES {
                info.result_mut().file_mut().remove_unread_values();
            }


            if let Some(statistics_info) = statistics_info {
                if let Some(read_limit) = statistics_info.read_limit {
                    if read_limit != info.result().file().len() {
                        log::info!("failed read limit {} != file len {} (chrono len {}) {:?}", read_limit, info.result().file().len(), info.result().chrono_stream().len(), info.result().stop_reason());
                    }
                }
            }
            // log::info!("Minimizing 3. Inputs: {}", info.result().file());
            if ENABLE_SANITY_CHECKS {
                self.sanity_check_minimization(&info, 5);
            }



            // log::info!("Minimizing 4. Inputs: {}", info.result().file());




            // remove empty input streams
            info.result_mut().file_mut().remove_empty_streams();
            // log::info!("Minimizing 5. Inputs: {}", info.result().file());


            if ENABLE_SANITY_CHECKS {
                self.sanity_check_minimization(&info, 6);
            }

            if ENABLE_SANITY_CHECKS {
                log::info!("Curr uniq6. len: {}. covs {:?}", info.uniq_features().len(), info.uniq_features());
            }

            // if !import && MINIMIZE_INTERRUPT_INTERVAL {
            //     info = self.minimize_interrupt_intervals(info, &target_corpus).context("minimize interrupts")?;
            // }
            if !import && MESSAGE_WINDOW_INFERENCE {
                info = self.merge_and_minimize_message_windows(info)?;

                // trim end of input file (unread input values)
                if REMOVE_UNREAD_VALUES {
                    info.result_mut().file_mut().remove_unread_values();
                }
            }


            if ENABLE_SANITY_CHECKS  {
                log::info!("Curr uniq7. len: {}. covs {:?}", info.uniq_features().len(), info.uniq_features());
            }
            
            if ENABLE_SANITY_CHECKS {
                self.sanity_check_minimization(&info, 7);
                if let Some(start_uniq_features) = start_uniq_features {
                    if !info.uniq_features().is_superset(&start_uniq_features) {
                        log::info!("failed! After minimization, not the same uniq features! Before {:?}, after {:?}", info.uniq_features(), start_uniq_features);
                    }
                } else {unreachable!()}
            }
        }
        
        // update statistics info
        if let Some(statistics_info) = statistics_info {
            statistics_info.update_input(info.result());
        }

        // write to corpus archive
        if ARCHIVE_EARLY_WRITE || !info.result().category().schedule() {
            write_input_file(&mut self.archive.borrow_mut(), info.result())?;
        }


        // add to corpus
        self.corpus
            .add_result(info)
            .context("Add result to corpus")?;


        Ok(None)
    }

    fn process_result(
        &mut self,
        result: ExecutionResult<InputFile>,
        import: bool,
    ) -> Result<Option<InputResult>> {
        let input = &result.hardware.input;
        let mut statistics_info = self.statistics.enabled().then(|| {
            StatisticsInfo::from_input(input, result.stop_reason.clone(), self.mutation_log.len())
        });

        // log::info!("Processing result for input {} ...", input.id());

        // log::info!("Start of process result (fuzzers) {} {:?}", input, result.stop_reason);

        let corpus_result = self.corpus.process_result(
            InputResult::new(
                result.hardware.input,
                epoch()?,
                result.counts.basic_block(),
                result.stop_reason,
                result.hardware.access_log,
            ),
            self.emulator.get_coverage_bitmap(),
            self.mutation_log
                .iter()
                .map(|log| &log.mutation.target().context),
            true,
        )?;
        let corpus_result_kind = CorpusResultKind::from(&corpus_result);
        let input_result = match corpus_result {
            CorpusResult::NewCoverage(info) => {
                self.minimize_and_store(info, import, &mut statistics_info)
                .expect("Minimize and store cov result");

                None
            }
            CorpusResult::ShorterInput(result) => {
                // log::info!("Shorter input From process_result");
                let (result, replaced) = self.shorter_input(result.into_inner())?;

                if replaced {
                    // update statistics info
                    if let Some(statistics_info) = &mut statistics_info {
                        statistics_info.input_id =
                            result.file().parent().context("missing parent input id")?;
                        statistics_info.update_input(&result);
                    }
                }

                Some(result)
            }
            CorpusResult::Uninteresting(result) => Some(result.into_inner()),
        };

        if !import {
            self.corpus.update()?;
        }
        self.statistics
            .process_result(statistics_info, corpus_result_kind, &self.corpus)?;

        Ok(input_result)
    }

    fn shorter_input(&mut self, mut result: InputResult) -> Result<(InputResult,bool)> {
        // log::info!("get pre 2. Processing shorter input {}, readlimit {:?}", result.file(), result.file().read_limit);
        // find base input (parent)

        if REMOVE_UNREAD_VALUES {
            result.file_mut().remove_unread_values();
        }
        // log::info!("Oh no.... {} ", result.file());

        result.file_mut().remove_empty_streams();

        // replace shorter input
        if REPLACE_WITH_SHORTER_INPUT {
            // write to corpus archive
            if ARCHIVE_KEEP_SHORTER_INPUT {
                write_input_file(&mut self.archive.borrow_mut(), &result)?;
            }

            // update corpus file
            self.corpus
                .replace_input(result.clone())
                .context("Replace with shorter input")?;

        }

        Ok((result, true))
    }

    fn minimize_mutation_chain(&mut self, info: NewCoverage) -> Result<NewCoverage> {
        // can't minimize with only one mutation
        if self.mutation_log.len() <= 1 {
            return Ok(info);
        }

        // get base input (input without mutations)
        let new_input = info.result().file().clone();
        let parent_input = match new_input.parent() {
            Some(parent_id) => self
                .corpus
                .input(parent_id)
                .context("Missing parent input in corpus")?,

            // can't minimize without base input
            None => {
                log::info!("oopsie!");
                return Ok(info)
            },
        };
        // log::info!("Base input for minimization: {:?}", parent_input);
        let random_seed = new_input.random_seed().expect("get random seed"); //self.seed.derive(&new_input.id());
        let mut base_input = parent_input.fork();
        // log::info!("Minimzing mutation chain with parent {}", base_input.file());
        base_input.file_mut().reset_cursor(); // TODO: set base_input cursor
        base_input.file_mut().replace_id(&new_input);
        base_input.file_mut().set_random_seed(random_seed);
        base_input.file_mut().remove_random_count();

        // keep complete input info in case no minimize is possible
        let mut minimized = info;

        // mutations
        let mut random = self.random.clone();
        let random_mutation = usize::from(random.is_some());
        let mut mutation_log = self.mutation_log.clone();
        let mut idx = mutation_log.len() - 1 + random_mutation;

        // keep removing unneeded mutations
        loop {
            let mut input = base_input.clone();
            let mut removed = vec![];
            let removed_random;

            // apply all mutations except one
            for (i, log) in mutation_log.iter().enumerate() {
                // skip mutation at idx
                if i == idx {
                    removed.push(i);
                    continue;
                }

                // apply mutation
                if !log.mutation.apply(&mut input)? {
                    // remove invalid / uneffective mutations
                    removed.push(i);
                }
            }

            // treat random mutation as last mutation (if any)
            match &random {
                Some(_random) if idx == mutation_log.len() => {
                    // apply random mutation
                    removed_random = true;
                }
                Some(random) => {
                    random.mutate(input.file_mut());
                    removed_random = false;
                }
                _ => {
                    removed_random = false
                }
            }

            // log::info!("Running mutation minimization {}", input.file());
            // run input
            match self.run_minimized_input(input.into_inner())? {
                CorpusResult::NewCoverage(info) => {
                    if verify_minimization(&info, &minimized) {
                        // remove mutations in reverse order (so index is valid)
                        for index in removed.into_iter().rev() {
                            log::debug!("found unneeded mutation: {:x?}", mutation_log[index]);
                            mutation_log.remove(index);
                            if index < idx {idx -= 1}
                        }

                        // remove random
                        if removed_random {
                            random = None;
                        }

                        // only the relevant findings
                        // info.remove_irrelevant_features(&mut minimized, &target_corpus);
                        // log::info!("found mutation minimization {:?}", random);
                        // update minimal input info
                        minimized = info;

                    }
                }
                CorpusResult::ShorterInput(result) => {
                    // log::info!("Short input found in minimize mutation chain");
                    let _ = self.shorter_input(result.into_inner())?;
                }
                CorpusResult::Uninteresting(_) => {}
            }

            if idx > 0 {
                // next mutation
                idx -= 1;
            } else {
                // last mutation => stop
                break;
            }
        }

        // update mutation logs
        self.mutation_log = mutation_log;
        log::info!("Found new input as result of the following mutations {:?}", self.mutation_log.iter().map(|v| v.mutation.mutator.clone().into()).collect::<Vec<MutatorKind>>());

        Ok(minimized)
    }

    fn minimize_input_length(&mut self, info: NewCoverage) -> Result<(NewCoverage, Option<usize>)> {
        // minimize input length
        // log::info!("Strat minimize input length with {}", info.result().file());
        let mut satis_input = info.result().file().clone();
        let mut result_info = info;
        let starting_len = satis_input.read_count();
        let mut left = 0;
        let mut right = starting_len;

        // log::info!("minimizing input length from {} bytes ...", right);
        // log::info!("starting with {}", satis_input);
        while left < right {
            let read_limit = left + (right - left) / 2;
            // log::info!("Read limit {}", read_limit);

            satis_input.reset_cursor();
            satis_input.set_read_limit(read_limit);
            match self.run_minimized_input(satis_input.clone())? {
                CorpusResult::NewCoverage(minimized) => {
                    if verify_minimization(&minimized, &result_info) {
                        // log::info!("found shorter input with read_limit = {}. cursor len {}. {:?}", read_limit, minimized.result().file().read_count(), minimized.result().stop_reason());

                        // log::info!("read limit {} works {}", read_limit, minimized.result().file());
                        // if minimized.result().file().values_read() != read_limit {
                        //     log::info!("failed: minimized values read {} != readlimit {}", minimized.result().file().values_read(), read_limit);
                        // }

                        // update right bound
                        satis_input = minimized.result().file().clone();
                        result_info = minimized;


                        // log::info!("Am I the same {}", result_info.result().file());
                        right = read_limit;

                        continue;
                    }
                }
                CorpusResult::ShorterInput(result) => {
                    // log::info!("Shorter input from minimize input length");
                    self.shorter_input(result.into_inner().clone())?; ()
                },
                CorpusResult::Uninteresting(_) => (),
            }


            // update left bound
            left = read_limit + 1;
        }
        debug_assert_eq!(left, right);

        let read_limit = if right < starting_len {
            // log::info!("found shorter input with read_limit = {}, (down from {})", right, starting_len);
            Some(right)
        } else {
            None
        };


        Ok((result_info, read_limit))
    }





    fn merge_and_minimize_message_windows(&mut self, info: NewCoverage) -> Result<NewCoverage> {
        // Default starting interval value if interval is None
        log_for_viewer!("Start merge and minimize");

        // starting point
        let mut result_info = info;
        let mut input = result_info.result().file().clone();
        input.remove_random_count();
        input.remove_random_seed();

        let mut chrono_stream = result_info.result().chrono_stream().clone();
        let mut interrupt_windows = chrono_stream.find_irq_chrono_windows();

        let mut at_last_message: bool = true;

        if interrupt_windows.len() == 0 {
            return Ok(result_info);
        }

        fn make_test_input(input: &InputFile) -> InputFile {
            let mut test_input = input.clone();
            // test_input.set_read_limit(test_input.len());
            test_input.reset_cursor();
            test_input.remove_random_seed();
            test_input.remove_random_count();
            test_input.remove_read_limit();
            test_input
        }

        fn create_head_file(s: &mut Fuzzer, input: &InputFile, interrupt_windows: &Vec<(usize, usize)>,
            chrono_stream: &ChronoStream, irq_idx: usize, min_expected_irqs: usize) -> Option<InputFile> {
            let mut test_input_removal = make_test_input(&input);
            // remove all the remaining interrupts
            shrink_or_remove_irqs(&mut test_input_removal, &interrupt_windows[irq_idx..], chrono_stream, true);
            test_input_removal.set_read_limit(test_input_removal.len());
            assert!(test_input_removal.random_count().is_none());
            assert!(test_input_removal.random_seed().is_none());
            // log::info!("test_input after remove tail irqs (read limit {:?}): {}", test_input_removal.read_limit, test_input_removal);

            let mut head_file = match s.run_minimized_input(test_input_removal).expect("run removal input") {
                CorpusResult::NewCoverage(minimized) => {
                    assert!(minimized.result().read_count() == minimized.result().file().read_count());
                    // log::info!("newcov {:?} {}", minimized.result().stop_reason(), minimized.result().basic_blocks());
                    // if verify_interval(&minimized, stream_index, mid) {
                    let res = minimized.into_inner();
                    

                    if res.chrono_stream().find_irq_chrono_indices().len() < min_expected_irqs {
                        // execution failed, rare qemu bug
                        log::info!("HERE {} < {}", res.chrono_stream().find_irq_chrono_indices().len(), min_expected_irqs);
                        log::info!("input after run {}", res.file());
                        return None
                    }
                    res.into_inner()

                    // } else {
                        // return None
                    // }
                },
                CorpusResult::ShorterInput(minimized) | CorpusResult::Uninteresting(minimized) => {
                    assert!(minimized.result.read_count() == minimized.result.file().read_count());
                    // log::info!("shorter/unint {:?} {}", minimized.result.stop_reason(), minimized.result.basic_blocks());
                    // if verify_interval_known(&minimized, stream_index, mid) {
                    let res = minimized.into_inner();
                    if res.chrono_stream().find_irq_chrono_indices().len() < min_expected_irqs {
                        // execution failed, rare qemu bug
                        log::info!("HERE {} < {}", res.chrono_stream().find_irq_chrono_indices().len(), min_expected_irqs);
                        log::info!("input after run {}", res.file());
                        return None
                    }
                    res.into_inner()

                },
            };
            // log::info!("input after run {}", head_file);
            head_file.remove_unread_values();
            // log::info!("head file {:?}", head_file);
            Some(head_file)
        }

        fn get_interval_mut<'a>(input: &'a mut InputFile, stream_index: &StreamIndex) -> &'a mut Option<u32> {
            if let Some(stream) = input.input_streams_mut().get_mut(&stream_index.context) {
                if let Some(modeling::input::value::InputValue::InterruptChoice {
                    interval: ref mut int,
                    ..
                }) = stream.as_mut().get_mut(stream_index.index)
                {
                    int
                }
                else { unreachable!("not interruptchoice"); }
            } else { unreachable!("none")}
        }

        fn get_interval(input: &InputFile, stream_index: &StreamIndex) -> Option<u32> {
            if let Some(stream) = input.input_streams().get(&stream_index.context) {
                if let Some(modeling::input::value::InputValue::InterruptChoice {
                    interval: ref int,
                    ..
                }) = stream.as_ref().get(stream_index.index)
                {
                    *int
                }
                else { unreachable!("not interruptchoice");  } //unreachable!("not interruptchoice"); 
            } else { unreachable!("none")}
        }

        // fn sum_intervals(test_input: &InputFile, interrupt_windows: &[(usize, usize)], chrono_stream: &ChronoStream) -> u16 {
        //     let mut sum = 0;
        //     for (start_idx, _end_idx) in interrupt_windows.iter().rev() {
        //         let stream_index = &chrono_stream.chrono_stream[*start_idx];
        //         assert!(matches!(stream_index.context.value_type(), InputValueType::InterruptChoice(_)), "Not interrupt choice");
        //         let stream = test_input.input_streams().get(&stream_index.context).expect("get stream");

        //         if let InputValue::InterruptChoice {interval , ..} = 
        //             stream.as_ref()[stream_index.index] 
        //         {
        //             if let Some(interval) = interval {
        //                 if interval > MERGE_LEFTOVER_INTERVAL {
        //                     sum += interval - MERGE_LEFTOVER_INTERVAL;
        //                 }
        //             } else {
        //                 sum += DEFEAULT_INTERVAL_SIZE - MERGE_LEFTOVER_INTERVAL;
        //             }
        //         } else { unreachable!("not interrupt choice"); }

        //     }
        //     sum
        // }


        fn shrink_or_remove_irqs(test_input: &mut InputFile, interrupt_windows: &[(usize, usize)], chrono_stream: &ChronoStream, remove: bool) -> u32 {
            
            let mut removed_sum = 0;

            for (start_idx, _end_idx) in interrupt_windows.iter().rev() {
                let stream_index = &chrono_stream.chrono_stream[*start_idx];
                assert!(matches!(stream_index.context.value_type(), InputValueType::InterruptChoice(_)), "Not interrupt choice");
                let stream = test_input.input_streams_mut().get_mut(&stream_index.context).expect("get stream");

                // if stream_index.index >= stream.len() {
                //     continue;
                // }
                if remove {
                    // remove the interrupt
                    if let InputValue::InterruptChoice {interval , .. } = 
                        stream.as_mut().remove(stream_index.index) {
                            if let Some(interval) = interval {
                                removed_sum += interval
                            } else {
                                removed_sum += DEFEAULT_INTERVAL_SIZE
                            }
                    }
                    else { unreachable!("not interrupt choice");}
                } else {
                    // reduce the interval to MERGE_LEFTOVER_INTERVAL and add the difference to mid
                    if let InputValue::InterruptChoice {interval , ..} = 
                        &mut stream.as_mut()[stream_index.index] 
                    {
                        if let Some(interval) = interval {
                            if *interval > MERGE_LEFTOVER_INTERVAL {
                                removed_sum += *interval - MERGE_LEFTOVER_INTERVAL;
                                *interval = MERGE_LEFTOVER_INTERVAL;
                            }
                        } else {
                            removed_sum += DEFEAULT_INTERVAL_SIZE - MERGE_LEFTOVER_INTERVAL;
                            *interval = Some(MERGE_LEFTOVER_INTERVAL);
                        }
                    } else { unreachable!("not interrupt choice"); }
                }

            }
            removed_sum
        }


        fn merge(
                s: &mut Fuzzer,
                mut test_input: InputFile,
                mid: usize,
                current_window_end: usize,
                chrono_stream: &ChronoStream,
                interrupt_windows: &Vec<(usize, usize)>,
                current_result_info: &NewCoverage,
                remove: bool,
                add_intervals_to_start: bool,
            ) -> Option<NewCoverage> {

            log_for_viewer!("Start merge --- mid = {} (end = {}) and remove = {} add = {} {}", mid, current_window_end, remove, add_intervals_to_start, test_input);
            // log::info!("Chronstream {}", chrono_stream);

            let mut removed_sum = 0;
            
            // // after the currently selected interrupt (mid), reduce/remove the interrupt intervals
            if mid + 1 < current_window_end {
                // removed_sum = sum_intervals(&test_input, &interrupt_windows[(mid+1)..current_window_end], chrono_stream);
                removed_sum = shrink_or_remove_irqs(&mut test_input, &interrupt_windows[(mid+1)..current_window_end], chrono_stream, false);
            }
            // log::info!("Stage 1 {}", test_input);

            // add the removed intervals to mid interrupt
            if add_intervals_to_start {
                let stream_index = &chrono_stream.chrono_stream[interrupt_windows[mid].0];
                let interval = get_interval_mut(&mut test_input, stream_index);
                if let Some(int) = interval {
                    *int = *int + removed_sum; // + DEFEAULT_INTERVAL_SIZE; // TODO: should we add the DEFEAULT_INTERVAL_SIZE?
                } else {
                    // unreachable!("none interval");
                    log::info!("failed - rare edge case where last interval is None");
                    *interval = Some(DEFEAULT_INTERVAL_SIZE);
                    // if removed_sum > 0 {
                    //     *interval = Some(2 * removed_sum + 2 * DEFEAULT_INTERVAL_SIZE);
                    //     // *int = Some((removed_sum * 2 + DEFEAULT_INTERVAL_SIZE).max(removed_sum + 2 * DEFEAULT_INTERVAL_SIZE));
                    //     // *int = Some(removed_sum + DEFAULT_INTERVAL_START + DEFAULT_INTERVAL_START);
                    // } 
                    // TODO: why was this here?
                    // else {
                    //     *interval = Some(DEFEAULT_INTERVAL_SIZE);
                    // }
                }
            }

            let cutoff_idx = if remove {
                mid + 1
            } else {
                current_window_end
            };

            if cutoff_idx < interrupt_windows.len() {
                // We need to find out what input values to remove from the streams
                let mut head_file = create_head_file(s, &test_input, interrupt_windows, chrono_stream, cutoff_idx, mid + 1);
                let mut count = 0;
                while head_file.is_none() {
                    count += 1;
                    if count > 5 {return None}
                    log::info!("head file creation failed, we looping (merge)");
                    head_file = create_head_file(s, &test_input, interrupt_windows, chrono_stream, cutoff_idx, mid + 1);
                }
                let head_file= head_file.unwrap();
                
                // log::info!("Head file {}", head_file);
                if current_window_end < interrupt_windows.len() {
                    let tail_file = chrono_stream.remove_head_at_chrono(test_input, interrupt_windows[current_window_end].0);
                    // log::info!("Tail file {}", tail_file);
                    test_input = head_file.merge(tail_file);
                } else {
                    test_input = head_file;
                }
                test_input.reset_cursor();
                test_input.set_read_limit(test_input.len());
            }


            
            log_for_viewer!("Trying merge with mid = {} (end = {}) and remove = {}. After merge {}", mid, current_window_end, remove, test_input);
            // log::info!("Checking if merge works {}", test_input);
            // log::info!("testing merge at {} with input {}", mid, test_input);
            // Test if this setup maintains coverage
            match s.run_minimized_input(test_input).expect("running minimzed input") {
                CorpusResult::NewCoverage(minimized) => {
                    if verify_minimization(&minimized, current_result_info) {
                        // log::info!("Interval {} works for interrupt at index {}", mid, idx);
                        log_for_viewer!("exit success");
                        // log::info!("Chronstream {}", chrono_stream);
                        return Some(minimized);
                    }
                },
                _ => {}
            }
            log_for_viewer!("exit fail");
            return None;
        }

        fn minimize_interval(
            s: &mut Fuzzer,
            input: InputFile,
            chrono_stream: &ChronoStream,
            interrupt_windows: &Vec<(usize, usize)>,
            mut result_info: NewCoverage,
            irq_idx: usize
        ) -> NewCoverage {
            // Default starting interval value if interval is None
            let interrupt_window = interrupt_windows[irq_idx];
            log_for_viewer!("minimzing irq window {} (chrono {:?})", irq_idx, interrupt_window);

            // Minimize interval for the given irq
            let (chrono_index_start, chrono_index_end) = interrupt_window;
            let stream_index = &chrono_stream.chrono_stream[chrono_index_start];
            let next_irq_chrono_index = if chrono_index_end < chrono_stream.len() {
                Some(chrono_index_end)
            } else { None };
            
            let interrupt_val = &input
                .input_streams().get(&stream_index.context).expect("get input stream")
                .as_ref()[stream_index.index];

            match interrupt_val {
                InputValue::InterruptChoice{interval, ..}  => {
                    // Binary search to minimize the interval
                    let mut left = if irq_idx == 0 { 1u32 } else { 0u32 };
                    let mut right = if let Some(interval) = interval {
                            *interval
                        } else {
                            // the last interval can be None
                            // unreachable!("None-sized interval");
                            // TODO: remove?
                            DEFEAULT_INTERVAL_SIZE
                        };
                    // log::info!("Starting interval is {}", right);
                    let _starting_interval = right;

                    'binary_search: while left <= right {
                        let mid = left + (right - left) / 2;
                        // let mid = left;
                        // log::info!("trying len {}", mid);

                        // Create a test input with this interval value
                        let mut test_input = make_test_input(&input);
                        // log::info!("test_input premin: {}", test_input);

                        // Update the specific interrupt with the test interval
                        let int = get_interval_mut(&mut test_input, stream_index);
                        *int = Some(mid);

                        // log::info!("test_input after setting int to mid {}: {}", mid, test_input);

                        if let Some(next_irq_chrono_index) = next_irq_chrono_index {
                            // We need to find out what input values to remove from the streams
                            let mut head_file = create_head_file(s, &test_input, interrupt_windows, chrono_stream, irq_idx + 1, irq_idx + 1);
                            let mut count = 0;
                            while head_file.is_none() {
                                count += 1;
                                if count > 5 {
                                    left = mid + 1;
                                    continue 'binary_search;
                                }
                                log::info!("head file creation failed, we looping (min interval)");
                                head_file = create_head_file(s, &test_input, interrupt_windows, chrono_stream, irq_idx + 1, irq_idx + 1);
                            }
                            let head_file= head_file.unwrap();
                            // log::info!("headfile {}", head_file);
                            match get_interval(&head_file, stream_index) {
                                Some(int) if int == mid => {},
                                Some(_) => {
                                    left = mid + 1;
                                    log_for_viewer!("Interval {} does not work", mid);
                                    continue;
                                },
                                _ => {
                                    // unreachable!("interval is None");
                                    // TODO: idk why but happens in very rare case
                                    left = mid + 1;
                                    log_for_viewer!("Interval {} does not work", mid);
                                    continue;
                                    // In super rare cases, maybe because of many executions in short time
                                    //  qemu/hoedur will not correctly execute the input.
                                    // log::info!("exeuction failed, trying again with same interval.")
                                    // log_for_viewer!("WTF run again");
                                    // continue;
                                } 
                            }
                            // log::info!("Head file {}", head_file);
                            let tail_file = chrono_stream.remove_head_at_chrono(test_input, next_irq_chrono_index);
                            // log::info!("Tail file {}", tail_file);
                            test_input = head_file.merge(tail_file);
                            test_input.reset_cursor();
                            test_input.set_read_limit(test_input.len());
                        }

                        // log::info!("test_input post-min: {}", test_input);

                        log_for_viewer!("Testing interrupt at chrono index {} with interval {} input {}", chrono_index_start, mid, test_input);

                        // Test if this interval still maintains coverage
                        match s.run_minimized_input(test_input).expect("running minimized input") {
                            CorpusResult::NewCoverage(minimized) => {
                                // log::info!("newcov {:?} {}", minimized.result().stop_reason(), minimized.result().basic_blocks());
                                if verify_minimization(&minimized, &result_info)
                                    && verify_interval(&minimized, stream_index, mid) {
                                    log_for_viewer!("Interval {} works for interrupt at index {}", mid, chrono_index_start);
                                    // This interval works, try to minimize further
                                    result_info = minimized;

                                    if mid == 0 {
                                        break
                                    }

                                    right = mid - 1;


                                } else {
                                    left = mid + 1;
                                    log_for_viewer!("Interval {} does not work", mid);
                                    // assert!(mid != starting_interval, "Interval {} did not work, but it is the starting interval! This should not happen", mid);
                                }
                            },
                            CorpusResult::ShorterInput(_minimized) | CorpusResult::Uninteresting(_minimized) => { 
                                // log::info!("shorter/unint {:?} {}", minimized.result.stop_reason(), minimized.result.basic_blocks());
                                left = mid + 1; 
                                log_for_viewer!("Interval {} does not work", mid);
                                // assert!(mid != starting_interval, "Interval {} did not work, but it is the starting interval! This should not happen", mid);

                            },
                        }
                    }
                    // input = result_info.result().file().clone();
                    // log::info!("input after loop {:?}", input);
                    // log::info!("Final interval for interrupt at index {} is {}", idx, left);
                },
                _ => { unreachable!("Not interruptChoice.."); }
            }

            // log::info!("Input stream after minimizing interrupt intervals: {}", input);
            result_info
        }

        fn window_trim(
            s: &mut Fuzzer, input: &InputFile, get_window_boundaries: fn(&ChronoStream, &InputFile, &Range<usize>) -> Vec<(usize, usize)>,
            chrono_stream: &ChronoStream, original_result_info: &NewCoverage, irq_level: bool, mut range: Range<usize>) -> Option<(NewCoverage, usize)> {

            let mut window_boundaries = get_window_boundaries(chrono_stream, input, &range);
            
            let starting_windows = window_boundaries.len();
            if starting_windows < 2 {
                return None;
            }

            let mut input = input.clone();
            let mut chrono_stream = chrono_stream.clone();

            let mut windows_removed = 0;
            let mut windows_skipped = 0;
            let mut window_max_right = starting_windows - 1;
            let mut new_result_info = None;

            // log::info!("Starting chronostream: {:?}", minimized.result().chrono_stream_init());
            // log::info!("Interrupt indices: {}", interrupt_indices.len());
            // log::info!("Starting windowtrim input windows with {}", input);

            while windows_skipped < window_max_right { // minimize as much as possible from the beginning. Then skip the window that has to be there
                let mut left = windows_skipped;
                let mut right = window_max_right;
                let mut found_removal = false;
                let mut best_found_cutoff = 0;

                let mut current_cutoff = if ENABLE_SMART_INTERRUPT_MINIMIZE {
                    left + 1 // start with one. if works, start binary search.
                } else if ENABLE_BINARY_SEARCH_INTERRUPT_MINIMIZE {
                    (right + left) / 2 // start directly with binary search
                } else {
                    right
                };

                while left < right { // binary search for best removal frame.
                    log_for_viewer!("Trying to remove windows: [{},{}>", windows_skipped, current_cutoff);
                    let left_chrono_index = window_boundaries[windows_skipped].0;
                    let right_chrono_index = window_boundaries[current_cutoff - 1].1;
                    let chrono_range = left_chrono_index..right_chrono_index;

                    // log_for_viewer!("pre-trim input: {}", input);

                    let mut test_input = make_test_input(&input);

                    // remove inputs
                    if ENABLE_SANITY_CHECKS {
                        assert!(*chrono_stream.chrono_stream[left_chrono_index].context.context() == StreamContext::Interrupt);
                        assert!(*chrono_stream.chrono_stream[right_chrono_index].context.context() == StreamContext::Interrupt);
                    }
                    // increase the interval of the last interrupt
                    // if irq_level {
                    //     let final_interrupt = &chrono_stream.chrono_stream[final_irq_idx]; // window_boundaries.last().expect("get last").0
                    //     if let Some(int) =  get_interval_mut(&mut test_input, final_interrupt) {
                    //         *int += (current_cutoff - windows_skipped) as u16;// TODO: fix!
                    //     }
                    // }

                    // log::info!("post-increase interval: {}", test_input);

                    // remove the IRQs + MMIO values
                    for (context, stream) in test_input.input_streams_mut() {
                        if stream.is_empty() { continue; }
                        if let Some(range) = chrono_stream.stream_range(context, &chrono_range) {
                            let range = range.start.min(stream.len())..range.end.min(stream.len());
                            // log::info!("Before rasing\t\t {:?} {:?}", context, stream);
                            stream.as_mut().splice(range, iter::empty());
                            // log::info!("After erasing\t\t {:?} {:?}", context, stream);
                        }
                    }

                    // if ENABLE_SANITY_CHECKS {
                    //     let new_len = test_input.len();
                    //     let exp = chrono_stream.len() - (right_chrono_index - left_chrono_index);
                    //     if new_len != exp {
                    //         log::info!("newlen {} != expect {} (failed)", new_len, exp);
                    //     }
                    // }

                    log_for_viewer!("post trim input {}", test_input);

                    match s.run_minimized_input(test_input).expect("run input") {
                        CorpusResult::NewCoverage(new_info) => {
                            // log::info!("Found an input with interrupts removed starting from index {} - pre verification", current_cutoff);
                            if verify_minimization(&new_info, original_result_info) {
                                found_removal = true;
                                log_for_viewer!("trim success");
                                log_for_viewer!("Can remove [{}, {}> (chrono {}, {})", windows_skipped, current_cutoff, left_chrono_index, right_chrono_index);
                                best_found_cutoff = current_cutoff;
                                // log::info!("Removes {} reads", best_found_removed_reads);
                                new_result_info = Some(new_info);
                                left = current_cutoff;
                                
                                // log::info!("removes from chronostream: {:?}", &base_input.chrono_stream().chrono_stream[left_chrono_index..=right_chrono_index]);
                                // log::info!("chronostream after change: {:?}", minimized.result().chrono_stream_init());
                            }
                            else {
                                right = current_cutoff;
                                log_for_viewer!("trim fail");
                            }
                        }
                        CorpusResult::ShorterInput(result) => {
                            // log::info!("From minimize interrupts");
                            let _ = s.shorter_input(result.into_inner()).expect("shorter input");
                            // log::info!("Found a shorter input with interrupts removed starting from index {}", current_cutoff);
                            right = current_cutoff;
                            log_for_viewer!("trim fail");
                        }
                        CorpusResult::Uninteresting(_) => {
                            right = current_cutoff;
                            log_for_viewer!("trim fail");
                        }
                    }
                    if ENABLE_BINARY_SEARCH_INTERRUPT_MINIMIZE {
                        current_cutoff = (right + left) / 2;
                    } else {
                        current_cutoff = right - 1;
                    }
                    if left == current_cutoff {
                        break;
                    }
                }
                if found_removal {
                    let windows_removed_this_pass = best_found_cutoff - windows_skipped;
                    windows_removed += windows_removed_this_pass;
                    range.end -= windows_removed_this_pass;
                    // result_info.result_mut().file_mut().remove_unread_values();
                    chrono_stream = new_result_info.as_ref().expect("new result info").result().chrono_stream().clone();
                    input = new_result_info.as_ref().expect("new result info").result().file().clone();
                    if irq_level {
                        // rare edge case where some interrupts of the last message are not needed anymore
                        let irqs = chrono_stream.find_irq_chrono_indices().len();
                        if range.end > irqs {
                            range.end = irqs;
                        }
                    }
                    // log::info!("New range: {:?}", range);
                    // log::info!("irq indices {:?}", chrono_stream.find_irq_chrono_indices());
                    // log::info!("input {}", input);
                    window_boundaries = get_window_boundaries(&chrono_stream, &input, &range);
                    if window_boundaries.len() == 0 {
                        break
                    }
                    window_max_right = window_boundaries.len()-1;
                }
                windows_skipped += 1;

                // log::info!("Now {} windows removed!", windows_removed);
            }
            if irq_level {
                log_for_viewer!("Removed {}/{} interrupt windows", windows_removed, starting_windows);
            } else {
                log_for_viewer!("Removed {}/{} message windows", windows_removed, starting_windows);
            }
            
            if let Some(result_info) = new_result_info {
                Some((result_info, windows_removed))
            } else { None }
        }

        fn get_window_end(chrono_stream: &ChronoStream, input: &InputFile, interrupt_windows: &Vec<(usize, usize)>, start: usize) -> usize {
            let non_zero_idcs = chrono_stream.find_idc_none_zero_irqs(input);
            let next = non_zero_idcs.windows(2).find(|w| w[0] == start).map(|w| &w[1]);

            if let Some(start_next) = next {
                *start_next
            } else {
                interrupt_windows.len()
            }
        }

        // find message windows by tailing inputs
        let mut left: usize = 0;
        let mut right: usize = interrupt_windows.len();
        let mut current_window_end: usize = right; // index of NOT-included irq (i.e., the next input window, or behind last irq)

        // log::info!("Starting merge_and_min with input {}", input);

        // continue until we found all input windows
        while current_window_end > 0 {
            // the current message window (starting from the back)
            let mut best_result = None;
            while left < right {
                // use binary search to look for the start of each input window
                let mid: usize = left + (right - left) / 2;

                // Create a test input with this interval value
                let test_input = make_test_input(&input);
                
                if let Some(new_cov) = merge(self, test_input, mid, current_window_end,
                        &chrono_stream, &interrupt_windows, &result_info, false, false
                ) {
                    best_result = Some((mid, new_cov, false));
                    right = mid;
                } else {
                    // attempt 2: add intervals to start
                    let test_input = make_test_input(&input);
                    if let Some(new_cov) = merge(self, test_input, mid, current_window_end,
                        &chrono_stream, &interrupt_windows, &result_info, false, true
                    ) {
                        best_result = Some((mid, new_cov, true));
                        right = mid;
                    } else {
                        left = mid + 1;
                    }
                }
            }
            
            let start_of_input_window: usize;
            
            // if the message windows has more than 1 irq, further process it
            if let Some((mid,merge_result, add_intervals)) = best_result {
                start_of_input_window = mid;
                let test_input = make_test_input(&input);

                // try removing all the minimized interrupts.
                if let Some(new_cov) = merge( self, test_input, start_of_input_window, current_window_end,
                            &chrono_stream, &interrupt_windows, &result_info, true, add_intervals
                    ) {
                        result_info = new_cov;
                        input = result_info.result().file().clone();
                        chrono_stream = result_info.result().chrono_stream().clone();
                        interrupt_windows = chrono_stream.find_irq_chrono_windows();
                        current_window_end = start_of_input_window + 1;
                }
                else {
                    // if not all interrupts in the message could be removed (Except the first one), trim them
                    result_info = merge_result;
                    input = result_info.result().file().clone();
                    chrono_stream = result_info.result().chrono_stream().clone();
                    interrupt_windows = chrono_stream.find_irq_chrono_windows();

                    // there are some edge cases we need to cover when it comes to the window_end
                    current_window_end = get_window_end(&chrono_stream, &input, &interrupt_windows, start_of_input_window);
                    if current_window_end == interrupt_windows.len() { at_last_message = true }

                    // log::info!("Starting window trim. Msg {}, start {}, end {}, {}, {}", at_last_message, start_of_input_window, current_window_end, input, chrono_stream);

                    let test_input = make_test_input(&input);

                    // trim the current message window
                    let get_window_boundaries = 
                        |chrono_stream: &ChronoStream, _input: &InputFile, range: &Range<usize>| -> Vec<(usize, usize)> {
                            chrono_stream.find_irq_chrono_windows()[range.clone()].into()
                        };
                        
                    // we need to skip the first interrupt (part of prev message), and include the next (part of this message)
                    let smart_trim_range: Range<usize> = (start_of_input_window+1)..(if at_last_message {current_window_end} else {current_window_end + 1});
                    // log::info!("send to the trimmer {:?} {}", smart_trim_range, input);
                    // log::info!("int windows (len {}) {:?}", interrupt_windows.len(), interrupt_windows);
                        
                    log_for_viewer!("Smart trim for window {:?}", smart_trim_range);
                    if let Some((new_cov, _irq_removed)) =
                        window_trim(self, &test_input, get_window_boundaries, &chrono_stream,
                            &result_info, true, smart_trim_range) {
                                result_info = new_cov;
                                input = result_info.result().file().clone();
                                chrono_stream = result_info.result().chrono_stream().clone();
                                interrupt_windows = chrono_stream.find_irq_chrono_windows();
                                current_window_end = get_window_end(&chrono_stream, &input, &interrupt_windows, start_of_input_window);
                                if current_window_end == interrupt_windows.len() {  } // at_last_message = true
                    }
                }
            }
            else {
                // if there is only one irq in the message, that is start idx
                start_of_input_window = right - 1;
            }

            // log::info!("pre minimizing intervals: {:?} {}", interrupt_indices, input);
            // log::info!("uniq feats pre {:?}", result_info.uniq_features());
            
            // minimize interval in the newly found input window
            if current_window_end < interrupt_windows.len() {
                result_info = minimize_interval(self, input, &chrono_stream, &interrupt_windows, result_info, current_window_end);
                input = result_info.result().file().clone();
                // TODO: uncomment?
                chrono_stream = result_info.result().chrono_stream().clone();
                interrupt_windows = chrono_stream.find_irq_chrono_windows();
            }

            // log::info!("uniq feats post {:?}", result_info.uniq_features());
            // done with this input window

            left = 0;
            right = start_of_input_window;
            current_window_end = right;

            at_last_message = false;
            // log::info!("Done with 1 input window: {}", input);

        }

        // minimize the first interval
        let test_input: InputFile = make_test_input(&input);
        result_info = minimize_interval(self, test_input, &chrono_stream, &interrupt_windows, result_info, 0);
        
        // input = result_info.result().file().clone();

        // log::info!("Final interval for interrupt at index {} is {}", idx, left);

        log_for_viewer!("Starting message trim");
        // remove unnecesarry message windows
        let test_input = make_test_input(&input);
        let get_message_boundaries =                 
            |chrono_stream: &ChronoStream, input: &InputFile, _range: &Range<usize>| -> Vec<(usize, usize)> {
                    chrono_stream.find_message_chrono_windows(input)
            };

        if let Some((new, _messages_removed)) = window_trim(self, &test_input, get_message_boundaries,
             &chrono_stream, &result_info, false, 0..test_input.len()) {
                result_info = new;
        }
        
        log::info!("Done with all input windows: {}", input);


        Ok(result_info)
    }


    fn write_config(&mut self) -> Result<()> {
        let timestamp = epoch()?;

        // write seed
        write_file(
            &mut self.archive.borrow_mut(),
            "config/seed.bin",
            timestamp,
            &self.seed.to_be_bytes(),
        )
        .context("write corpus seed")?;

        Ok(())
    }

    fn write_statistics(&mut self) -> Result<()> {
        let timestamp = epoch()?;

        // executions history
        if let Some(executions) = self.statistics.executions() {
            write_serialized(
                &mut self.archive.borrow_mut(),
                "statistics/executions.bin",
                timestamp,
                &executions,
            )
            .context("write executions history")?;
        }

        // input size history
        if let Some(input_size) = self.statistics.input_size() {
            write_serialized(
                &mut self.archive.borrow_mut(),
                "statistics/input-size.bin",
                timestamp,
                &input_size,
            )
            .context("write input size history")?;
        }

        Ok(())
    }

    fn write_input_files(&mut self) -> Result<()> {
        for info in self.corpus.inputs() {
            write_input_file(&mut self.archive.borrow_mut(), info.result())?;
        }

        Ok(())
    }

    fn next_mutator(&self) -> MutatorKind {
        MutatorKind::index_enum(self.distribution_mutator.sample(&mut FastRand))
            .expect("Mutator index is valid")
    }

    fn next_stream_random_distribution(&self) -> StreamRandomDistribution {
        StreamRandomDistribution::index_enum(self.distribution_stream_select.sample(&mut FastRand))
            .expect("StreamRandomDistribution index is valid")
    }
}

/// verify stop reason and unqiue features are equal
fn verify_minimization(new: &NewCoverage, old: &NewCoverage) -> bool {
    let new_stop_reason = new.result().stop_reason();
    if new_stop_reason != old.result().stop_reason()
        // && *new_stop_reason != StopReason::InfiniteSleep
        && *new_stop_reason != StopReason::EndOfInput {
            log::info!("new {:?}, old {:?}", new_stop_reason, old.result().stop_reason());
            return false
    }
    new.uniq_features().is_superset(old.uniq_features())
}

fn verify_interval(new: &NewCoverage, stream_idx: &StreamIndex, expected_interval: u32) -> bool {
    if let Some(stream) = new.result().file().input_streams().get(&stream_idx.context) {
        if let Some(modeling::input::value::InputValue::InterruptChoice {
            interval: int,
            ..
        }) = stream.as_ref().get(stream_idx.index)
        {
            if let Some(interval) = int {
                if expected_interval == *interval {
                    true
                }
                else {
                    // log::info!("interval min mismatch: {} != {}", expected_interval, *interval);
                    false
                 }
            } else {
                false
                // unreachable!("None interval");
            }
        } else { unreachable!("get interrupt stream")}
    } else { unreachable!(" get stream")}
}
