use std::{
    fs::{remove_file, File}, 
    io::{Read,Write}, 
    path::PathBuf, 
    sync::{mpsc::{channel, Receiver, Sender}, Arc, Mutex}, 
    thread, 
    time::{Duration, Instant}
};
use structopt::StructOpt;
use regex::Regex;
use log::{debug, error, info, LevelFilter};
use log4rs::{
    append::{
        console::{ConsoleAppender, Target},
        file::FileAppender,
    },
    encode::pattern::PatternEncoder,
    config::{Appender, Config, Root},
    filter::threshold::ThresholdFilter,
};


#[derive(Debug, Clone)]
enum Status {
    Initial,
    Downloaded,
}

#[derive(Debug, Clone)]
struct Chunk {
    id: usize,
    start: usize,
    end: usize,
    status: Status,
}

struct Downloader {
    url: String,
    file_name: String,
    chunk_size: usize,
    max_workers: usize,
}

impl Downloader {
    fn new(url: String, file_name: PathBuf, chunk_size: usize, max_workers: usize) -> Downloader {
        return Downloader {
            url: url.to_string(),
            file_name: String::from(file_name.to_str().unwrap()),
            chunk_size: chunk_size,
            max_workers: max_workers,
        }
    }

    fn request_content_length(&self) -> usize {
        return ureq::get(&self.url)
            .call().unwrap()
            .header("content-length").unwrap()
            .parse::<usize>().unwrap();
    }

    fn download_chunk(&self, chunk: &mut Chunk) {
        match ureq::get(&self.url)
            .set("Range", format!("bytes={}-{}", chunk.start, chunk.end).as_str())
            .call() 
        {
            Ok(response) => {
                let mut data = Vec::new();
                match response
                    .into_reader()
                    .read_to_end(&mut data)
                {
                    Ok(_) => (),
                    Err(err) => {
                        error!("response read error: {}", err);
                        chunk.status = Status::Initial;
                        return;
                    }
                }
                match self.save_chunk(chunk, &data) {
                    Ok(_) => {
                        chunk.status = Status::Downloaded;
                        debug!("downloaded chunk {:?}", chunk);
                    }
                    Err(err) => {
                        error!("chunk write error: {}", err);
                        chunk.status = Status::Downloaded;
                    }
                };
                chunk.status = Status::Downloaded;
            }
            Err(err) => {
                error!("request error: {}", err);
            }
        };  
    }

    fn save_chunk(&self, chunk: &Chunk, data: &Vec<u8>) -> Result<(), std::io::Error> {
        let chunk_file_name = format!("{}.chunk-{}", self.file_name, chunk.id);
        let mut output_chunk = File::create(chunk_file_name).expect("Failed to create file");
         output_chunk.write_all(&data)
    }

    fn start_worker(shared_self: Arc<Self>, id: usize, rx: Arc<Mutex<Receiver<Option<Chunk>>>>, tx: Arc<Mutex<Sender<Chunk>>>) -> thread::JoinHandle<()> {
        return thread::spawn(move || {
            loop {
                let response = match rx.lock().unwrap().recv() {
                    Ok(response) => response,
                    Err(err) => {
                        eprint!("worker id={} recv error: {}", id, err);
                        break;
                    }
                };
                if let Some(mut chunk) = response {
                    debug!("worker id={} recieved chunk: {:?}", id, chunk);
                    shared_self.download_chunk(&mut chunk);
                    tx.lock().unwrap().send(chunk).unwrap();
                } else {
                    debug!("worker id={} recieved stop", id);
                    break;
                }
            }
        });
    }

    fn merge_chunk(&self, output_file: &mut File, chunk: &Chunk) {
        let chunk_file_name = format!("{}.chunk-{}", self.file_name, chunk.id);
        let mut chunk_file = File::open(&chunk_file_name).expect("failed to create file");
        let mut data = Vec::new();
        match chunk_file.read_to_end(data.as_mut()) {
            Ok(_n) => {
                match output_file.write(&data) {
                    Ok(m) => {
                        // chunk.status = Status::Merged;
                        debug!("merged chunk id={}, size={}", chunk.id, m);
                    }
                    Err(err) => {
                        error!("chunk merge error: {}", err);
                    }
                }
            }
            Err(err) => {
                error!("chunk read error: {}", err);
            }
        }
        remove_file(chunk_file_name).unwrap();
    }
    
