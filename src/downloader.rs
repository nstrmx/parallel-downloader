use std::{
    fs::{File, OpenOptions}, 
    io::{Read, Seek, Write}, 
    path::PathBuf, 
    sync::{Arc, Mutex},
    thread, 
};
use anyhow::{Context, Result};
use log::{debug, error, info};
use url::Url;
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
    url: Url,
    file: Mutex<File>,
    max_workers: usize,
}

impl Downloader {
    pub fn new(url: Url, filename: PathBuf, max_workers: usize) -> Result<Arc<Downloader>> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(filename)?;
        Ok(Arc::new(Downloader {
            url,
            file: Mutex::new(file),
            max_workers,
        }))
    }

    fn request_content_length(&self) -> Result<usize> {
        Ok(ureq::get(self.url.as_str())
            .call()?
            .header("content-length").context("content-length header not found")?
            .parse::<usize>()?
        )
    }

    fn download_chunk(&self, chunk: &mut Chunk) {
        match ureq::get(self.url.as_str())
            .set("Range", &format!("bytes={}-{}", chunk.start, chunk.end))
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

    fn save_chunk(&self, chunk: &Chunk, data: &[u8]) -> Result<()> {
        let mut file = self.file.lock().ok().context("failed to lock file")?;
        file.seek(std::io::SeekFrom::Start(chunk.start as u64))?;
        file.write_all(data)?;
        info!("merged chunk id={}, size={}", chunk.id, chunk.end - chunk.start);
        Ok(())
    }

    fn start_worker(self: Arc<Self>, id: usize, task_chan: SharedChannel<Option<Chunk>>, result_chan: SharedChannel<Chunk>) -> thread::JoinHandle<Result<()>> {
        thread::spawn(move || -> Result<()> {
            loop {
                if let Some(mut chunk) = task_chan.recv()? {
                    debug!("worker id={} recieved chunk: {:?}", id, chunk);
                    self.download_chunk(&mut chunk);
                    result_chan.send(chunk)?;
                } else {
                    debug!("worker id={} recieved stop", id);
                    break;
                }
            }
            Ok(())
        })
    }

    pub fn run(self: Arc<Self>) -> Result<()> {
        // Derive number of chunks from content length
        let content_length = self.request_content_length()?;
        info!("content-length: {}", content_length);
        self.file.lock().ok().context("failed to lock file")?
            .set_len(content_length as u64)?;
        let num_chunks = self.max_workers;
        info!("number of chunks: {}", num_chunks);
        let chunk_size = content_length / num_chunks;
        info!("chunk size: {}", chunk_size);
        // Channels
        let result_chan = SharedChannel::<Chunk>::new("result");
        let task_chan = SharedChannel::<Option<Chunk>>::new("task");
        //Start workers
        info!("number of workers: {}", self.max_workers);
        let mut workers = Vec::with_capacity(self.max_workers);
        for i in 0..self.max_workers {
            let worker = Self::start_worker(self.clone(), i, task_chan.clone(), result_chan.clone());
            workers.push(worker);
        }
        // Send tasks
        let mut chunks = Vec::with_capacity(num_chunks);
        info!("downloading chunks");
        for i in 0..num_chunks {
            let start_byte = i * chunk_size;
            let end_byte = if i == num_chunks - 1 {
                content_length - 1
            } else {
                (i + 1) * chunk_size - 1
            };
            let chunk = Chunk{id: i, start: start_byte, end: end_byte, status: Status::Initial};
            chunks.push(chunk.clone());
            task_chan.send(Some(chunk))?;
        }
        // Send stop and join workers
        for _ in 0..workers.len() {
            task_chan.send(None)?;
        }
        for worker in workers {
            if let Err(e) = worker.join() {
                error!("error joining worker: {e:?}");
            };
        }
        Ok(())
    }
}
