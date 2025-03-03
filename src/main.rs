use std::{num::NonZero, path::PathBuf, time::Instant};
use anyhow::Result;
use structopt::StructOpt;
use::log::info;
use downloader::Downloader;
use logging::build_logger;
mod channel;
mod downloader;
mod logging;


#[derive(Debug, StructOpt)]
#[structopt(name = "parallel downloader", about = "An implementation of a configurable parallel downloader.")]
struct Opt {
    #[structopt(short = "v", long, parse(from_occurrences))]
    verbose: u8,

    #[structopt(short, long, parse(from_os_str))]
    log_path: Option<PathBuf>,

    #[structopt(short, long)]
    url: url::Url,

    #[structopt(short, long, parse(from_os_str))]
    filename: PathBuf,

    #[structopt(short, long)]
    workers: Option<usize>,
}


fn main() -> Result<()> {
    let now = Instant::now();
    let opt = Opt::from_args();
    // Logging
    let log_level = match opt.verbose {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        2 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };
    build_logger(log_level, opt.log_path)?;
    // Workers
    let workers = opt.workers.unwrap_or(
        std::thread::available_parallelism()
            .unwrap_or(NonZero::new(8).unwrap())
            .get()
    );
    // Let's go
    Downloader::new(opt.url, opt.filename, workers)?.run()?;
    let elapsed = Instant::now() - now;
    info!("elapsed = {}", elapsed.as_secs());
    Ok(())
}
