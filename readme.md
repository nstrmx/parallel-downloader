# Parallel downloader
Simple command line utility for parallel downloading written in rust for learing purposes.
Notice: work in progress, use at your own risk.

## Features
* chunked multithreaded downloading 
* get request
* retry failed chunks
* specify number of workers
* specify chunk size
* logging

## TODO features
* load urls from csv
* check hashsum
* default logging path to /var/log/
* async version
* different methods like get and post
* pass custom headers
* timeouts for chunks
* timeout for the whole download request
* max retries for a chunk
* max retries for the whole download request
* download individual chunks mode
* merge individual chunks mode
* validate urls
* validate chunks
* improved error handling
* check support for Range header
* python support
* extract file name from url
* tests
* proxies