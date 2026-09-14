use std::{fmt::Debug, num::NonZeroUsize, usize};

use anyhow::{Context, Result};
use frametracer::{symbolizer::Symbolizer, Address};
use itertools::Itertools;
use modeling::{
    hardware::{Hardware, Input, Interrupt},
    input::{
        value::{InputValue, InputValueType},
        InputContext, StreamContext,
    },
};
use serde::{Deserialize, Serialize};

use crate::{arch::ArchEmulator, hooks::HookTarget, EmulatorCounts};

#[derive(Debug, Clone)]
pub struct EmulatorInterruptConfig {
    mode: InterruptMode,
    trigger: InterruptTrigger,
    allowlist: Option<Vec<Interrupt>>,
    blocklist: Option<Vec<Interrupt>>,
    index: usize,
    last_interrupt: usize,
    current_interrupt_interval: Option<u32>,
    last_isr_return: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct EmulatorInterruptConfigSnapshot {
    index: usize,
    last_interrupt: usize,
}

pub enum InterruptReadResult {
    Val(Interrupt),
    NoneAvaliable,
    EndOfStream,
}

impl From<TargetInterruptConfig> for EmulatorInterruptConfig {
    fn from(config: TargetInterruptConfig) -> Self {
        // let current_interrupt_interval = 
        //     match config.trigger.interval {
        //         TargetInterruptInterval::Enabled(_) => None,
        //         TargetInterruptInterval::BasicBlock(bbs) => Some(bbs as u16),
        //     };
        Self {
            mode: config.mode,
            trigger: config.trigger.into(),
            allowlist: config.allowlist,
            blocklist: config.blocklist,
            index: 0,
            last_interrupt: 0,
            current_interrupt_interval: None,
            last_isr_return: None,
        }
    }
}

impl From<TargetInterruptTrigger> for InterruptTrigger {
    fn from(trigger: TargetInterruptTrigger) -> Self {
        Self {
            on_infinite_sleep: trigger.on_infinite_sleep,
            interval: trigger.interval.into(),
            custom: trigger.custom.unwrap_or_default(),
        }
    }
}

impl EmulatorInterruptConfig {
    pub(crate) fn custom_trigger(&self, symbolizer: &Symbolizer) -> Result<Vec<Address>> {
        self.trigger
            .custom
            .iter()
            .map(|trigger| trigger.target.resolve(symbolizer))
            .flatten_ok()
            .collect::<Result<Vec<_>>>()
            .context("Failed to resolve custom interrupt trigger addresses")
    }

    pub(crate) fn next_interval(&self, counts: &EmulatorCounts) -> Option<(usize, bool)> {
        let fallback_interval = || {
            self.trigger
                .interval
                .map(|interval| (interval.get() - (counts.basic_block - self.last_interrupt), false))
        };

        if let Some(current_interrupt_interval) = self.current_interrupt_interval {
            if let Some(last_isr_return) = self.last_isr_return {
                if (last_isr_return + current_interrupt_interval as usize) < counts.basic_block {
                    fallback_interval()
                } else {
                    let interval = current_interrupt_interval as usize - (counts.basic_block - last_isr_return);
                    // log::info!("Next interval set to interrupt: {}", interval);
                    Some((interval, true))
                }
            } else {
                // the isr has not returned yet. return default
                fallback_interval()
            }
        } else {
            // no interval set by interrupt, use default instead
            fallback_interval()
        }
    }

    pub(crate) fn first_interrupt_interval<I: Input + Debug>(&mut self, counts: &EmulatorCounts, hardware: &mut Hardware<I>) -> () {
        assert!(self.current_interrupt_interval.is_none());
        let int = hardware
            .input_read(InputContext::new(
                StreamContext::Interrupt,
                InputValueType::InterruptChoice(0),
            ));
        if let Some(int) = int {            
            match int.as_ref() {
                InputValue::InterruptChoice { interval, .. } => {
                    // log::info!("Read int from input. setting current_interrupt_interval to {:?} and last_isr_return to 0", interval);
                    self.current_interrupt_interval = interval.clone();
                    self.last_isr_return = Some(counts.basic_block());
                },
                _ => unreachable!(),
            }
        };
    }

