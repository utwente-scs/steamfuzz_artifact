use std::{
    borrow::Cow,
    cmp::Ordering,
    fmt,
    hash::Hash,
    io::{Read, Write},
    ops::Deref,
    path::Path,
    str::FromStr,
    sync::atomic::{self, AtomicUsize},
};

use anyhow::{bail, Context, Result};
use common::{
    FxHashMap, config::{
        fuzzer::{RANDOM_EMPTY_STREAM, RANDOM_NEW_STREAM},
        input::{INPUT_CONTEXT_TYPE, InputContextType},
        mutation::RANDOM_COUNT_STREAM_RANGE_POW2,
    }, fs::bufreader, hashbrown::hash_map::{Entry, RawEntryMut}, random::DeriveRandomSeed
};
use itertools::Itertools;
use qemu_rs::{Address, MmioAddress};
use serde::{Deserialize, Serialize};

use crate::{
    hardware::{Input, WriteTo},
    input::stream::StreamInternal,
    mmio::AccessContext,
};

pub mod stream;
pub mod value;
use stream::{InputStream, Stream};
use value::{InputValue, InputValueType};

static NEXT_INPUT_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Serialize, Deserialize)]
pub struct InputId(pub usize);

impl Deref for InputId {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Default for InputId {
    fn default() -> Self {
        Self::next()
    }
}

impl InputId {
    pub unsafe fn new(id: usize) -> Self {
        Self(id)
    }

    pub fn next() -> Self {
        Self(NEXT_INPUT_ID.fetch_add(1, atomic::Ordering::Relaxed))
    }
}

impl fmt::Display for InputId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct InputContext {
    context: StreamContext,
    value_type: InputValueType,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamContext {
    AccessContext { pc: Address, mmio: MmioAddress },
    MmioContext { mmio: MmioAddress },
    Custom { id: u64 },
    None,
    Interrupt,
}

impl InputContext {
    pub fn new(context: StreamContext, value_type: InputValueType) -> Self {
        Self {
            context,
            value_type,
        }
    }

    pub fn from_access(context: &AccessContext, value_type: InputValueType) -> Self {
        let stream_context = match INPUT_CONTEXT_TYPE {
            InputContextType::AccessContext => {
                StreamContext::access_context(context.pc(), context.mmio().addr())
            }
            InputContextType::MmioContext => StreamContext::mmio_context(context.mmio().addr()),
            InputContextType::None => StreamContext::None,
        };

        Self::new(stream_context, value_type)
    }

    pub fn context(&self) -> &StreamContext {
        &self.context
    }

    pub fn value_type(&self) -> InputValueType {
        self.value_type
    }

    pub fn is_interrupt(&self) -> bool {
        self.context == StreamContext::Interrupt
    }
}

impl StreamContext {
    pub fn access_context(pc: Address, mmio: MmioAddress) -> Self {
        Self::AccessContext { pc, mmio }
    }

    pub fn mmio_context(mmio: MmioAddress) -> Self {
        Self::MmioContext { mmio }
    }

    pub fn custom(id: u64) -> Self {
        Self::Custom { id }
    }

    pub fn pc(&self) -> Option<Address> {
        match self {
            StreamContext::AccessContext { pc, .. } => Some(*pc),
            StreamContext::MmioContext { .. }
            | StreamContext::Custom { .. }
            | StreamContext::None
            | StreamContext::Interrupt => None,
        }
    }

    pub fn mmio(&self) -> Option<MmioAddress> {
        match self {
            StreamContext::AccessContext { mmio, .. } | StreamContext::MmioContext { mmio } => {
                Some(*mmio)
            }
            StreamContext::Custom { .. } | StreamContext::None | StreamContext::Interrupt => None,
        }
    }

    pub fn id(&self) -> Option<u64> {
        match self {
            StreamContext::Custom { id } => Some(*id),
            StreamContext::AccessContext { .. }
            | StreamContext::MmioContext { .. }
            | StreamContext::None
            | StreamContext::Interrupt => None,
        }
    }

