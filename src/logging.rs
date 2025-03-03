use::std::path::PathBuf;
use anyhow::Result;
use log::LevelFilter;
use log4rs::{
    append::{
        console::{ConsoleAppender, Target},
        file::FileAppender,
    },
    encode::pattern::PatternEncoder,
    config::{Appender, Config, Root},
    filter::threshold::ThresholdFilter,
};

pub fn build_logger(log_level: log::LevelFilter, log_path: Option<PathBuf>) -> Result<log4rs::Handle> {
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
            .build(&log_path)?;
        let appender_name = "log_file";
        config
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
            )?
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
        )?
    };
    Ok(log4rs::init_config(config)?)
}
