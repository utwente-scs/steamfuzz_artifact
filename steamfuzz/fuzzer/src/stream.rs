use std::{fmt, iter, mem, ops::Range};

use common::{FxHashMap, config::fuzzer::MINIMUM_INPUT_WINDOW_INTERVAL, hashbrown::hash_map::Entry};
use itertools::Itertools;
use modeling::input::{InputContext, InputFile, StreamContext, stream::Stream, value::InputValue};

#[derive(Debug, Clone)]
pub struct ChronoStream {
    pub chrono_stream: Vec<StreamIndex>,
    reverse_lookup: FxHashMap<InputContext, Vec<usize>>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct StreamIndex {
    pub context: InputContext,
    pub index: usize,
}

impl fmt::Display for StreamIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.context.context() {
            StreamContext::AccessContext { pc, mmio } => write!(f, "{pc:x}@{mmio:x}[{}]", self.index),
            StreamContext::MmioContext { mmio } => write!(f, "@{mmio:x}[{}]", self.index),
            StreamContext::Interrupt => write!(f, "irq({:?})[{}]", self.context.value_type(), self.index),
            other => write!(f, "{other:?}[{}]", self.index),
        }
    }
}

impl fmt::Display for ChronoStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        write!(f, "[")?;
        for entry in &self.chrono_stream {
            if *entry.context.context() == StreamContext::Interrupt {
                write!(f, "\n  {}", entry)?;
                first = false;
            } else {
                if !first {
                    write!(f, ", ")?;
                }
                write!(f, "{}", entry)?;
                first = false;
            }
        }
        write!(f, "]")
    }
}

impl ChronoStream {
    pub fn from_access_log(access_log: Vec<InputContext>) -> Self {
        // keep track of stream index (forward lookup)
        let mut index: FxHashMap<_, usize> = FxHashMap::default();
        let mut next_index = |context| {
            *(match index.entry(context) {
                Entry::Occupied(entry) => {
                    let index = entry.into_mut();
                    *index += 1;
                    index
                }
                Entry::Vacant(entry) => entry.insert(0),
            })
        };

        let mut chrono_stream = Vec::with_capacity(access_log.len());
        let mut reverse_lookup: FxHashMap<_, Vec<_>> = FxHashMap::default();

        for context in access_log {
            // add input stream to chrono stream lookup entry (reverse lookup)
            reverse_lookup
                .entry(context.clone())
                .or_default()
                .push(chrono_stream.len());

            // create chrono stream entry
            let index = next_index(context.clone());
            chrono_stream.push(StreamIndex { context, index });
        }

        Self {
            chrono_stream,
            reverse_lookup,
        }
    }

    pub fn len(&self) -> usize {
        self.chrono_stream.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chrono_stream.is_empty()
    }

    pub fn contains(&self, context: &InputContext) -> bool {
        self.reverse_lookup.contains_key(context)
    }

    pub fn chrono_index(&self, target: &StreamIndex) -> Option<usize> {
        self.reverse_lookup
            .get(&target.context)
            .and_then(|lookup| lookup.get(target.index).or_else(|| lookup.last()).copied())
    }

    pub fn stream_range(
        &self,
        context: &InputContext,
        chrono_range: &Range<usize>,
    ) -> Option<Range<usize>> {
        self.reverse_lookup.get(context).map(|entry| {
            let start = entry
                .binary_search(&chrono_range.start)
                .unwrap_or_else(|index| index);

            let end = if chrono_range.is_empty() {
                // reuse start for empty range
                start
            } else {
                // binary search from start
                start
                    + entry[start..]
                        .binary_search(&chrono_range.end)
                        .unwrap_or_else(|index| index)
            };

            start..end
        })
    }

    pub fn skip_until(&self, target: &StreamIndex) -> impl Iterator<Item = &StreamIndex> {
        let index = self.chrono_index(target);

        self.chrono_stream
            .iter()
            .skip(index.unwrap_or(self.chrono_stream.len()))
    }

    pub fn next_target(&self, target: &StreamIndex) -> Option<StreamIndex> {
        self.skip_until(target).nth(1).cloned()
    }

    pub fn find_irq_chrono_indices(&self) -> Vec<usize> {
        // returns sorted vec of irq indices
        let mut idx_vec: Vec<usize> = Vec::new();
        for (context, indices) in &self.reverse_lookup {
            match context.context() {
                StreamContext::Interrupt => idx_vec = idx_vec.iter().merge(indices.iter()).cloned().collect(),
                _ => {}
            }
        }
        assert!(idx_vec.is_sorted());
        idx_vec
        
    }

    pub fn find_irq_chrono_windows(&self) -> Vec<(usize, usize)> {
        // returns (irq_chrono_start, irq_chrono_end + 1). So you can use ret.0..ret.1
        let start_indices = self.find_irq_chrono_indices();
        let mut windows: Vec<(usize, usize)> = start_indices.windows(2)
            .map(|w| (w[0], w[1]))
            .collect();
        if let Some(&last) = start_indices.last() {
            windows.push((last, self.len()));
        }
        windows
    }