    pub fn to_padded_string(&self) -> String {
        match self {
            Self::AccessContext { pc, mmio } => format!("pc: {pc:>8x}, mmio: {mmio:>8x}"),
            Self::MmioContext { mmio } => format!("mmio: {mmio:>8x}"),
            Self::Custom { id } => format!("id: {id:>16x}"),
            Self::None => "none".to_string(),
            Self::Interrupt => "irq".to_string(),
        }
    }
}

impl Ord for StreamContext {
    fn cmp(&self, other: &Self) -> Ordering {
        self.mmio()
            .cmp(&other.mmio())
            .then_with(|| self.pc().cmp(&other.pc()))
            .then_with(|| self.id().cmp(&other.id()))
    }
}

impl PartialOrd for StreamContext {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for InputContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:x?} {}", self.context, self.value_type)
    }
}

impl FromStr for InputContext {
    type Err = anyhow::Error;

    fn from_str(context: &str) -> Result<Self> {
        let mut parts: Vec<_> = context.split('_').collect();

        // verify format
        if parts.len() < 2 || parts.len() > 3 {
            bail!(
                "Unknown InputContext format, expected mmio_type or pc_mmio_type: {:?}",
                context
            );
        }

        // parse PC (optional)
        let pc = if parts.len() == 3 {
            Some(
                Address::from_str_radix(parts.remove(0), 16)
                    .context("Failed to parse MMIO address")?,
            )
        } else {
            None
        };

        // parse MMIO
        let mmio = MmioAddress::from_str_radix(parts.remove(0), 16)
            .context("Failed to parse MMIO address")?;

        // parse value type
        let mut parts = parts.remove(0).split('-');
        let value_type = match parts.next().context("input value type variant missing")? {
            "byte" => InputValueType::Byte,
            "word" => InputValueType::Word,
            "dword" => InputValueType::DWord,
            "bits" => {
                let bits = parts
                    .next()
                    .context("bits missing")?
                    .parse()
                    .context("failed to parse bits")?;
                InputValueType::Bits(bits)
            }
            "choice" => {
                let len = parts
                    .next()
                    .context("len missing")?
                    .parse()
                    .context("failed to parse bits")?;
                InputValueType::Choice(len)
            }
            unknown => bail!("unknown value type: {:?}", unknown),
        };

        let context = match pc {
            Some(pc) => StreamContext::access_context(pc, mmio),
            None => StreamContext::mmio_context(mmio),
        };

        Ok(InputContext::new(context, value_type))
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct InputFile {
    id: InputId,
    #[serde(skip)]
    parent: Option<InputId>,
    input_streams: FxHashMap<InputContext, InputStream>,
    #[serde(skip)]
    random_seed: Option<u64>,
    #[serde(skip)]
    random_count: Option<usize>,
    #[serde(skip)]
    pub read_limit: Option<usize>,
    #[serde(skip)]
    last_interrupt_context: Option<InputContext>
}

impl Hash for InputFile {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.input_streams.iter().for_each(|(ctx, inst)| {
            ctx.hash(state);
            inst.hash(state);
        })
    }
}

impl fmt::Display for InputFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Input #{}", *self.id())?;
        match self.parent() {
            Some(parent) => writeln!(f, " (Parent #{})", *parent),
            None => writeln!(f, " (No Parent)"),
        }?;

        writeln!(f, "{} inputstreams.", self.input_streams().len())?;
        for (ctx, is) in self
            .input_streams()
            .iter()
            .sorted_by(|(a, _), (b, _)| a.cmp(b))
        {
            // writeln!(f, "Input Stream {} ({} bytes):\n{}", ctx, is.bytes(), is)?;
            writeln!(f, "Input Stream {} ({} reads, cursor at {}).", ctx, is.len(), is.cursor())?;
            if matches!(ctx.value_type(), InputValueType::InterruptChoice(_)) {
                writeln!(f, "{is}")?;
            } 
        }

        Ok(())
    }
}

impl Input for InputFile {
    fn id(&self) -> usize {
        *self.id()
    }

    fn reset(&mut self) {
        self.reset_cursor();
    }

    fn reset_interrupt_context(&mut self) {
        self.last_interrupt_context = None;
    }

    fn last_interrupt_context(&self) -> bool {
        self.last_interrupt_context.is_some()
    }