    pub(crate) fn set_last_interval_at_stop<I: Input + Debug>(&mut self, hardware: &mut Hardware<I>, counts: &EmulatorCounts) {
        // set the inputValue stored in inputstream to the correct interval
        if let Some(last_isr_return) = self.last_isr_return {
            // log::info!("setting inteval at stop");

            hardware.set_last_interrupt_interval(
                Some((counts.basic_block() - last_isr_return + 1) as u32)
            );
        } else {
            hardware.set_last_interrupt_interval(
                None // exited before the ISR returned
            );
        }
    }

    pub(crate) fn next_interrupt<I: Input + Debug>(
        &mut self,
        arch: &ArchEmulator,
        counts: &EmulatorCounts,
        hardware: &mut Hardware<I>,
        force_raise: bool,
    ) -> InterruptReadResult {
        let irqs = arch.available_interrupts(force_raise);
        log::trace!("available irqs = {:?}", irqs);

        let irqs = self.apply_filter(irqs);
        log::trace!("available filtered irqs = {:?}", irqs);

        // set last interrupt (even when no interrupt will be raised)
        self.last_interrupt = counts.basic_block;

        if irqs.is_empty() {
            // log::info!("No available interrupts!");
            self.current_interrupt_interval = None;
            return InterruptReadResult::NoneAvaliable;
        }

        // set the inputValue stored in inputstream to the correct interval
        if let Some(last_isr_return) = self.last_isr_return {
            hardware.set_last_interrupt_interval(
                Some((counts.basic_block() - last_isr_return) as u32)
            );
        } else {
            // this can happen when the interrupt has an infinite loop (or when it never triggers, but that shouldnt happen)
            // log::info!("WTF previous ISR did not return");
        }


        match self.mode {
            InterruptMode::Disabled => InterruptReadResult::NoneAvaliable,
            InterruptMode::RoundRobin => {
                let irq = irqs[self.index % irqs.len()];
                self.index += 1;
                InterruptReadResult::Val(irq)
            }
            InterruptMode::Fuzzed => {
                // if irqs.len() == 1 {
                //     // only one interrupt available
                //     Some(irqs[0])
                // } else {
                // use fuzzer input

                let int = hardware
                    .input_read(InputContext::new(
                        StreamContext::Interrupt,
                        InputValueType::InterruptChoice(irqs.len() as u8),
                    ));
                if let Some(int) = int {
                    // Set interval from trigger
                    // let interval = self.trigger.interval.map(|v| v.get() as u16);
                    // int.to_mut().set_interval(interval);
                    
                    match int.as_ref() {
                        InputValue::InterruptChoice { index, interval, .. } => {
                            // log::info!("Read int from input. setting current_interrupt_interval to {:?}", interval);
                            self.current_interrupt_interval = interval.clone();
                            self.last_isr_return = None;

                            InterruptReadResult::Val(irqs[*index as usize])
                        },
                        _ => unreachable!(),
                    }
                }
                else {
                    InterruptReadResult::EndOfStream
                }
            }
        }
    }

    fn apply_filter(&self, irqs: Vec<Interrupt>) -> Vec<Interrupt> {
        // no filters
        if self.blocklist.is_none() && self.allowlist.is_none() {
            return irqs;
        }

        irqs.into_iter()
            .filter(|irq| {
                // filter when in blocklist
                if let Some(blocklist) = &self.blocklist {
                    if blocklist.contains(irq) {
                        return false;
                    }
                }

                // allowlist exists
                if let Some(allowlist) = &self.allowlist {
                    // filter when not in allowlist
                    if !allowlist.contains(irq) {
                        return false;
                    }
                }

                true
            })
            .collect()
    }

