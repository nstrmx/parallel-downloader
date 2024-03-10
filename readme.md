# Parallel downloader
Simple command line utility for parallel downloading written in rust for learing purposes.
Notice: work in progress, use at your own risk.

## Features 0.1.0
* chunked multithreaded downloading 
* get request
* retry failed chunks
* specify number of workers
* specify chunk size
* logging

## TODO features 0.2.0
* load urls from csv
* extract file name from url
* validate urls
* check support for Range header
* default logging path to /var/log/
* different methods like get and post

## TODO features 0.3.0
* timeout for the whole download request
* max retries for the whole download request
* download individual chunks mode
* merge individual chunks mode
* max retries for a chunk
* timeouts for chunks
* validate chunks

## TODO features 0.4.0
* check hashsum
* pass custom headers
* proxies
* improved error handling
* python support
* async version
* tests