    pub fn find_irq_none_zero_irqs(&self, input: &InputFile) -> Vec<usize> {
        let irq_windows = self.find_irq_chrono_windows();
        // if there are no interrupts, return an empty vector
        if irq_windows.is_empty() {
            return Vec::new();
        }

        let mut final_vec = vec![];
        for (current_irq_start, _current_irq_end) in &irq_windows {
            let stream_entry = &self.chrono_stream[*current_irq_start];
            let stream = input.input_streams()
                .get(&stream_entry.context)
                .expect("get stream");

            if let InputValue::InterruptChoice { interval, .. } = &stream.as_ref()[stream_entry.index] {
                let end_reached = match interval {
                    Some(val) => *val >= MINIMUM_INPUT_WINDOW_INTERVAL,
                    None => true,
                };

                if end_reached {
                    final_vec.push(*current_irq_start);
                }
            }
        }
        final_vec
    }

    pub fn find_idc_none_zero_irqs(&self, input: &InputFile) -> Vec<usize> {
        let irq_windows = self.find_irq_chrono_windows();
        // if there are no interrupts, return an empty vector
        if irq_windows.is_empty() {
            return Vec::new();
        }

        let mut final_vec = vec![];
        for (idx, (current_irq_start, _end)) in irq_windows.iter().enumerate() {
            let stream_entry = &self.chrono_stream[*current_irq_start];
            let stream = input.input_streams()
                .get(&stream_entry.context)
                .expect("get stream");

            if let InputValue::InterruptChoice { interval, .. } = &stream.as_ref()[stream_entry.index] {
                match interval {
                    Some(val) => { 
                        if *val >= MINIMUM_INPUT_WINDOW_INTERVAL {
                            final_vec.push(idx);
                        }
                    },
                    None => {},
                };

            }
        }
        final_vec
    }


    pub fn find_message_chrono_windows(&self, input: &InputFile) -> Vec<(usize, usize)> {
        // returns (message_chrono_start, message_chrono_end + 1). So you can use ret.0..ret.1
        let irq_windows = self.find_irq_chrono_windows();
        
        // if there are no interrupts, return an empty vector
        if irq_windows.is_empty() {
            return Vec::new();
        }
        
        let mut message_windows = vec![];
        let mut current_window = (None,None);

        for (current_irq_start, current_irq_end) in &irq_windows {
            if current_window.0.is_none() {
                current_window.0 = Some(current_irq_start);
            }

            let stream_entry = &self.chrono_stream[*current_irq_start];
            let stream = input.input_streams()
                .get(&stream_entry.context)
                .expect("get stream");

            if let InputValue::InterruptChoice { interval, .. } = &stream.as_ref()[stream_entry.index] {
                let end_reached = match interval {
                    Some(val) => *val >= MINIMUM_INPUT_WINDOW_INTERVAL,
                    None => true,
                };

                if end_reached {
                    current_window.1 = Some(current_irq_end);
                    message_windows.push(mem::take(&mut current_window));
                }
            }
        }
        if current_window.0.is_some() {
            current_window.1 = Some(
                &irq_windows.last().expect("get last").1
            );
            message_windows.push(current_window);
        }
        message_windows.into_iter()
            .map(|(start, end)| (*start.unwrap(), *end.unwrap()))
            .collect()
    }


    pub fn find_message_window_end_indices(&self, input: &InputFile) -> Vec<usize> {
        self.get_input_segment_window_indices_helper(input, false)
    }

    fn get_input_segment_window_indices_helper(&self, input: &InputFile, start: bool) -> Vec<usize> {
        let irq_indices = self.find_irq_chrono_indices();
        
        // if there are no interrupts, return an empty vector
        if irq_indices.is_empty() {
            return Vec::new();
        }

        // The first input window always starts at the first irq?
        // let mut input_window_indices = vec![irq_indices[0]];
        let mut input_window_indices = vec![];

        // Input windows start after an interrupt with a long interval
        for window in irq_indices.windows(2) {
            let current_irq_idx = window[0];
            let next_irq_idx = window[1];

            let stream_entry = &self.chrono_stream[current_irq_idx];
            let stream = input.input_streams()
                .get(&stream_entry.context)
                .expect("get stream");

            if let InputValue::InterruptChoice { interval, .. } = &stream.as_ref()[stream_entry.index] {
                let should_pick_next = match interval {
                    Some(val) => *val >= MINIMUM_INPUT_WINDOW_INTERVAL,
                    None => true,
                };

                if should_pick_next {
                    if start {
                        input_window_indices.push(next_irq_idx);
                    } else {
                        input_window_indices.push(current_irq_idx);
                    }
                }
            }
        }
        input_window_indices
    }

    pub fn remove_head_at_chrono(&self, mut file: InputFile, chrono_idx: usize) -> InputFile {
        // removes all inputs prior to the presceding index

        let start = 0;
        let end = chrono_idx;
        let chrono_range = start..end;

        // erase stream ranges
        for (context, stream) in file.input_streams_mut() {
            if stream.is_empty() {
                continue;
            }

            if let Some(range) = self.stream_range(context, &chrono_range) {
                let range = range.start.min(stream.len())..range.end.min(stream.len());

                stream.as_mut().splice(range, iter::empty());
            }
        }

        file
    } 


}

impl AsRef<[StreamIndex]> for ChronoStream {
    fn as_ref(&self) -> &[StreamIndex] {
        &self.chrono_stream
    }
}