    pub fn activate_interval_count(&mut self, counts: &EmulatorCounts) -> bool {
        // called when ISR exits
        if self.last_isr_return.is_none() { // only the first return after interrupt
            // log::info!("Activate interval count. Setting last_isr_return to {} (curr int int {:?})", counts.basic_block(), self.current_interrupt_interval);
            self.last_isr_return = Some(counts.basic_block());
            true
        }
        else {
            false
        }
    }

    pub fn snapshot_create(&self) -> EmulatorInterruptConfigSnapshot {
        EmulatorInterruptConfigSnapshot {
            index: self.index,
            last_interrupt: self.last_interrupt,
        }
    }

    pub fn snapshot_restore(&mut self, snapshot: &EmulatorInterruptConfigSnapshot) {
        self.index = snapshot.index;
        self.last_interrupt = snapshot.last_interrupt;
        self.current_interrupt_interval = None; //snapshot.current_interrupt_interval;
        self.last_isr_return = None;
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InterruptMode {
    Disabled,
    RoundRobin,
    Fuzzed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct InterruptTrigger {
    on_infinite_sleep: bool,
    interval: Option<NonZeroUsize>,
    custom: Vec<CustomInterruptTrigger>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TargetInterruptConfig {
    #[serde(default = "Default::default")]
    mode: InterruptMode,
    #[serde(default = "Default::default")]
    trigger: TargetInterruptTrigger,
    #[serde(alias = "whitelist")]
    allowlist: Option<Vec<Interrupt>>,
    #[serde(alias = "blacklist")]
    blocklist: Option<Vec<Interrupt>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TargetInterruptTrigger {
    #[serde(default = "default_on_infinite_sleep")]
    on_infinite_sleep: bool,
    #[serde(default = "default_interval")]
    interval: TargetInterruptInterval,
    custom: Option<Vec<CustomInterruptTrigger>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TargetInterruptInterval {
    Enabled(bool),
    BasicBlock(usize),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CustomInterruptTrigger {
    name: Option<String>,
    #[serde(flatten)]
    target: HookTarget,
    // # mode:
    // # allowlist / blocklist
}

impl Default for InterruptMode {
    fn default() -> Self {
        Self::RoundRobin
    }
}

impl Default for TargetInterruptTrigger {
    fn default() -> Self {
        Self {
            on_infinite_sleep: default_on_infinite_sleep(),
            interval: default_interval(),
            custom: None,
        }
    }
}

fn default_on_infinite_sleep() -> bool {
    true
}

fn default_interval() -> TargetInterruptInterval {
    TargetInterruptInterval::BasicBlock(1_000)
}

impl TargetInterruptConfig {
    pub fn new(
        mode: InterruptMode,
        trigger: TargetInterruptTrigger,
        allowlist: Option<Vec<Interrupt>>,
        blocklist: Option<Vec<Interrupt>>,
    ) -> Self {
        Self {
            mode,
            trigger,
            allowlist,
            blocklist,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if !matches!(self.mode, InterruptMode::Fuzzed) {
            anyhow::bail!("interrupt.mode must be 'fuzzed', got {:?}", self.mode);
        }
        if self.trigger.on_infinite_sleep || self.trigger.custom.is_some() {
            anyhow::bail!("interrupt.trigger.on-infinite-sleep and custom triggers must be false");
        }
        Ok(())
    }
}

impl TargetInterruptTrigger {
    pub fn new(
        on_infinite_sleep: bool,
        interval: TargetInterruptInterval,
        custom: Option<Vec<CustomInterruptTrigger>>,
    ) -> Self {
        Self {
            on_infinite_sleep,
            interval,
            custom,
        }
    }
}

impl Into<Option<NonZeroUsize>> for TargetInterruptInterval {
    fn into(self) -> Option<NonZeroUsize> {
        match self {
            TargetInterruptInterval::Enabled(enabled) if !enabled => None,
            TargetInterruptInterval::Enabled(_) => default_interval().into(),
            TargetInterruptInterval::BasicBlock(interval) => NonZeroUsize::new(interval),
        }
    }
}

impl CustomInterruptTrigger {
    pub fn new(name: Option<String>, target: HookTarget) -> Self {
        Self { name, target }
    }
}
