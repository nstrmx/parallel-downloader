use std::{path::PathBuf, sync::Arc, time::Instant};
use anyhow::{Context, Result};
use structopt::StructOpt;
use regex::Regex;
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
    file_name: PathBuf,

    #[structopt(short, long)]
    chunk_size: Option<String>,

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
    // Chunk size
    let re = Regex::new(r"(\d+)([Mm][Bb])")?;
    let default_chunk_size = 1024 * 1024 * 10;
    let mut chunk_size = default_chunk_size;
    if let Some(text) = opt.chunk_size {
        if let Some(captures) = re.captures(&text) {
            let number = captures
                .get(1).context("invalid chunk size format")?
                .as_str().parse::<usize>()?;
            chunk_size = number * 1024 * 1024;
        }
    };
    // Workers
    let workers = opt.workers.unwrap_or(8);
    // Let's go
    let downloader = Arc::new(Downloader::new(
        opt.url, opt.file_name, chunk_size, workers
    ));
    downloader.run()?;
    let elapsed = Instant::now() - now;
    info!("elapsed = {}", elapsed.as_secs());
    Ok(())
}