    fn run(self) {
        // Derive number of chunks from content length
        let content_length = self.request_content_length();
        info!("content-length: {}", content_length);
        let num_chunks = content_length / self.chunk_size;
        info!("number of chunks: {}", num_chunks);
        info!("chunk size: {}", self.chunk_size);
        let shared_self = Arc::new(self);
        // Result channel
        let (tx1, rx1) = channel::<Chunk>();
        let shared_tx1 = Arc::new(Mutex::new(tx1));
        let shared_rx1 = Arc::new(Mutex::new(rx1));
        // Task channel
        let (tx2, rx2) = channel::<Option<Chunk>>();
        let shared_tx2 = Arc::new(Mutex::new(tx2));
        let shared_rx2 = Arc::new(Mutex::new(rx2));
        //Start workers
        info!("number of workers: {}", shared_self.max_workers);
        let mut workers = Vec::with_capacity(shared_self.max_workers);
        for i in 0..shared_self.max_workers {
            let worker = Self::start_worker(shared_self.clone(), i, shared_rx2.clone(), shared_tx1.clone());
            workers.push(worker);
        }
        // Send tasks
        let mut chunks = Vec::with_capacity(num_chunks);
        info!("downloading chunks");
        for i in 0..num_chunks {
            let start_byte = i * shared_self.chunk_size;
            let end_byte = if i == num_chunks - 1 {
                content_length - 1
            } else {
                (i + 1) * shared_self.chunk_size - 1
            };
            let chunk = Chunk{id: i, start: start_byte, end: end_byte, status: Status::Initial};
            chunks.push(chunk.clone());
            shared_tx2.lock().unwrap().send(Some(chunk)).unwrap();
        }
        // Receive chunks
        // Failed chunks are sent back to workers
        // Expected chunks are merged to output file
        let mut output_file = File::create(&shared_self.file_name).expect("failed to create file");
        let mut expected_id = 0;
        let mut ok_chunks = 0;
        while ok_chunks < num_chunks {
            let chunk = match shared_rx1.lock().unwrap().try_recv() {
                Ok(chunk) => chunk,
                Err(_) => {
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
            };
            debug!("main thread recieved chunk: {:?}", chunk);
            match chunk.status {
                Status::Downloaded => {
                    chunks[chunk.id].status = Status::Downloaded;
                    ok_chunks += 1;
                }
                _ => {
                    shared_tx2.lock().unwrap().send(Some(chunk.clone())).unwrap();
                }
            }
            match chunks[expected_id].status {
                Status::Downloaded => {
                    shared_self.merge_chunk(&mut output_file, &chunks[expected_id]);
                    expected_id += 1;
                }
                _ => (),
            }
        }
        // Merge the rest
        for i in expected_id..num_chunks {
            match chunks[i].status {
                Status::Downloaded => {
                    shared_self.merge_chunk(&mut output_file, &chunks[i]);
                    expected_id += 1;
                }
                _ => {
                    error!("unexpected chunk status: {:?}", chunks[i]);
                },
            }
        }
        // Send stop and join workers
        for _worker in workers.iter() {
            shared_tx2.lock().unwrap().send(None).unwrap();
        }
        for worker in workers {
            worker.join().unwrap();
        }
    }
}


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


fn build_logger(log_level: log::LevelFilter, log_path: Option<PathBuf>) -> log4rs::Handle {
    // Build a stderr logger.
    let stderr = ConsoleAppender::builder().target(Target::Stderr).build();
    // Log Trace level output to file where trace is the default level
    // and the programmatically specified level to stderr.
    let config = Config::builder();
    let config = if let Some(log_path) = log_path {
        // Logging to log file.
        let log_file = FileAppender::builder()
            // Pattern: https://docs.rs/log4rs/*/log4rs/encode/pattern/index.html
            .encoder(Box::new(PatternEncoder::new("{l} {d} - {m}\n")))
            .build(&log_path)
            .unwrap();
        let appender_name = "log_file";
        let config = config
            .appender(Appender::builder().build(appender_name, Box::new(log_file)))
            .appender(
                Appender::builder()
                    .filter(Box::new(ThresholdFilter::new(log_level)))
                    .build("stderr", Box::new(stderr)),
            )
            .build(
                Root::builder()
                    .appender(appender_name)
                    .appender("stderr")
                    .build(LevelFilter::Trace),
            )
            .unwrap();
        config
    } else {
        config.appender(
            Appender::builder()
                .filter(Box::new(ThresholdFilter::new(log_level)))
                .build("stderr", Box::new(stderr)),
        )
        .build(
            Root::builder()
                .appender("stderr")
                .build(LevelFilter::Trace),
        )
        .unwrap()
    };
    return log4rs::init_config(config).unwrap();
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