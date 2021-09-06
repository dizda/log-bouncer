use chrono::Utc;
use std::fs;
use std::fs::File;
use std::time::Duration;

/// Rotator has 2 missions
///   1. Rotate at launch if target file exists
///   2. Check periodically if file is larger than defined size then rotate
///
/// The rotate will rename the file from `input.log` to `input-%Y-%m-%d-%H-%M-%S.log`
pub struct Rotator {
    filename: String,
    interval: Duration,
    date_format: String,
}

impl Rotator {
    pub fn new(filename: &str, interval: Duration, date_format: Option<String>) -> Self {
        Self {
            filename: filename.to_owned(),
            date_format: date_format.unwrap_or_else(|| "%Y-%m-%d-%H-%M-%S".to_string()),
            interval,
        }
    }

    fn rotate(&self) {
        // do something
        let now = Utc::now();
        let dt_str = now.format(&self.date_format).to_string();
        debug!("{}", dt_str);
        let new_filename = format!("{}.{}", self.filename, dt_str);

        fs::rename(&self.filename, new_filename);

        // todo: return error if can't rename
        // self.file = Some(File::create(&self.basename)?);
    }

    pub fn watch(self) {
        tokio::spawn(async move { self.work().await });
    }

    async fn work(&self) {
        info!(
            "Will check for file rotation every {}ms",
            self.interval.as_millis()
        );
        let mut interval = tokio::time::interval(self.interval);

        // first tick completes immediately
        interval.tick().await;

        loop {
            interval.tick().await;
            debug!("Tick: do a job");

            // do job
            self.rotate();
            debug!("Tick: lap")
        }
    }
}