    fn read(&mut self, context: &InputContext) -> Option<Cow<InputValue>> {
        // log::info!("read lim {:?} {:?}", self.read_limit, context);
        // verify read limit, if any
        if let Some(read_limit) = &mut self.read_limit {
            if *read_limit > 0 {
                *read_limit -= 1;
            } else {
                return None;
            }
        }

        // create missing input stream or return end of stream
        let is = match self.input_streams.raw_entry_mut().from_key(context) {
            RawEntryMut::Occupied(entry) => entry.into_key_value().1,
            RawEntryMut::Vacant(entry) => {
                log::debug!("new input stream found: {:x?}", context);

                if let Some(seed) = self.random_seed {
                    // add random count for new input stream (only when none was set before)
                    if RANDOM_NEW_STREAM && self.random_count.is_none() {
                        // set random seed: input seed + input stream context
                        fastrand::seed(seed.derive(&context));

                        self.random_count =
                            Some(1 << fastrand::usize(RANDOM_COUNT_STREAM_RANGE_POW2));
                    }

                    // random input value available
                    if RANDOM_EMPTY_STREAM || self.random_count > Some(0) {
                        // create new stream
                        entry
                            .insert(context.clone(), InputStream::new(context.value_type()))
                            .1
                    } else {
                        // no random available => end of stream
                        // log::info!("returning none 1");
                        return None;
                    }
                } else {
                    // no random available => end of stream
                    // log::info!("returning none 2");
                    return None;
                }
            }
        };

        let init_random = |seed: u64, context, is: &InputStream| {
            // set random seed: input seed + input stream context + cursor
            fastrand::seed(seed.derive(&context).derive(&is.cursor()));
        };

        // let is = self.input_streams.get_mut(&context).unwrap();
        let random_seed = &self.random_seed;
        let random_count = &mut self.random_count;
        let input_val = is.next_or_random(|is| {
            match (random_seed, random_count) {
                // empty stream => random value
                (Some(seed), _) if RANDOM_EMPTY_STREAM && is.is_empty() => {
                    // log::info!("Doing A");
                    init_random(*seed, context, is);
                    true
                }
                // end of stream with random_count > 0 => random value
                (Some(seed), Some(random_count)) if *random_count > 0 => {
                    init_random(*seed, context, is);
                    *random_count -= 1;
                    // log::info!("Doing B");
                    true
                }
                // odd usage warning
                (None, Some(random_count)) if *random_count > 0 => {
                    log::warn!("end of input found with random count set in a replay input (failed)");
                    // log::info!("Doing C");
                    false
                }
                // end of stream
                (_, _) => {
                    // log::info!("returning none 3. read lim {:?}", self.read_limit);
                    false
                },
            }
        })
        .map(Cow::Borrowed);
        
        if context.is_interrupt() && input_val.is_some() {
            // log::info!("set last irq context to {:?}", context);
            self.last_interrupt_context = Some(context.clone());
        }
        input_val
    }

    fn set_irq_interval(&mut self, interval: Option<u32>) {
        if let Some(context) = &self.last_interrupt_context {
            let stream = match self.input_streams.raw_entry_mut().from_key(context) {
                RawEntryMut::Occupied(entry) => {
                    entry.into_key_value().1
                },
                RawEntryMut::Vacant(_) => { unreachable!("get inputstream") }
            };
            stream.set_last_irq_interval(interval);
        } else {
            // log::info!("last interrupt context == None.") 
            // this can happen because systick returns before the first interrupt
        }
    }

}

impl InputFile {
    pub fn from_streams(id: InputId, input_streams: FxHashMap<InputContext, InputStream>) -> Self {
        Self {
            id,
            parent: None,
            input_streams,
            random_seed: None,
            random_count: None,
            read_limit: None,
            last_interrupt_context: None
        }
    }

    pub fn fork(&self) -> Self {
        Self {
            id: InputId::next(),
            parent: Some(self.id),
            input_streams: self
                .input_streams
                .iter()
                .map(|(context, input_stream)| (context.clone(), input_stream.fork()))
                .collect(),
            random_seed: None,
            random_count: None,
            read_limit: None,
            last_interrupt_context: None,
        }
    }

    pub fn fork_for_corpus(&self) -> Self{
        Self {
            id: InputId::next(),
            parent: self.parent.clone(),
            input_streams: self
                .input_streams
                .iter()
                .map(|(context, input_stream)| (context.clone(), input_stream.fork_for_corpus()))
                .collect(),
            random_seed: self.random_seed.clone(),
            random_count: None, //self.random_count.clone(),
            read_limit: self.read_limit.clone(),
            last_interrupt_context: None,
        }
    }

