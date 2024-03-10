use std::{path::PathBuf, time::Instant};
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
    url: String,

    #[structopt(short, long, parse(from_os_str))]
    file_name: PathBuf,

    #[structopt(short, long)]
    chunk_size: Option<String>,

    #[structopt(short, long)]
    workers: Option<usize>,
}


fn main() {
    let now = Instant::now();
    let opt = Opt::from_args();
    // Logging
    let log_level = match opt.verbose {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        2 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };
    build_logger(log_level, opt.log_path);
    // Chunk size
    let re = Regex::new(r"(\d+)([Mm][Bb])").unwrap();
    let default_chunk_size = 1024 * 1024 * 10;
    let chunk_size = match opt.chunk_size {
        Some(text) => {
            if let Some(captures) = re.captures(&text) {
                let number = captures.get(1).unwrap().as_str().parse::<usize>().unwrap();
                number * 1024 * 1024
            } else {
                default_chunk_size
            }
        }
        None => {
            default_chunk_size
        }
    };
    // Workers
    let workers = match opt.workers {
        Some(val) => val,
        None => 8
    };
    // Let's go
    let downloader = Downloader::new(opt.url, opt.file_name, chunk_size, workers);
    downloader.run();
    let elapsed = Instant::now() - now;
    info!("elapsed = {}", elapsed.as_secs());
}