use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use common::{
    fs::decoder,
    log::{init_log, LOG_INFO},
};
use frametracer::{Symbolizable, TraceEvent, TraceIter};

#[derive(Parser, Debug)]
#[command(name = "hoedur-print-trace")]
pub struct Arguments {
    #[arg(long, default_value = LOG_INFO)]
    pub log_config: PathBuf,

    /// Only print BasicBlock events (skip Access, Exception, etc.)
    #[arg(long)]
    pub bb_only: bool,

    /// Trace file (zstd-compressed)
    pub trace: PathBuf,
}

fn main() -> Result<()> {
    let opt = Arguments::parse();

    init_log(&opt.log_config)?;

    let mut stream = decoder(&opt.trace)
        .with_context(|| format!("Failed to open trace file {:?}", opt.trace))?;

    for trace in TraceIter::new(&mut stream) {
        let trace = trace.context("Failed to read trace")?;

        for event in &trace.events {
            match event {
                TraceEvent::BasicBlock(bb) => {
                    println!("BB pc={:#010x} ra={:#010x}", bb.pc, bb.ra);
                }
                _ if !opt.bb_only => {
                    println!("{}", event.display());
                }
                _ => {}
            }
        }
    }

    Ok(())
}
