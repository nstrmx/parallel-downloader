use std::{
    fs::{remove_file, File}, 
    io::{Read,Write}, 
    path::PathBuf, 
    sync::Arc, 
    thread, 
    time::Duration,
};
use anyhow::{bail, Context, Result};
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
    file_name: PathBuf,
    chunk_size: usize,
    max_workers: usize,
}

impl Downloader {
    pub fn new(url: Url, file_name: PathBuf, chunk_size: usize, max_workers: usize) -> Downloader {
        Downloader {
            url,
            file_name,
            chunk_size,
            max_workers,
        }
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

    fn save_chunk(&self, chunk: &Chunk, data: &[u8]) -> Result<()> {
        let chunk_file_name = format!("{}.chunk-{}", self.file_name.to_string_lossy(), chunk.id);
        let mut output_chunk = File::create(chunk_file_name)?;
        Ok(output_chunk.write_all(data)?)
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

    fn merge_chunk(&self, output_file: &mut File, chunk: &Chunk) -> Result<()> {
        let chunk_file_name = format!("{}.chunk-{}", self.file_name.to_string_lossy(), chunk.id);
        let mut chunk_file = File::open(&chunk_file_name)?;
        let mut data = Vec::new();
        let n = chunk_file.read_to_end(&mut data)?;
        let m = output_file.write(&data)?;
        if m < n {
            bail!(format!("error merging chunk id={}", chunk.id));
        }
        info!("merged chunk id={}, size={}", chunk.id, m);
        remove_file(chunk_file_name)?;
        Ok(())
    }
    
    pub fn run(self: Arc<Self>) -> Result<()> {
        // Derive number of chunks from content length
        let content_length = self.request_content_length()?;
        info!("content-length: {}", content_length);
        let num_chunks = content_length / self.chunk_size;
        info!("number of chunks: {}", num_chunks);
        info!("chunk size: {}", self.chunk_size);
        // Channels
        // let result_chan = SharedChannel::<Chunk>::new("result", self.max_workers * 2);
        let result_chan = SharedChannel::<Chunk>::new("result");
        // let task_chan = SharedChannel::<Option<Chunk>>::new("task", self.max_workers * 2);
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
            let start_byte = i * self.chunk_size;
            let end_byte = if i == num_chunks - 1 {
                content_length - 1
            } else {
                (i + 1) * self.chunk_size - 1
            };
            let chunk = Chunk{id: i, start: start_byte, end: end_byte, status: Status::Initial};
            chunks.push(chunk.clone());
            task_chan.send(Some(chunk))?;
        }
        // Receive chunks
        // Failed chunks are sent back to workers
        // Expected chunks are merged to output file
        let mut output_file = File::create(&self.file_name)?;
        let mut expected_id = 0;
        let mut ok_chunks = 0;
        while ok_chunks < num_chunks {
            let chunk = match result_chan.try_recv() {
                Ok(chunk) => chunk,
                _ => {
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
                    task_chan.send(Some(chunk.clone()))?;
                }
            }
            if let Status::Downloaded = chunks[expected_id].status {
                self.merge_chunk(&mut output_file, &chunks[expected_id])?;
                expected_id += 1;
            }
        }
        // Merge the rest
        for chunk in &chunks[expected_id..num_chunks] {
            if let Status::Downloaded = chunk.status {
                self.merge_chunk(&mut output_file, chunk)?;
                expected_id += 1;
            }
        }
        // Send stop and join workers
        for _worker in workers.iter() {
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