    pub fn replace_id(&mut self, base: &InputFile) {
        self.id = base.id;
        self.parent = base.parent;
    }

    pub fn set_random_seed(&mut self, random_seed: u64) {
        self.random_seed = Some(random_seed);
    }
    
    pub fn remove_random_seed(&mut self) {
        self.random_seed = None;
    }

    pub fn random_seed(&self) -> Option<u64> {
        self.random_seed
    }

    pub fn random_count(&self) -> Option<usize> {
        self.random_count
    }

    pub fn set_random_count(&mut self, random_count: usize) {
        self.random_count = Some(random_count);
    }
    
    pub fn remove_random_count(&mut self) {
        self.random_count = None;
    }

    pub fn set_read_limit(&mut self, read_limit: usize) {
        self.read_limit = Some(read_limit);
    }

    pub fn remove_read_limit(&mut self) {
        self.read_limit = None;
    }

    pub fn id(&self) -> InputId {
        self.id
    }

    pub fn parent(&self) -> Option<InputId> {
        self.parent
    }

    pub fn input_streams(&self) -> &FxHashMap<InputContext, InputStream> {
        &self.input_streams
    }

    pub fn input_streams_mut(&mut self) -> &mut FxHashMap<InputContext, InputStream> {
        &mut self.input_streams
    }

    pub fn len(&self) -> usize {
        self.input_streams.values().map(InputStream::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.input_streams.values().all(|is| is.is_empty())
    }

    pub fn bytes(&self) -> usize {
        self.input_streams.values().map(InputStream::bytes).sum()
    }

    pub fn read_count(&self) -> usize {
        self.input_streams.values().map(|is| is.cursor()).sum()
    }

    pub fn remaining_values(&self) -> usize {
        self.input_streams
            .values()
            .map(|is| is.len() - is.cursor())
            .sum()
    }

    pub fn remove_unread_values(&mut self) {
        for stream in self.input_streams.values_mut() {
            stream.minimize();
        }
    }

    pub fn remove_empty_streams(&mut self) {
        self.input_streams
            .drain_filter(|_, stream| stream.is_empty());
    }

    /// Merge two input files.
    /// Input of `other` is appended to `self`.
    pub fn merge(mut self, other: InputFile) -> InputFile {
        self.id = InputId::next();

        for (context, other_stream) in other.input_streams {
            match self.input_streams.entry(context) {
                Entry::Occupied(mut entry) => entry.get_mut().merge(other_stream),
                Entry::Vacant(entry) => {
                    entry.insert(other_stream);
                }
            }
        }

        self
    }

    /// Split input at cursor.
    /// Everything after the cursor is added to the new input.
    pub fn split(&self) -> InputFile {
        InputFile {
            id: self.id,
            parent: self.parent,
            input_streams: self
                .input_streams
                .iter()
                .map(|(context, stream)| (context.clone(), stream.split()))
                .collect(),
            random_seed: self.random_seed,
            random_count: self.random_count,
            read_limit: self.read_limit,
            last_interrupt_context: None
        }
    }

    pub fn set_cursor(&mut self, other: &Self) {
        for (context, stream) in &mut self.input_streams {
            stream.set_cursor(
                other
                    .input_streams()
                    .get(context)
                    .map(Stream::cursor)
                    .unwrap_or(0),
            );
        }
    }

    /// Reset the cursor of each input stream.
    pub fn reset_cursor(&mut self) {
        for stream in self.input_streams.values_mut() {
            stream.set_cursor(0);
        }
    }

    pub fn read_from_path(path: &Path) -> Result<Self> {
        bufreader(path).and_then(Self::read_from)
    }

    pub fn read_from_slice(content: &[u8]) -> Result<Self> {
        bincode::deserialize(content).context("Failed to deserialize")
    }

    pub fn read_from<R: Read>(reader: R) -> Result<Self> {
        bincode::deserialize_from(reader).context("Failed to deserialize")
    }
}

impl WriteTo for InputFile {
    fn write_to<W: Write>(&self, writer: W) -> Result<()> {
        bincode::serialize_into(writer, self).context("Failed to serialize input file")
    }

    fn write_size(&self) -> Result<u64> {
        bincode::serialized_size(self).context("Failed to get serialized input file size")
    }

    fn filename(&self) -> String {
        format!("input-{}.bin", *self.id())
    }
}
