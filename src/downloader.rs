use std::{
    fs::{remove_file, File}, 
    io::{Read,Write}, 
    path::PathBuf, 
    sync::Arc, 
    thread, 
    time::Duration,
};
use log::{debug, error, info};
use crate::channel::SharedChannel;


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

pub struct Downloader {
    url: String,
    file_name: String,
    chunk_size: usize,
    max_workers: usize,
}

impl Downloader {
    pub fn new(url: String, file_name: PathBuf, chunk_size: usize, max_workers: usize) -> Downloader {
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
                    }
                };
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

    fn start_worker(shared_self: Arc<Self>, id: usize, task_chan: SharedChannel<Option<Chunk>>, result_chan: SharedChannel<Chunk>) -> thread::JoinHandle<()> {
        return thread::spawn(move || {
            loop {
                let response = task_chan.recv().unwrap();
                if let Some(mut chunk) = response {
                    debug!("worker id={} recieved chunk: {:?}", id, chunk);
                    shared_self.download_chunk(&mut chunk);
                    result_chan.send(chunk).unwrap();
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
                        info!("merged chunk id={}, size={}", chunk.id, m);
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
    
    pub fn run(self) {
        // Derive number of chunks from content length
        let content_length = self.request_content_length();
        info!("content-length: {}", content_length);
        let num_chunks = content_length / self.chunk_size;
        info!("number of chunks: {}", num_chunks);
        info!("chunk size: {}", self.chunk_size);
        let shared_self = Arc::new(self);
        // Channels
        let result_chan = SharedChannel::<Chunk>::new("result");
        let task_chan = SharedChannel::<Option<Chunk>>::new("task");
        //Start workers
        info!("number of workers: {}", shared_self.max_workers);
        let mut workers = Vec::with_capacity(shared_self.max_workers);
        for i in 0..shared_self.max_workers {
            let worker = Self::start_worker(shared_self.clone(), i, task_chan.clone(), result_chan.clone());
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
            task_chan.send(Some(chunk)).unwrap();
        }
        // Receive chunks
        // Failed chunks are sent back to workers
        // Expected chunks are merged to output file
        let mut output_file = File::create(&shared_self.file_name).expect("failed to create file");
        let mut expected_id = 0;
        let mut ok_chunks = 0;
        while ok_chunks < num_chunks {
            let chunk = match result_chan.try_recv() {
                Some(chunk) => chunk,
                None => {
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
                    task_chan.send(Some(chunk.clone())).unwrap();
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
            task_chan.send(None).unwrap();
        }
        for worker in workers {
            worker.join().unwrap();
        }
    }
}